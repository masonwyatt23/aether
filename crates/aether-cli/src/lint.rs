//! `aether lint` — static checks beyond what the type checker enforces.
//!
//! Lints are advisory; each emits a `Severity::Warning` diagnostic. The CLI's
//! exit code is non-zero only when `--deny-warnings` is passed.

use aether_ast::{Decl, Expr, Module, Pattern, Span, Stmt};
use aether_types::{Diagnostic, Severity};
use std::collections::HashSet;

/// Run every lint and return the resulting diagnostics.
pub fn lint_module(m: &Module) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    lint_missing_docstrings(m, &mut out);
    lint_unused_imports(m, &mut out);
    lint_unused_lets(m, &mut out);
    lint_overbroad_effects(m, &mut out);
    out
}

fn warn(span: Span, msg: impl Into<String>) -> Diagnostic {
    Diagnostic {
        severity: Severity::Warning,
        span,
        msg: msg.into(),
    }
}

// ── lints ────────────────────────────────────────────────────────────────────

/// Public-looking fns (no leading underscore, not `main`, not desugared `test_/bench_/snap_`)
/// should have a docstring.
fn lint_missing_docstrings(m: &Module, out: &mut Vec<Diagnostic>) {
    for d in &m.decls {
        if let Decl::Fn(f) = d {
            if f.doc.is_some() {
                continue;
            }
            if f.name == "main"
                || f.name.starts_with('_')
                || f.name.starts_with("test__")
                || f.name.starts_with("bench__")
                || f.name.starts_with("snap__")
            {
                continue;
            }
            out.push(warn(
                f.span,
                format!("function `{}` has no docstring (use `## …`)", f.name),
            ));
        }
    }
}

/// `import std::iter` whose imported names are never referenced anywhere.
fn lint_unused_imports(m: &Module, out: &mut Vec<Diagnostic>) {
    // Collect identifiers referenced in any decl body or signature default.
    let mut used = HashSet::<String>::new();
    for d in &m.decls {
        collect_used_idents(d, &mut used);
    }
    // For each Import with explicit names, warn if every selected name is unused.
    for d in &m.decls {
        if let Decl::Import(imp) = d {
            if imp.names.is_empty() {
                // Module-wide import; can't tell what's unused without resolving.
                continue;
            }
            let any_used = imp.names.iter().any(|n| used.contains(n));
            if !any_used {
                let path = imp.path.join("::");
                out.push(warn(
                    imp.span,
                    format!(
                        "import `{{ {} }} from {}` is unused",
                        imp.names.join(", "),
                        path
                    ),
                ));
            }
        }
    }
}

/// `let x = …` inside a block whose `x` is never referenced.
fn lint_unused_lets(m: &Module, out: &mut Vec<Diagnostic>) {
    for d in &m.decls {
        if let Decl::Fn(f) = d {
            walk_for_unused_lets(&f.body, out);
        }
    }
}

/// A function declares an effect that nothing in its body needs.
/// (Heuristic — without effect-inference flowing through call sites we just
/// flag effects that don't appear in any sub-expression's set of *named*
/// callees.)
fn lint_overbroad_effects(m: &Module, out: &mut Vec<Diagnostic>) {
    let needs_io = builtins_with_effect("IO");
    let needs_net = builtins_with_effect("Net");
    let needs_fs = builtins_with_effect("FS");
    let needs_throw = builtins_with_effect("Throw");
    let needs_rand = builtins_with_effect("Rand");
    let needs_state = builtins_with_effect("State");

    for d in &m.decls {
        if let Decl::Fn(f) = d {
            let mut calls = HashSet::<String>::new();
            collect_called_names(&f.body, &mut calls);
            for eff in &f.effects.effects {
                let label = eff.as_str();
                let dont_need = match label {
                    "IO" => calls.is_disjoint(&needs_io),
                    "Net" => calls.is_disjoint(&needs_net),
                    "FS" => calls.is_disjoint(&needs_fs),
                    "Throw" => calls.is_disjoint(&needs_throw),
                    "Rand" => calls.is_disjoint(&needs_rand),
                    "State" => calls.is_disjoint(&needs_state),
                    _ => false, // unknown / custom effect — don't lint
                };
                if dont_need && !f.name.starts_with("test__") && !f.name.starts_with("bench__") {
                    out.push(warn(
                        f.span,
                        format!(
                            "function `{}` declares effect `{}` but doesn't appear to use it",
                            f.name, label
                        ),
                    ));
                }
            }
        }
    }
}

// ── traversal helpers ────────────────────────────────────────────────────────

fn collect_used_idents(d: &Decl, out: &mut HashSet<String>) {
    match d {
        Decl::Fn(f) => collect_idents_in_expr(&f.body, out),
        Decl::Let(l) => collect_idents_in_expr(&l.value, out),
        Decl::TypeAlias(_) | Decl::Import(_) | Decl::Tool(_) => {}
    }
}

fn collect_idents_in_expr(e: &Expr, out: &mut HashSet<String>) {
    match e {
        Expr::Var(n, _) => {
            out.insert(n.clone());
        }
        Expr::Call { callee, args, .. } => {
            collect_idents_in_expr(callee, out);
            for a in args {
                collect_idents_in_expr(&a.value, out);
            }
        }
        Expr::Bin(_, l, r, _) => {
            collect_idents_in_expr(l, out);
            collect_idents_in_expr(r, out);
        }
        Expr::Un(_, x, _) => collect_idents_in_expr(x, out),
        Expr::If {
            cond,
            then_branch,
            else_branch,
            ..
        } => {
            collect_idents_in_expr(cond, out);
            collect_idents_in_expr(then_branch, out);
            collect_idents_in_expr(else_branch, out);
        }
        Expr::Let { value, body, .. } => {
            collect_idents_in_expr(value, out);
            collect_idents_in_expr(body, out);
        }
        Expr::Block { stmts, tail, .. } => {
            for s in stmts {
                match s {
                    Stmt::Let { value, .. } => collect_idents_in_expr(value, out),
                    Stmt::Expr(e) => collect_idents_in_expr(e, out),
                }
            }
            if let Some(t) = tail {
                collect_idents_in_expr(t, out);
            }
        }
        Expr::Tuple(xs, _) | Expr::List(xs, _) => {
            for x in xs {
                collect_idents_in_expr(x, out);
            }
        }
        Expr::Record(fs, _) => {
            for (_, e) in fs {
                collect_idents_in_expr(e, out);
            }
        }
        Expr::Field(e, _, _) => collect_idents_in_expr(e, out),
        Expr::Index(e, i, _) => {
            collect_idents_in_expr(e, out);
            collect_idents_in_expr(i, out);
        }
        Expr::Match {
            scrutinee, arms, ..
        } => {
            collect_idents_in_expr(scrutinee, out);
            for a in arms {
                if let Some(g) = &a.guard {
                    collect_idents_in_expr(g, out);
                }
                collect_idents_in_expr(&a.body, out);
            }
        }
        Expr::Lambda { body, .. } => collect_idents_in_expr(body, out),
        Expr::Annot { expr, .. } => collect_idents_in_expr(expr, out),
        Expr::Confident { value, p, .. } => {
            collect_idents_in_expr(value, out);
            collect_idents_in_expr(p, out);
        }
        Expr::Assume(p, _) => collect_idents_in_expr(p, out),
        Expr::StrInterp { parts, .. } => {
            for p in parts {
                if let aether_ast::StrPart::Expr(e) = p {
                    collect_idents_in_expr(e, out);
                }
            }
        }
        Expr::Lit(_, _) => {}
    }
}

fn collect_called_names(e: &Expr, out: &mut HashSet<String>) {
    match e {
        Expr::Call { callee, args, .. } => {
            if let Expr::Var(n, _) = &**callee {
                out.insert(n.clone());
            }
            for a in args {
                collect_called_names(&a.value, out);
            }
        }
        Expr::Bin(_, l, r, _) => {
            collect_called_names(l, out);
            collect_called_names(r, out);
        }
        Expr::Un(_, x, _) => collect_called_names(x, out),
        Expr::If {
            cond,
            then_branch,
            else_branch,
            ..
        } => {
            collect_called_names(cond, out);
            collect_called_names(then_branch, out);
            collect_called_names(else_branch, out);
        }
        Expr::Let { value, body, .. } => {
            collect_called_names(value, out);
            collect_called_names(body, out);
        }
        Expr::Block { stmts, tail, .. } => {
            for s in stmts {
                match s {
                    Stmt::Let { value, .. } => collect_called_names(value, out),
                    Stmt::Expr(e) => collect_called_names(e, out),
                }
            }
            if let Some(t) = tail {
                collect_called_names(t, out);
            }
        }
        Expr::Tuple(xs, _) | Expr::List(xs, _) => {
            for x in xs {
                collect_called_names(x, out);
            }
        }
        Expr::Record(fs, _) => {
            for (_, e) in fs {
                collect_called_names(e, out);
            }
        }
        Expr::Field(e, _, _) => collect_called_names(e, out),
        Expr::Index(e, i, _) => {
            collect_called_names(e, out);
            collect_called_names(i, out);
        }
        Expr::Match {
            scrutinee, arms, ..
        } => {
            collect_called_names(scrutinee, out);
            for a in arms {
                if let Some(g) = &a.guard {
                    collect_called_names(g, out);
                }
                collect_called_names(&a.body, out);
            }
        }
        Expr::Lambda { body, .. } => collect_called_names(body, out),
        Expr::Annot { expr, .. } => collect_called_names(expr, out),
        Expr::Confident { value, p, .. } => {
            collect_called_names(value, out);
            collect_called_names(p, out);
        }
        Expr::Assume(p, _) => collect_called_names(p, out),
        Expr::StrInterp { parts, .. } => {
            for p in parts {
                if let aether_ast::StrPart::Expr(e) = p {
                    collect_called_names(e, out);
                }
            }
        }
        Expr::Var(_, _) | Expr::Lit(_, _) => {}
    }
}

fn walk_for_unused_lets(e: &Expr, out: &mut Vec<Diagnostic>) {
    match e {
        Expr::Block { stmts, tail, .. } => {
            // Build the set of identifiers used by everything that follows each let.
            for (i, st) in stmts.iter().enumerate() {
                if let Stmt::Let {
                    pat: Pattern::Var(name, span),
                    ..
                } = st
                {
                    if name.starts_with('_') {
                        continue;
                    }
                    let mut used = HashSet::new();
                    for later in &stmts[i + 1..] {
                        match later {
                            Stmt::Let { value, .. } => collect_idents_in_expr(value, &mut used),
                            Stmt::Expr(e) => collect_idents_in_expr(e, &mut used),
                        }
                    }
                    if let Some(t) = tail {
                        collect_idents_in_expr(t, &mut used);
                    }
                    if !used.contains(name) {
                        out.push(warn(
                            *span,
                            format!("`let {name}` is unused (prefix with `_` to silence)"),
                        ));
                    }
                }
            }
            // Recurse into nested blocks.
            for s in stmts {
                match s {
                    Stmt::Expr(e) | Stmt::Let { value: e, .. } => walk_for_unused_lets(e, out),
                }
            }
            if let Some(t) = tail {
                walk_for_unused_lets(t, out);
            }
        }
        Expr::If {
            cond,
            then_branch,
            else_branch,
            ..
        } => {
            walk_for_unused_lets(cond, out);
            walk_for_unused_lets(then_branch, out);
            walk_for_unused_lets(else_branch, out);
        }
        Expr::Let { body, .. } => walk_for_unused_lets(body, out),
        Expr::Match { arms, .. } => {
            for a in arms {
                walk_for_unused_lets(&a.body, out);
            }
        }
        Expr::Lambda { body, .. } => walk_for_unused_lets(body, out),
        _ => {}
    }
}

/// Names of builtins that contribute the given effect, used by the
/// overbroad-effect lint.
fn builtins_with_effect(effect: &str) -> HashSet<String> {
    let names: &[&str] = match effect {
        "IO" => &[
            "print",
            "println",
            "print_module_surface",
            "print_prov",
            "sys_exit",
            "sys_stdin_line",
            "sys_now_unix",
            "sys_spawn",
            "sys_hostname",
            "time_now_ms",
            "time_monotonic_ms",
        ],
        "Net" => &["http_get", "llm_complete"],
        "FS" => &["path_exists", "mem_get", "mem_set", "snap_expect"],
        "Throw" => &[
            "int",
            "http_get",
            "llm_complete",
            "list_max",
            "result_unwrap",
            "assert",
            "assert_eq",
            "snap_expect",
            "regex_match",
            "regex_find",
            "regex_replace",
            "regex_split",
            "regex_captures",
            "sys_spawn",
        ],
        "Rand" => &[],
        "State" => &["mem_set", "env_set"],
        _ => &[],
    };
    names.iter().map(|s| (*s).to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_ast::FileId;
    use aether_parser::parse_module;

    fn lints(src: &str) -> Vec<Diagnostic> {
        let m = parse_module(FileId(0), src).unwrap();
        lint_module(&m)
    }

    #[test]
    fn flags_missing_docstring() {
        let diags = lints("fn add(x: Int, y: Int) -> Int effects {} { x + y }");
        assert!(diags.iter().any(|d| d.msg.contains("docstring")));
    }

    #[test]
    fn flags_unused_let() {
        let diags = lints(r#"fn main() -> Unit effects {} { let unused = 5; () }"#);
        assert!(diags
            .iter()
            .any(|d| d.msg.contains("`let unused` is unused")));
    }

    #[test]
    fn underscore_prefix_silences_unused_let() {
        let diags = lints(r#"fn main() -> Unit effects {} { let _ignore = 5; () }"#);
        assert!(diags.iter().all(|d| !d.msg.contains("_ignore")));
    }

    #[test]
    fn flags_overbroad_io() {
        let diags = lints("fn nothing() -> Int effects {IO} { 1 }");
        assert!(diags.iter().any(|d| d.msg.contains("declares effect `IO`")));
    }

    #[test]
    fn used_let_no_warning() {
        let diags = lints(r#"fn f() -> Int effects {} { let x = 5; x }"#);
        assert!(diags.iter().all(|d| !d.msg.contains("`let x` is unused")));
    }
}

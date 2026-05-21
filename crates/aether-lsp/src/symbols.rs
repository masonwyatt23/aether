//! Symbol resolution for hover, goto-definition, and completion.
//!
//! All functions operate on a parsed `Module` plus a `TypeCtx` that was produced
//! by `check_module`. They are pure functions that can be tested without any
//! LSP infrastructure.

use aether_ast::{Decl, Expr, FnDecl, Module, Span, Stmt, Type};
use aether_types::{FnSig, TypeCtx};

/// A resolved symbol — enough to produce a hover or goto response.
#[derive(Debug, Clone)]
pub struct SymbolInfo {
    pub name: String,
    pub kind: SymbolKind,
    /// The declaration span (for goto definition).
    pub def_span: Span,
}

#[derive(Debug, Clone)]
pub enum SymbolKind {
    Fn { sig: String },
    Let { ty: String },
    TypeAlias { expansion: String },
    Tool { sig: String },
}

impl SymbolInfo {
    pub fn hover_text(&self) -> String {
        match &self.kind {
            SymbolKind::Fn { sig } => format!("```aether\n{sig}\n```"),
            SymbolKind::Let { ty } => format!("```aether\nlet {}: {ty}\n```", self.name),
            SymbolKind::TypeAlias { expansion } => {
                format!("```aether\ntype {} = {expansion}\n```", self.name)
            }
            SymbolKind::Tool { sig } => format!("```aether\ntool {sig}\n```"),
        }
    }
}

// ---------------------------------------------------------------------------
// Type pretty-printing
// ---------------------------------------------------------------------------

/// Produce a human-readable representation of an Aether `Type`.
pub fn fmt_type(ty: &Type) -> String {
    match ty {
        Type::Con(c, _) => c.verbose_name().to_string(),
        Type::Var(v, _) => v.clone(),
        Type::Fun {
            params,
            ret,
            effects,
            ..
        } => {
            let ps: Vec<String> = params.iter().map(fmt_type).collect();
            let eff = if effects.is_pure() {
                String::new()
            } else {
                let effs: Vec<&str> = effects.effects.iter().map(|e| e.as_str()).collect();
                if let Some(tail) = &effects.tail {
                    format!(" !{{{}, {tail}}}", effs.join(", "))
                } else {
                    format!(" !{{{}}}", effs.join(", "))
                }
            };
            format!("({}) -> {}{eff}", ps.join(", "), fmt_type(ret))
        }
        Type::Refined {
            base, refinement, ..
        } => {
            format!("{}{{{}:...}}", fmt_type(base), refinement.binder)
        }
        Type::Tuple(ts, _) => {
            let parts: Vec<String> = ts.iter().map(fmt_type).collect();
            format!("({})", parts.join(", "))
        }
        Type::List(inner, _) => format!("[{}]", fmt_type(inner)),
        Type::Record(fields, _) => {
            let parts: Vec<String> = fields
                .iter()
                .map(|(k, v)| format!("{k}: {}", fmt_type(v)))
                .collect();
            format!("{{{}}}", parts.join(", "))
        }
        Type::Sum(ts, _) => {
            let parts: Vec<String> = ts.iter().map(fmt_type).collect();
            parts.join(" | ")
        }
        Type::Option(inner, _) => format!("{}?", fmt_type(inner)),
        Type::Confidence { base, .. } => format!("{}~confidence", fmt_type(base)),
        Type::Generic { name, args, .. } => {
            if args.is_empty() {
                name.clone()
            } else {
                let a: Vec<String> = args.iter().map(fmt_type).collect();
                format!("{name}<{}>", a.join(", "))
            }
        }
        Type::Adt { name, ctors, .. } => {
            let parts: Vec<String> = ctors
                .iter()
                .map(|(cn, fields)| {
                    if fields.is_empty() {
                        cn.clone()
                    } else {
                        let fs: Vec<String> = fields.iter().map(fmt_type).collect();
                        format!("{cn}({})", fs.join(", "))
                    }
                })
                .collect();
            format!("{name} = {}", parts.join(" | "))
        }
    }
}

/// Format a `FnSig` (from TypeCtx) as a readable Aether-like signature string.
pub fn fmt_fn_sig(name: &str, sig: &FnSig) -> String {
    let params: Vec<String> = sig
        .params
        .iter()
        .map(|(n, t)| format!("{n}: {}", fmt_type(t)))
        .collect();
    let eff = if sig.effects.is_pure() {
        " effects {}".to_string()
    } else {
        let effs: Vec<&str> = sig.effects.effects.iter().map(|e| e.as_str()).collect();
        format!(" effects {{{}}}", effs.join(", "))
    };
    format!(
        "fn {}({}) -> {}{}",
        name,
        params.join(", "),
        fmt_type(&sig.ret),
        eff
    )
}

/// Format a `FnDecl` (from the AST) as a signature string.
pub fn fmt_fn_decl(f: &FnDecl) -> String {
    let params: Vec<String> = f
        .params
        .iter()
        .map(|p| format!("{}: {}", p.name, fmt_type(&p.ty)))
        .collect();
    let eff = if f.effects.is_pure() {
        " effects {}".to_string()
    } else {
        let effs: Vec<&str> = f.effects.effects.iter().map(|e| e.as_str()).collect();
        format!(" effects {{{}}}", effs.join(", "))
    };
    format!(
        "fn {}({}) -> {}{}",
        f.name,
        params.join(", "),
        fmt_type(&f.ret),
        eff
    )
}

// ---------------------------------------------------------------------------
// Cursor hit-testing
// ---------------------------------------------------------------------------

/// Return true if `offset` (byte offset in file) falls inside `span`.
fn span_contains(span: Span, offset: u32) -> bool {
    !span.is_dummy() && offset >= span.start && offset < span.end
}

/// Walk an expression AST to find an identifier name under `offset`.
fn find_var_in_expr(expr: &Expr, offset: u32) -> Option<String> {
    match expr {
        Expr::Var(name, span) if span_contains(*span, offset) => Some(name.clone()),
        Expr::Var(_, _) | Expr::Lit(_, _) => None,
        Expr::Call { callee, args, .. } => find_var_in_expr(callee, offset)
            .or_else(|| args.iter().find_map(|a| find_var_in_expr(&a.value, offset))),
        Expr::Block { stmts, tail, .. } => stmts
            .iter()
            .find_map(|s| match s {
                Stmt::Let { value, .. } => find_var_in_expr(value, offset),
                Stmt::Expr(e) => find_var_in_expr(e, offset),
            })
            .or_else(|| tail.as_ref().and_then(|e| find_var_in_expr(e, offset))),
        Expr::If {
            cond,
            then_branch,
            else_branch,
            ..
        } => find_var_in_expr(cond, offset)
            .or_else(|| find_var_in_expr(then_branch, offset))
            .or_else(|| find_var_in_expr(else_branch, offset)),
        Expr::Bin(_, lhs, rhs, _) => {
            find_var_in_expr(lhs, offset).or_else(|| find_var_in_expr(rhs, offset))
        }
        Expr::Un(_, operand, _) => find_var_in_expr(operand, offset),
        Expr::Let { value, body, .. } => {
            find_var_in_expr(value, offset).or_else(|| find_var_in_expr(body, offset))
        }
        Expr::Match {
            scrutinee, arms, ..
        } => find_var_in_expr(scrutinee, offset).or_else(|| {
            arms.iter().find_map(|arm| {
                arm.guard
                    .as_ref()
                    .and_then(|g| find_var_in_expr(g, offset))
                    .or_else(|| find_var_in_expr(&arm.body, offset))
            })
        }),
        Expr::Lambda { body, .. } => find_var_in_expr(body, offset),
        Expr::Record(fields, _) => fields.iter().find_map(|(_, v)| find_var_in_expr(v, offset)),
        Expr::Field(base, _, _) => find_var_in_expr(base, offset),
        Expr::Index(base, index, _) => {
            find_var_in_expr(base, offset).or_else(|| find_var_in_expr(index, offset))
        }
        Expr::List(elems, _) => elems.iter().find_map(|e| find_var_in_expr(e, offset)),
        Expr::Tuple(elems, _) => elems.iter().find_map(|e| find_var_in_expr(e, offset)),
        Expr::Confident { value, p, .. } => {
            find_var_in_expr(value, offset).or_else(|| find_var_in_expr(p, offset))
        }
        Expr::Assume(e, _) => find_var_in_expr(e, offset),
        Expr::Annot { expr, .. } => find_var_in_expr(expr, offset),
        Expr::StrInterp { parts, .. } => parts
            .iter()
            .filter_map(|p| {
                if let aether_ast::StrPart::Expr(e) = p {
                    find_var_in_expr(e, offset)
                } else {
                    None
                }
            })
            .next(),
    }
}

/// Find the identifier name at `offset` across all declarations of a module.
pub fn find_name_at(module: &Module, offset: u32) -> Option<String> {
    for decl in &module.decls {
        let dspan = decl.span();
        if !span_contains(dspan, offset) {
            continue;
        }
        match decl {
            Decl::Fn(f) => {
                let name_end = f.span.start + f.name.len() as u32;
                if offset >= f.span.start && offset < name_end {
                    return Some(f.name.clone());
                }
                for p in &f.params {
                    if span_contains(p.span, offset) {
                        return Some(p.name.clone());
                    }
                }
                if let Some(name) = find_var_in_expr(&f.body, offset) {
                    return Some(name);
                }
            }
            Decl::Let(l) => {
                if let Some(name) = find_var_in_expr(&l.value, offset) {
                    return Some(name);
                }
                return Some(l.name.clone());
            }
            _ => {}
        }
    }
    None
}

/// Resolve a symbol name into a `SymbolInfo` using the module AST and type context.
pub fn resolve_symbol(module: &Module, ctx: &TypeCtx, name: &str) -> Option<SymbolInfo> {
    if let Some(sig) = ctx.lookup_fn(name) {
        let def_span = sig.span;
        return Some(SymbolInfo {
            name: name.to_string(),
            kind: SymbolKind::Fn {
                sig: fmt_fn_sig(name, sig),
            },
            def_span,
        });
    }
    if let Some(ty) = ctx.lookup_let(name) {
        let def_span = module
            .decls
            .iter()
            .find_map(|d| {
                if let Decl::Let(l) = d {
                    if l.name == name {
                        return Some(l.span);
                    }
                }
                None
            })
            .unwrap_or(Span::DUMMY);
        return Some(SymbolInfo {
            name: name.to_string(),
            kind: SymbolKind::Let { ty: fmt_type(ty) },
            def_span,
        });
    }
    for decl in &module.decls {
        match decl {
            Decl::TypeAlias(t) if t.name == name => {
                return Some(SymbolInfo {
                    name: name.to_string(),
                    kind: SymbolKind::TypeAlias {
                        expansion: fmt_type(&t.ty),
                    },
                    def_span: t.span,
                });
            }
            Decl::Tool(t) if t.name == name => {
                let params: Vec<String> = t
                    .params
                    .iter()
                    .map(|p| format!("{}: {}", p.name, fmt_type(&p.ty)))
                    .collect();
                let sig = format!("{}({}) -> {}", t.name, params.join(", "), fmt_type(&t.ret));
                return Some(SymbolInfo {
                    name: name.to_string(),
                    kind: SymbolKind::Tool { sig },
                    def_span: t.span,
                });
            }
            _ => {}
        }
    }
    None
}

/// Return all top-level symbol names for completion — (label, detail) pairs.
pub fn completion_names(module: &Module, ctx: &TypeCtx) -> Vec<(String, String)> {
    let mut items: Vec<(String, String)> = Vec::new();
    for (name, sig) in &ctx.funs {
        items.push((name.clone(), fmt_fn_sig(name, sig)));
    }
    for (name, ty) in &ctx.lets {
        items.push((name.clone(), fmt_type(ty)));
    }
    for decl in &module.decls {
        match decl {
            Decl::TypeAlias(t) => {
                items.push((
                    t.name.clone(),
                    format!("type {} = {}", t.name, fmt_type(&t.ty)),
                ));
            }
            Decl::Tool(t) => {
                let params: Vec<String> = t
                    .params
                    .iter()
                    .map(|p| format!("{}: {}", p.name, fmt_type(&p.ty)))
                    .collect();
                items.push((
                    t.name.clone(),
                    format!(
                        "tool {}({}) -> {}",
                        t.name,
                        params.join(", "),
                        fmt_type(&t.ret)
                    ),
                ));
            }
            _ => {}
        }
    }
    items.sort_by(|a, b| a.0.cmp(&b.0));
    items.dedup_by(|a, b| a.0 == b.0);
    items
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_ast::SourceMap;
    use aether_parser::parse_module;
    use aether_types::check_module;

    fn parse_and_check(src: &str) -> (Module, TypeCtx, SourceMap) {
        let mut map = SourceMap::new();
        let file = map.add("test.ae", src);
        let module = parse_module(file, src).expect("parse failed");
        let (ctx, _diags) = check_module(&module);
        (module, ctx, map)
    }

    #[test]
    fn resolve_fn_symbol() {
        let src = "fn add(x: Int, y: Int) -> Int effects {} { x + y }";
        let (module, ctx, _map) = parse_and_check(src);
        let info = resolve_symbol(&module, &ctx, "add").expect("should resolve 'add'");
        assert!(matches!(info.kind, SymbolKind::Fn { .. }));
        if let SymbolKind::Fn { sig } = &info.kind {
            assert!(sig.contains("add"), "sig should contain fn name: {sig}");
            assert!(sig.contains("Int"), "sig should mention Int: {sig}");
        }
    }

    #[test]
    fn completion_includes_fn() {
        let src = "fn greet(name: Str) -> Str effects {} { name }";
        let (module, ctx, _map) = parse_and_check(src);
        let items = completion_names(&module, &ctx);
        let names: Vec<&str> = items.iter().map(|(n, _)| n.as_str()).collect();
        assert!(
            names.contains(&"greet"),
            "expected 'greet' in completions: {names:?}"
        );
    }

    #[test]
    fn fmt_type_con_int() {
        let ty = Type::Con(aether_ast::TyCon::Int, aether_ast::Span::DUMMY);
        assert_eq!(fmt_type(&ty), "Int");
    }

    #[test]
    fn fmt_type_list() {
        let inner = Type::Con(aether_ast::TyCon::Str, aether_ast::Span::DUMMY);
        let ty = Type::List(Box::new(inner), aether_ast::Span::DUMMY);
        assert_eq!(fmt_type(&ty), "[Str]");
    }
}

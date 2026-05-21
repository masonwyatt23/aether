//! Bidirectional type/effect checker with refinement verification.

use crate::builtins::install_builtins;
use crate::ctx::{FnSig, TypeCtx};
use crate::refine::{prove, subst, Verdict};
use crate::suggest::closest_match;
use aether_ast::*;
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Note,
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub severity: Severity,
    pub span: Span,
    pub msg: String,
}

impl Diagnostic {
    fn err(span: Span, msg: impl Into<String>) -> Self {
        Diagnostic {
            severity: Severity::Error,
            span,
            msg: msg.into(),
        }
    }
    fn warn(span: Span, msg: impl Into<String>) -> Self {
        Diagnostic {
            severity: Severity::Warning,
            span,
            msg: msg.into(),
        }
    }
    pub fn note(span: Span, msg: impl Into<String>) -> Self {
        Diagnostic {
            severity: Severity::Note,
            span,
            msg: msg.into(),
        }
    }
}

/// Public entry point.
pub fn check_module(m: &Module) -> (TypeCtx, Vec<Diagnostic>) {
    let mut ctx = TypeCtx::new();
    install_builtins(&mut ctx);
    let mut diags = Vec::new();

    // Pass 1: collect signatures.
    for d in &m.decls {
        match d {
            Decl::Fn(f) => {
                let sig = FnSig {
                    params: f
                        .params
                        .iter()
                        .map(|p| (p.name.clone(), p.ty.clone()))
                        .collect(),
                    ret: f.ret.clone(),
                    effects: f.effects.clone(),
                    requires: f.spec.requires.clone(),
                    ensures: f.spec.ensures.clone(),
                    span: f.span,
                };
                ctx.insert_fn(f.name.clone(), sig);
            }
            Decl::Tool(t) => {
                let sig = FnSig {
                    params: t
                        .params
                        .iter()
                        .map(|p| (p.name.clone(), p.ty.clone()))
                        .collect(),
                    ret: t.ret.clone(),
                    effects: t.effects.clone(),
                    requires: vec![],
                    ensures: vec![],
                    span: t.span,
                };
                ctx.insert_fn(t.name.clone(), sig);
            }
            Decl::Let(l) => {
                if let Some(ty) = &l.ty {
                    ctx.insert_let(l.name.clone(), ty.clone());
                }
            }
            Decl::TypeAlias(ta) => {
                // Register ADT constructors so they can be called as functions.
                if let Type::Adt {
                    name: adt_name,
                    ctors,
                    ..
                } = &ta.ty
                {
                    for (ctor_name, fields) in ctors {
                        ctx.insert_ctor(ctor_name.clone(), adt_name.clone(), fields.clone());
                        // Also expose each constructor as a function signature so
                        // check_call can validate arity and argument types.
                        let adt_span = ta.span;
                        let ret_ty = Type::Generic {
                            name: adt_name.clone(),
                            args: vec![],
                            span: adt_span,
                        };
                        let sig = FnSig {
                            params: fields
                                .iter()
                                .enumerate()
                                .map(|(i, t)| (format!("arg{i}"), t.clone()))
                                .collect(),
                            ret: ret_ty,
                            effects: EffectRow::pure_(),
                            requires: vec![],
                            ensures: vec![],
                            span: adt_span,
                        };
                        ctx.insert_fn(ctor_name.clone(), sig);
                    }
                }
            }
            _ => {}
        }
    }

    // Pass 2: type-check each fn body.
    for d in &m.decls {
        if let Decl::Fn(f) = d {
            check_fn(&ctx, f, &mut diags);
        }
    }
    (ctx, diags)
}

#[derive(Debug, Default, Clone)]
struct Scope {
    vars: Vec<(String, Type)>,
    /// Path conditions accumulated via if/else branching. Each entry is a
    /// boolean expression assumed to hold at this point.
    path: Vec<Expr>,
}

impl Scope {
    fn bind(&mut self, name: String, ty: Type) {
        self.vars.push((name, ty));
    }
    fn lookup(&self, name: &str) -> Option<&Type> {
        self.vars
            .iter()
            .rev()
            .find(|(n, _)| n == name)
            .map(|(_, t)| t)
    }
    fn push_assume(&mut self, e: Expr) {
        self.path.push(e);
    }
}

fn check_fn(ctx: &TypeCtx, f: &FnDecl, diags: &mut Vec<Diagnostic>) {
    let mut scope = Scope::default();
    for p in &f.params {
        scope.bind(p.name.clone(), p.ty.clone());
        // If the parameter has a refinement, push its predicate as a hypothesis.
        if let Type::Refined { refinement, .. } = &p.ty {
            // The refinement's binder is local; rename to the param name.
            let pred = subst(
                &refinement.pred,
                &refinement.binder,
                &Expr::Var(p.name.clone(), p.span),
            );
            scope.push_assume(pred);
        }
    }
    // Also any `requires` clauses become assumed.
    for r in &f.spec.requires {
        scope.push_assume(r.clone());
    }
    let mut observed = HashSet::<Effect>::new();
    let body_ty = check_expr(ctx, &mut scope, &f.body, &mut observed, diags);
    // Type check return.
    if !type_compatible(&body_ty, &f.ret) {
        diags.push(Diagnostic::err(
            f.body.span(),
            format!(
                "function `{}` returns {} but body has type {}",
                f.name,
                show_type(&f.ret),
                show_type(&body_ty),
            ),
        ));
    }
    // Effect check.
    for eff in &observed {
        if !effect_allowed(eff, &f.effects) {
            diags.push(Diagnostic::err(
                f.body.span(),
                format!(
                    "function `{}` uses effect `{}` but declares effects {}",
                    f.name,
                    eff.as_str(),
                    show_effects(&f.effects),
                ),
            ));
        }
    }
    // Refinement: for each ensures clause, try to prove it under path conditions
    // and the substitution result := body_value. We can only do this if the body
    // is itself convertible to a linear expression (literal or simple branching).
    for ens in &f.spec.ensures {
        // Substitute `result` with the body expression (which the solver may
        // simplify if it's an if/then/else of arithmetic).
        prove_ensures(&scope, ens, &f.body, diags);
    }
}

fn prove_ensures(scope: &Scope, ensures: &Expr, body: &Expr, diags: &mut Vec<Diagnostic>) {
    // Walk the body symbolically. For each path, substitute `result` with the
    // leaf expression and call the solver under the accumulated path conditions.
    fn walk(scope: &Scope, ens: &Expr, body: &Expr, diags: &mut Vec<Diagnostic>) {
        match body {
            Expr::If {
                cond,
                then_branch,
                else_branch,
                ..
            } => {
                let mut s_then = scope.clone();
                s_then.push_assume((**cond).clone());
                walk(&s_then, ens, then_branch, diags);
                let mut s_else = scope.clone();
                s_else.push_assume(Expr::Un(UnOp::Not, cond.clone(), cond.span()));
                walk(&s_else, ens, else_branch, diags);
            }
            Expr::Block { stmts, tail, .. } => {
                let mut s2 = scope.clone();
                for st in stmts {
                    if let Stmt::Let {
                        pat: Pattern::Var(name, _),
                        value,
                        ..
                    } = st
                    {
                        // Add `name = value` as an equality assumption.
                        let eq = Expr::Bin(
                            BinOp::Eq,
                            Box::new(Expr::Var(name.clone(), st.span())),
                            Box::new(value.clone()),
                            st.span(),
                        );
                        s2.push_assume(eq);
                    }
                }
                if let Some(t) = tail {
                    walk(&s2, ens, t, diags);
                } else {
                    let unit = Expr::Lit(Lit::Unit, body.span());
                    let g = subst(ens, "result", &unit);
                    judge(&s2, &g, body.span(), diags);
                }
            }
            other => {
                let g = subst(ens, "result", other);
                judge(scope, &g, body.span(), diags);
            }
        }
    }
    walk(scope, ensures, body, diags);
}

fn judge(scope: &Scope, goal: &Expr, span: Span, diags: &mut Vec<Diagnostic>) {
    match prove(&scope.path, goal) {
        Verdict::Proved => { /* good */ }
        Verdict::RefutedWith { values } => {
            let msg = if values.is_empty() {
                format!(
                    "postcondition refuted: a counterexample exists for `{}`",
                    goal_display(goal),
                )
            } else {
                let pairs: Vec<String> =
                    values.iter().map(|(k, v)| format!("{}={}", k, v)).collect();
                format!(
                    "postcondition refuted: counterexample {{{}}} satisfies the hypothesis but not the goal `{}`",
                    pairs.join(", "),
                    goal_display(goal),
                )
            };
            diags.push(Diagnostic::err(span, msg));
        }
        Verdict::Unknown => {
            diags.push(Diagnostic::warn(
                span,
                format!(
                    "postcondition `{}` could not be verified (outside linear-arithmetic fragment or beyond solver budget)",
                    goal_display(goal),
                ),
            ));
        }
    }
}

fn goal_display(e: &Expr) -> String {
    use aether_parser::pretty::{expr, Form};
    expr(e, Form::Verbose)
}

fn check_expr(
    ctx: &TypeCtx,
    scope: &mut Scope,
    e: &Expr,
    observed: &mut HashSet<Effect>,
    diags: &mut Vec<Diagnostic>,
) -> Type {
    match e {
        Expr::Lit(l, s) => lit_type(l, *s),
        Expr::Var(name, s) => {
            if let Some(t) = scope.lookup(name) {
                t.clone()
            } else if let Some(t) = ctx.lookup_let(name) {
                t.clone()
            } else if ctx.lookup_fn(name).is_some() {
                // Reference to a function as a value — we don't fully support
                // first-class functions, but allow the lookup to type-check.
                Type::Var(name.clone(), *s)
            } else {
                {
                    let mut candidates: Vec<String> =
                        scope.vars.iter().map(|(n, _)| n.clone()).collect();
                    candidates.extend(ctx.lets.keys().cloned());
                    candidates.extend(ctx.funs.keys().cloned());
                    let base = format!("unbound identifier `{name}`");
                    let msg = match closest_match(name, &candidates, 2) {
                        Some(suggestion) => format!("{base}. did you mean `{suggestion}`?"),
                        None => base,
                    };
                    diags.push(Diagnostic::err(*s, msg));
                }
                Type::Var("?".into(), *s)
            }
        }
        Expr::Bin(op, l, r, s) => {
            let lt = check_expr(ctx, scope, l, observed, diags);
            let rt = check_expr(ctx, scope, r, observed, diags);
            check_bin(*op, &lt, &rt, *s, diags)
        }
        Expr::Un(UnOp::Neg, x, s) => {
            let t = check_expr(ctx, scope, x, observed, diags);
            if matches!(
                t.unrefined(),
                Type::Con(TyCon::Int, _) | Type::Con(TyCon::Float, _)
            ) {
                t
            } else {
                diags.push(Diagnostic::err(*s, "negation requires Int or Float"));
                Type::Con(TyCon::Int, *s)
            }
        }
        Expr::Un(UnOp::Not, x, s) => {
            let t = check_expr(ctx, scope, x, observed, diags);
            if !matches!(t.unrefined(), Type::Con(TyCon::Bool, _)) {
                diags.push(Diagnostic::err(*s, "`!` requires a Bool"));
            }
            Type::Con(TyCon::Bool, *s)
        }
        Expr::Call { callee, args, span } => {
            check_call(ctx, scope, callee, args, *span, observed, diags)
        }
        Expr::If {
            cond,
            then_branch,
            else_branch,
            span,
        } => {
            let ct = check_expr(ctx, scope, cond, observed, diags);
            if !matches!(ct.unrefined(), Type::Con(TyCon::Bool, _)) {
                diags.push(Diagnostic::err(cond.span(), "if-condition must be Bool"));
            }
            // We do not narrow types within branches in the MVP type checker.
            let tt = check_expr(ctx, scope, then_branch, observed, diags);
            let et = check_expr(ctx, scope, else_branch, observed, diags);
            if !type_compatible(&tt, &et) {
                diags.push(Diagnostic::err(
                    *span,
                    format!(
                        "branches of `if` have incompatible types: {} vs {}",
                        show_type(&tt),
                        show_type(&et)
                    ),
                ));
            }
            tt
        }
        Expr::Block { stmts, tail, span } => {
            for st in stmts {
                match st {
                    Stmt::Let {
                        pat: Pattern::Var(name, _),
                        ty,
                        value,
                        ..
                    } => {
                        let vt = check_expr(ctx, scope, value, observed, diags);
                        let bound_ty = ty.clone().unwrap_or(vt);
                        scope.bind(name.clone(), bound_ty);
                    }
                    Stmt::Let { value, .. } => {
                        check_expr(ctx, scope, value, observed, diags);
                    }
                    Stmt::Expr(e) => {
                        check_expr(ctx, scope, e, observed, diags);
                    }
                }
            }
            if let Some(t) = tail {
                check_expr(ctx, scope, t, observed, diags)
            } else {
                Type::Con(TyCon::Unit, *span)
            }
        }
        Expr::Let {
            pat,
            ty,
            value,
            body,
            ..
        } => {
            let vt = check_expr(ctx, scope, value, observed, diags);
            let bound_ty = ty.clone().unwrap_or(vt);
            if let Pattern::Var(name, _) = pat {
                scope.bind(name.clone(), bound_ty);
            }
            check_expr(ctx, scope, body, observed, diags)
        }
        Expr::Tuple(elts, s) => {
            let types = elts
                .iter()
                .map(|e| check_expr(ctx, scope, e, observed, diags))
                .collect();
            Type::Tuple(types, *s)
        }
        Expr::List(elts, s) => {
            if elts.is_empty() {
                Type::List(Box::new(Type::Var("a".into(), *s)), *s)
            } else {
                let head_ty = check_expr(ctx, scope, &elts[0], observed, diags);
                for e in &elts[1..] {
                    let t = check_expr(ctx, scope, e, observed, diags);
                    if !type_compatible(&t, &head_ty) {
                        diags.push(Diagnostic::err(
                            e.span(),
                            format!(
                                "list element type {} doesn't match {}",
                                show_type(&t),
                                show_type(&head_ty)
                            ),
                        ));
                    }
                }
                Type::List(Box::new(head_ty), *s)
            }
        }
        Expr::Record(fields, s) => {
            let typed = fields
                .iter()
                .map(|(n, e)| (n.clone(), check_expr(ctx, scope, e, observed, diags)))
                .collect();
            Type::Record(typed, *s)
        }
        Expr::Field(e, name, s) => {
            let t = check_expr(ctx, scope, e, observed, diags);
            match t.unrefined() {
                Type::Record(fields, _) => fields
                    .iter()
                    .find(|(n, _)| n == name)
                    .map(|(_, t)| t.clone())
                    .unwrap_or_else(|| {
                        {
                            let field_names: Vec<String> =
                                fields.iter().map(|(n, _)| n.clone()).collect();
                            let base = format!("record has no field `{name}`");
                            let msg = match closest_match(name, &field_names, 2) {
                                Some(suggestion) => format!("{base}. did you mean `{suggestion}`?"),
                                None if !field_names.is_empty() => {
                                    format!(
                                        "{base}. the record has fields: {}",
                                        field_names.join(", ")
                                    )
                                }
                                None => base,
                            };
                            diags.push(Diagnostic::err(*s, msg));
                        }
                        Type::Var("?".into(), *s)
                    }),
                _ => {
                    diags.push(Diagnostic::err(*s, "field access on non-record"));
                    Type::Var("?".into(), *s)
                }
            }
        }
        Expr::Index(e, idx, s) => {
            let t = check_expr(ctx, scope, e, observed, diags);
            let it = check_expr(ctx, scope, idx, observed, diags);
            if !matches!(it.unrefined(), Type::Con(TyCon::Int, _)) {
                diags.push(Diagnostic::err(idx.span(), "index must be Int"));
            }
            match t.unrefined() {
                Type::List(inner, _) => (**inner).clone(),
                _ => {
                    diags.push(Diagnostic::err(*s, "indexing a non-list"));
                    Type::Var("?".into(), *s)
                }
            }
        }
        Expr::Confident { value, p, span } => {
            let vt = check_expr(ctx, scope, value, observed, diags);
            let pt = check_expr(ctx, scope, p, observed, diags);
            if !matches!(
                pt.unrefined(),
                Type::Con(TyCon::Float, _) | Type::Con(TyCon::Int, _)
            ) {
                diags.push(Diagnostic::err(p.span(), "confidence must be a number"));
            }
            Type::Confidence {
                base: Box::new(vt),
                p: p.clone(),
                span: *span,
            }
        }
        Expr::Assume(p, s) => {
            let pt = check_expr(ctx, scope, p, observed, diags);
            if !matches!(pt.unrefined(), Type::Con(TyCon::Bool, _)) {
                diags.push(Diagnostic::err(
                    p.span(),
                    "`assume` requires a Bool predicate",
                ));
            }
            scope.push_assume((**p).clone());
            Type::Con(TyCon::Unit, *s)
        }
        Expr::StrInterp { parts, span } => {
            // Evaluate each embedded expression for side effects + effects accounting;
            // the result is always a Str.
            for p in parts {
                if let aether_ast::StrPart::Expr(e) = p {
                    check_expr(ctx, scope, e, observed, diags);
                }
            }
            Type::Con(TyCon::Str, *span)
        }
        Expr::Annot { expr, ty, .. } => {
            let t = check_expr(ctx, scope, expr, observed, diags);
            if !type_compatible(&t, ty) {
                diags.push(Diagnostic::err(
                    expr.span(),
                    format!(
                        "annotation expects {}, got {}",
                        show_type(ty),
                        show_type(&t)
                    ),
                ));
            }
            ty.clone()
        }
        Expr::Lambda {
            params,
            ret,
            body,
            span,
        } => {
            let mut inner = scope.clone();
            for p in params {
                inner.bind(p.name.clone(), p.ty.clone());
            }
            let bt = check_expr(ctx, &mut inner, body, observed, diags);
            let ret_ty = ret.clone().unwrap_or(bt.clone());
            Type::Fun {
                params: params.iter().map(|p| p.ty.clone()).collect(),
                ret: Box::new(ret_ty),
                effects: EffectRow::pure_(),
                span: *span,
            }
        }
        Expr::Match {
            scrutinee,
            arms,
            span,
        } => {
            let scrut_ty = check_expr(ctx, scope, scrutinee, observed, diags);
            if arms.is_empty() {
                return Type::Con(TyCon::Unit, *span);
            }
            let mut arm_types = Vec::with_capacity(arms.len());
            for arm in arms {
                // Pattern variables come into scope inside the arm body.
                let mut inner = scope.clone();
                bind_pattern_typed(&arm.pat, &mut inner, ctx);
                if let Some(g) = &arm.guard {
                    let gt = check_expr(ctx, &mut inner, g, observed, diags);
                    if !matches!(gt.unrefined(), Type::Con(TyCon::Bool, _)) {
                        diags.push(Diagnostic::err(g.span(), "match guard must be Bool"));
                    }
                }
                let t = check_expr(ctx, &mut inner, &arm.body, observed, diags);
                arm_types.push(t);
            }
            let head = arm_types[0].clone();
            for t in &arm_types[1..] {
                if !type_compatible(t, &head) {
                    diags.push(Diagnostic::err(
                        *span,
                        format!(
                            "match arms have incompatible types: {} vs {}",
                            show_type(&head),
                            show_type(t)
                        ),
                    ));
                }
            }
            // Exhaustiveness check: only for ADT scrutinees.
            exhaustiveness_check(ctx, &scrut_ty, arms, *span, diags);
            head
        }
    }
}

/// Binding of pattern variables into scope with ADT-aware field types.
fn bind_pattern_typed(pat: &Pattern, scope: &mut Scope, ctx: &TypeCtx) {
    match pat {
        Pattern::Var(name, span) => scope.bind(name.clone(), Type::Var("a".into(), *span)),
        Pattern::Ctor {
            name: ctor_name,
            args: ps,
            span,
        } => {
            // Look up field types from the constructor table.
            let field_types: Vec<Type> = if let Some((_, fields)) = ctx.lookup_ctor(ctor_name) {
                fields.clone()
            } else {
                vec![Type::Var("a".into(), *span); ps.len()]
            };
            for (i, p) in ps.iter().enumerate() {
                let ft = field_types
                    .get(i)
                    .cloned()
                    .unwrap_or_else(|| Type::Var("a".into(), p.span()));
                bind_pattern_inner(p, scope, ctx, &ft);
            }
        }
        Pattern::Tuple(ps, _) => {
            for p in ps {
                bind_pattern_typed(p, scope, ctx);
            }
        }
        Pattern::Record(ps, _) => {
            for (_, p) in ps {
                bind_pattern_typed(p, scope, ctx);
            }
        }
        _ => {}
    }
}

/// Helper: bind a sub-pattern with a known expected type.
fn bind_pattern_inner(pat: &Pattern, scope: &mut Scope, ctx: &TypeCtx, ty: &Type) {
    match pat {
        Pattern::Var(name, _) => scope.bind(name.clone(), ty.clone()),
        Pattern::Wild(_) => {}
        _ => bind_pattern_typed(pat, scope, ctx),
    }
}

/// Exhaustiveness check for match against a known ADT type.
/// Emits at most one warning per match site.
fn exhaustiveness_check(
    ctx: &TypeCtx,
    scrut_ty: &Type,
    arms: &[MatchArm],
    span: Span,
    diags: &mut Vec<Diagnostic>,
) {
    // Resolve the scrutinee type name: accept Generic { name, args: [] } or Var.
    let adt_name = match scrut_ty.unrefined() {
        Type::Generic { name, args, .. } if args.is_empty() => name.clone(),
        Type::Var(name, _) => name.clone(),
        _ => return, // not a named type — skip
    };
    // Gather all constructor names for this ADT from the ctor table.
    let all_ctors: Vec<&str> = ctx
        .ctors
        .iter()
        .filter(|(_, (aname, _))| aname == &adt_name)
        .map(|(cname, _)| cname.as_str())
        .collect();
    if all_ctors.is_empty() {
        return; // not a known ADT
    }
    // Check if any arm is a wildcard/var (catches all) — if so, exhaustive.
    let has_wildcard = arms
        .iter()
        .any(|a| matches!(a.pat, Pattern::Wild(_) | Pattern::Var(_, _)));
    if has_wildcard {
        return;
    }
    // Collect covered constructor names.
    let covered: HashSet<&str> = arms
        .iter()
        .filter_map(|a| {
            if let Pattern::Ctor { name, .. } = &a.pat {
                Some(name.as_str())
            } else {
                None
            }
        })
        .collect();
    let missing: Vec<&str> = all_ctors
        .into_iter()
        .filter(|c| !covered.contains(c))
        .collect();
    if !missing.is_empty() {
        diags.push(Diagnostic::warn(
            span,
            format!(
                "non-exhaustive match on `{}`: missing constructor(s) {}",
                adt_name,
                missing.join(", ")
            ),
        ));
    }
}

fn check_call(
    ctx: &TypeCtx,
    scope: &mut Scope,
    callee: &Expr,
    args: &[Arg],
    span: Span,
    observed: &mut HashSet<Effect>,
    diags: &mut Vec<Diagnostic>,
) -> Type {
    let name = match callee {
        Expr::Var(n, _) => n.clone(),
        _ => {
            diags.push(Diagnostic::err(
                callee.span(),
                "callee must be an identifier in MVP",
            ));
            return Type::Var("?".into(), span);
        }
    };
    let sig = if let Some(s) = ctx.lookup_fn(&name) {
        s.clone()
    } else if let Some(local_ty) = scope.lookup(&name).cloned() {
        // Local binding shadows / replaces — check it's a function type.
        match local_ty.unrefined() {
            Type::Fun {
                params,
                ret,
                effects,
                ..
            } => FnSig {
                params: params
                    .iter()
                    .enumerate()
                    .map(|(i, t)| (format!("arg{i}"), t.clone()))
                    .collect(),
                ret: (**ret).clone(),
                effects: effects.clone(),
                requires: vec![],
                ensures: vec![],
                span,
            },
            Type::Var(_, _) => {
                // Unknown / inferred function — accept positional args without checking.
                FnSig {
                    params: args
                        .iter()
                        .enumerate()
                        .map(|(i, _)| (format!("arg{i}"), Type::Var("a".into(), span)))
                        .collect(),
                    ret: Type::Var("a".into(), span),
                    effects: EffectRow::pure_(),
                    requires: vec![],
                    ensures: vec![],
                    span,
                }
            }
            _ => {
                diags.push(Diagnostic::err(
                    callee.span(),
                    format!("`{name}` is not callable"),
                ));
                return Type::Var("?".into(), span);
            }
        }
    } else {
        {
            let candidates: Vec<String> = ctx.funs.keys().cloned().collect();
            let base = format!("no function named `{name}`");
            let msg = match closest_match(&name, &candidates, 2) {
                Some(suggestion) => format!("{base}. did you mean `{suggestion}`?"),
                None => base,
            };
            diags.push(Diagnostic::err(callee.span(), msg));
        }
        return Type::Var("?".into(), span);
    };
    if args.len() > sig.params.len() {
        diags.push(Diagnostic::err(
            span,
            format!(
                "too many arguments to `{name}`: expected {} got {}",
                sig.params.len(),
                args.len()
            ),
        ));
    }
    for (i, arg) in args.iter().enumerate() {
        if i >= sig.params.len() {
            break;
        }
        let at = check_expr(ctx, scope, &arg.value, observed, diags);
        let (_pname, pty) = &sig.params[i];
        if !type_compatible(&at, pty) {
            diags.push(Diagnostic::err(
                arg.value.span(),
                format!(
                    "argument {}: expected {}, got {}",
                    i + 1,
                    show_type(pty),
                    show_type(&at)
                ),
            ));
        }
    }
    // Effect propagation.
    for eff in &sig.effects.effects {
        observed.insert(eff.clone());
    }
    sig.ret.clone()
}

fn check_bin(op: BinOp, l: &Type, r: &Type, span: Span, diags: &mut Vec<Diagnostic>) -> Type {
    let lu = l.unrefined();
    let ru = r.unrefined();
    match op {
        BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => {
            if matches!(lu, Type::Con(TyCon::Int, _)) && matches!(ru, Type::Con(TyCon::Int, _)) {
                Type::Con(TyCon::Int, span)
            } else if numeric(lu) && numeric(ru) {
                Type::Con(TyCon::Float, span)
            } else {
                diags.push(Diagnostic::err(span, "arithmetic requires numbers"));
                Type::Con(TyCon::Int, span)
            }
        }
        BinOp::Eq | BinOp::Neq => Type::Con(TyCon::Bool, span),
        BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
            if numeric(lu) && numeric(ru) {
                Type::Con(TyCon::Bool, span)
            } else {
                diags.push(Diagnostic::err(span, "comparison requires numbers"));
                Type::Con(TyCon::Bool, span)
            }
        }
        BinOp::And | BinOp::Or => {
            if matches!(lu, Type::Con(TyCon::Bool, _)) && matches!(ru, Type::Con(TyCon::Bool, _)) {
                Type::Con(TyCon::Bool, span)
            } else {
                diags.push(Diagnostic::err(span, "&&/|| require Bool operands"));
                Type::Con(TyCon::Bool, span)
            }
        }
        BinOp::Concat => {
            if matches!(lu, Type::Con(TyCon::Str, _)) && matches!(ru, Type::Con(TyCon::Str, _)) {
                Type::Con(TyCon::Str, span)
            } else if matches!(lu, Type::List(..)) {
                lu.clone()
            } else {
                diags.push(Diagnostic::err(span, "++ requires Str or List operands"));
                Type::Con(TyCon::Str, span)
            }
        }
        BinOp::Implies => {
            if matches!(lu, Type::Con(TyCon::Bool, _)) && matches!(ru, Type::Con(TyCon::Bool, _)) {
                Type::Con(TyCon::Bool, span)
            } else {
                diags.push(Diagnostic::err(span, "=> requires Bool operands"));
                Type::Con(TyCon::Bool, span)
            }
        }
    }
}

fn numeric(t: &Type) -> bool {
    matches!(
        t,
        Type::Con(TyCon::Int, _) | Type::Con(TyCon::Float, _) | Type::Var(_, _)
    )
}

fn lit_type(l: &Lit, s: Span) -> Type {
    match l {
        Lit::Int(_) => Type::Con(TyCon::Int, s),
        Lit::Float(_) => Type::Con(TyCon::Float, s),
        Lit::Bool(_) => Type::Con(TyCon::Bool, s),
        Lit::Str(_) => Type::Con(TyCon::Str, s),
        Lit::Unit => Type::Con(TyCon::Unit, s),
    }
}

/// Structural compatibility, ignoring refinements and confidence layers.
fn type_compatible(actual: &Type, expected: &Type) -> bool {
    use Type::*;
    let a = actual.unrefined();
    let e = expected.unrefined();
    match (a, e) {
        (Var(_, _), _) | (_, Var(_, _)) => true,
        (Con(c1, _), Con(c2, _)) => c1 == c2,
        (Tuple(a1, _), Tuple(a2, _)) => {
            a1.len() == a2.len() && a1.iter().zip(a2).all(|(x, y)| type_compatible(x, y))
        }
        (List(x, _), List(y, _)) => type_compatible(x, y),
        (Record(a1, _), Record(a2, _)) => {
            a1.len() == a2.len() && {
                let mut a1 = a1.clone();
                let mut a2 = a2.clone();
                a1.sort_by(|x, y| x.0.cmp(&y.0));
                a2.sort_by(|x, y| x.0.cmp(&y.0));
                a1.iter()
                    .zip(a2.iter())
                    .all(|((n1, t1), (n2, t2))| n1 == n2 && type_compatible(t1, t2))
            }
        }
        (Option(x, _), Option(y, _)) => type_compatible(x, y),
        (
            Fun {
                params: p1,
                ret: r1,
                ..
            },
            Fun {
                params: p2,
                ret: r2,
                ..
            },
        ) => {
            p1.len() == p2.len()
                && p1.iter().zip(p2).all(|(x, y)| type_compatible(x, y))
                && type_compatible(r1, r2)
        }
        (
            Generic {
                name: n1, args: a1, ..
            },
            Generic {
                name: n2, args: a2, ..
            },
        ) => {
            n1 == n2
                && a1.len() == a2.len()
                && a1.iter().zip(a2).all(|(x, y)| type_compatible(x, y))
        }
        _ => false,
    }
}

fn effect_allowed(eff: &Effect, row: &EffectRow) -> bool {
    row.effects.contains(eff) || row.tail.is_some()
}

fn show_type(t: &Type) -> String {
    use aether_parser::pretty::{type_, Form};
    type_(t, Form::Verbose)
}

fn show_effects(e: &EffectRow) -> String {
    let mut s = String::from("{");
    for (i, eff) in e.effects.iter().enumerate() {
        if i > 0 {
            s.push_str(", ");
        }
        s.push_str(eff.as_str());
    }
    s.push('}');
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_ast::FileId;
    use aether_parser::parse_module;

    fn check(src: &str) -> Vec<Diagnostic> {
        let m = parse_module(FileId(0), src).unwrap();
        check_module(&m).1
    }

    #[test]
    fn ok_simple_fn() {
        let diags = check("fn add(x: Int, y: Int) -> Int effects {} { x + y }");
        assert!(
            diags.iter().all(|d| d.severity != Severity::Error),
            "got {:?}",
            diags
        );
    }

    #[test]
    fn detects_missing_effect() {
        // fetch uses http_get which has !{Net,Throw}; declares only {Net} -> error
        let diags = check("fn fetch(u: Str) -> Str effects {Net} { http_get(u) }");
        assert!(
            diags.iter().any(|d| d.severity == Severity::Error),
            "expected error, got {:?}",
            diags
        );
    }

    #[test]
    fn proves_trivial_postcondition() {
        let diags = check("fn pos(n: Int) -> Int where result >= 0 effects {} { 1 }");
        assert!(
            !diags.iter().any(|d| d.severity == Severity::Error),
            "got {:?}",
            diags
        );
    }

    #[test]
    fn refutes_false_postcondition() {
        let diags = check("fn neg(n: Int) -> Int where result > 0 effects {} { -1 }");
        assert!(
            diags
                .iter()
                .any(|d| matches!(d.severity, Severity::Error | Severity::Warning)),
            "got {:?}",
            diags
        );
    }

    // ── ADT type-checker tests ─────────────────────────────────────────────────

    #[test]
    fn adt_ctor_wrong_arity_error() {
        // Circle takes 1 Float; passing 2 args should produce an error.
        let diags = check(
            "type Shape = Circle(Float) | Square(Float)\n\
             fn bad() -> Shape effects {} { Circle(1.0, 2.0) }",
        );
        assert!(
            diags.iter().any(|d| d.severity == Severity::Error),
            "expected arity error, got {:?}",
            diags
        );
    }

    #[test]
    fn adt_ctor_correct_call_no_error() {
        let diags = check(
            "type Shape = Circle(Float) | Square(Float)\n\
             fn ok() -> Shape effects {} { Circle(5.0) }",
        );
        assert!(
            diags.iter().all(|d| d.severity != Severity::Error),
            "unexpected errors: {:?}",
            diags
        );
    }

    #[test]
    fn adt_exhaustiveness_warning_partial_coverage() {
        // Match covers Circle and Square but not Triangle — should warn.
        let diags = check(
            "type Shape = Circle(Float) | Square(Float) | Triangle(Float, Float, Float)\n\
             fn area(s: Shape) -> Float effects {} {\n\
               match s with {\n\
                 Circle(r)    => r * r * 3.14,\n\
                 Square(side) => side * side\n\
               }\n\
             }",
        );
        assert!(
            diags.iter().any(|d| d.severity == Severity::Warning),
            "expected exhaustiveness warning, got {:?}",
            diags
        );
    }

    #[test]
    fn adt_exhaustiveness_wildcard_no_warning() {
        // `_` wildcard makes the match exhaustive — no warning.
        let diags = check(
            "type Shape = Circle(Float) | Square(Float)\n\
             fn area(s: Shape) -> Float effects {} {\n\
               match s with {\n\
                 Circle(r) => r * r * 3.14,\n\
                 _         => 0.0\n\
               }\n\
             }",
        );
        assert!(
            diags.iter().all(|d| d.severity != Severity::Warning),
            "unexpected warning with wildcard arm: {:?}",
            diags
        );
    }
    // ── Suggestion-engine diagnostic tests ────────────────────────────────────

    #[test]
    fn suggests_close_var_name() {
        // "prnt" is 1 edit from "print" (a builtin fn).
        let diags = check("fn f() -> Unit effects {IO} { prnt(\"hi\") }");
        let errs: Vec<&Diagnostic> = diags
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        assert!(!errs.is_empty(), "expected an error for unknown identifier");
        let found = errs.iter().any(|d| d.msg.contains("did you mean"));
        assert!(found, "expected 'did you mean' suggestion, got: {:?}", errs);
    }

    #[test]
    fn suggests_close_fn_name() {
        // "lenght" is 1-2 edits from "length" — but no "length" builtin exists.
        // Use "prnt" → "print" which is a well-known builtin registered by install_builtins.
        let diags = check("fn f() -> Unit effects {IO} { prnt(\"hello\") }");
        let errs: Vec<&Diagnostic> = diags
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        assert!(!errs.is_empty(), "expected an error for unknown function");
        let found = errs
            .iter()
            .any(|d| d.msg.contains("did you mean") || d.msg.contains("print"));
        assert!(
            found,
            "expected a suggestion toward 'print', got: {:?}",
            errs
        );
    }

    #[test]
    fn no_suggestion_for_random_text() {
        let diags = check("fn f() -> Unit effects {} { asdfqwerty }");
        let errs: Vec<&Diagnostic> = diags
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        assert!(!errs.is_empty(), "expected an error for unknown identifier");
        let has_suggestion = errs.iter().any(|d| d.msg.contains("did you mean"));
        assert!(
            !has_suggestion,
            "should not suggest anything for random text, got: {:?}",
            errs
        );
    }

    #[test]
    fn refuted_message_includes_witness() {
        // result > 0 is false when body is -1.
        let diags = check("fn neg(n: Int) -> Int where result > 0 effects {} { -1 }");
        let has_counter = diags.iter().any(|d| {
            (d.severity == Severity::Error || d.severity == Severity::Warning)
                && d.msg.contains("counterexample")
        });
        assert!(
            has_counter,
            "expected 'counterexample' in refuted postcondition message, got: {:?}",
            diags
        );
    }
}

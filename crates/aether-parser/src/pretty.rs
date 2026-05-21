//! Pretty-printers for both surface syntaxes.
//!
//! `compact` reproduces the dense `.ae` form. `verbose` produces the readable
//! human projection. Both are deterministic for the same AST: `parse(compact(parse(x)))`
//! is invariant under repeated application.

use aether_ast::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Form {
    Compact,
    Verbose,
}

pub fn module(m: &Module, form: Form) -> String {
    let mut out = String::new();
    if let Some(doc) = &m.doc {
        for line in doc.lines() {
            out.push_str("## ");
            out.push_str(line);
            out.push('\n');
        }
    }
    for (i, d) in m.decls.iter().enumerate() {
        if i > 0 {
            out.push_str("\n\n");
        }
        out.push_str(&decl(d, form));
    }
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

pub fn decl(d: &Decl, form: Form) -> String {
    let mut out = String::new();
    if let Some(doc) = d.doc() {
        for line in doc.lines() {
            out.push_str("## ");
            out.push_str(line);
            out.push('\n');
        }
    }
    match d {
        Decl::Fn(f) => fn_decl(f, form, &mut out),
        Decl::Let(l) => {
            out.push_str("let ");
            out.push_str(&l.name);
            if let Some(t) = &l.ty {
                out.push_str(": ");
                out.push_str(&type_(t, form));
            }
            out.push_str(" = ");
            out.push_str(&expr(&l.value, form));
        }
        Decl::TypeAlias(t) => {
            out.push_str("type ");
            out.push_str(&t.name);
            if !t.generics.is_empty() {
                out.push('<');
                out.push_str(&t.generics.join(", "));
                out.push('>');
            }
            out.push_str(" = ");
            out.push_str(&type_(&t.ty, form));
        }
        Decl::Import(i) => {
            out.push_str("import ");
            if !i.names.is_empty() {
                out.push('{');
                out.push_str(&i.names.join(", "));
                out.push_str("} from ");
            }
            out.push_str(&i.path.join("::"));
            if let Some(a) = &i.alias {
                out.push_str(" as ");
                out.push_str(a);
            }
        }
        Decl::Tool(t) => {
            out.push_str("tool ");
            out.push_str(&t.name);
            out.push('(');
            out.push_str(&params(&t.params, form));
            out.push_str(") -> ");
            out.push_str(&type_(&t.ret, form));
            if !t.effects.is_pure() {
                out.push_str(" !");
                out.push_str(&effects(&t.effects));
            }
        }
    }
    out
}

fn fn_decl(f: &FnDecl, form: Form, out: &mut String) {
    if f.no_prov {
        out.push_str("@no_prov\n");
    }
    match form {
        Form::Verbose => {
            out.push_str("fn ");
            out.push_str(&f.name);
            if !f.generics.is_empty() {
                out.push('<');
                out.push_str(&f.generics.join(", "));
                out.push('>');
            }
            out.push('(');
            out.push_str(&params(&f.params, form));
            out.push_str(") -> ");
            out.push_str(&type_(&f.ret, form));
            if !f.spec.ensures.is_empty() {
                out.push_str(" where ");
                for (i, e) in f.spec.ensures.iter().enumerate() {
                    if i > 0 {
                        out.push_str(" && ");
                    }
                    out.push_str(&expr(e, form));
                }
            }
            out.push_str(" effects ");
            out.push_str(&effects(&f.effects));
            out.push_str(" { ");
            out.push_str(&expr_body(&f.body, form));
            out.push_str(" }");
        }
        Form::Compact => {
            out.push_str(&f.name);
            if !f.generics.is_empty() {
                out.push('<');
                out.push_str(&f.generics.join(","));
                out.push('>');
            }
            out.push('(');
            out.push_str(&params(&f.params, form));
            out.push_str("):");
            out.push_str(&type_(&f.ret, form));
            out.push('!');
            out.push_str(&effects(&f.effects));
            if !f.spec.ensures.is_empty() {
                out.push_str(" where ");
                for (i, e) in f.spec.ensures.iter().enumerate() {
                    if i > 0 {
                        out.push_str("&&");
                    }
                    out.push_str(&expr(e, form));
                }
            }
            out.push_str(" = ");
            out.push_str(&expr_body(&f.body, form));
        }
    }
}

fn params(ps: &[Param], form: Form) -> String {
    ps.iter()
        .map(|p| {
            let mut s = String::new();
            s.push_str(&p.name);
            s.push(':');
            s.push_str(&type_(&p.ty, form));
            if let Some(d) = &p.default {
                s.push('=');
                s.push_str(&expr(d, form));
            }
            s
        })
        .collect::<Vec<_>>()
        .join(if form == Form::Verbose { ", " } else { "," })
}

fn effects(e: &EffectRow) -> String {
    let mut s = String::from("{");
    let mut first = true;
    for eff in &e.effects {
        if !first {
            s.push(',');
        }
        first = false;
        s.push_str(eff.as_str());
    }
    if let Some(t) = &e.tail {
        if !first {
            s.push(',');
        }
        s.push_str(t);
    }
    s.push('}');
    s
}

pub fn type_(t: &Type, form: Form) -> String {
    match t {
        Type::Var(s, _) => s.clone(),
        Type::Con(c, _) => match form {
            Form::Compact => c.compact_name().to_string(),
            Form::Verbose => c.verbose_name().to_string(),
        },
        Type::Fun {
            params: ps,
            ret,
            effects: eff,
            ..
        } => {
            let mut s = String::new();
            if ps.len() == 1 {
                s.push_str(&type_(&ps[0], form));
            } else {
                s.push('(');
                for (i, p) in ps.iter().enumerate() {
                    if i > 0 {
                        s.push(',');
                    }
                    s.push_str(&type_(p, form));
                }
                s.push(')');
            }
            s.push_str(" -> ");
            s.push_str(&type_(ret, form));
            if !eff.is_pure() {
                s.push_str(" !");
                s.push_str(&effects(eff));
            }
            s
        }
        Type::Refined {
            base, refinement, ..
        } => {
            let mut s = type_(base, form);
            s.push('{');
            s.push_str(&refinement.binder);
            s.push(':');
            s.push_str(&expr(&refinement.pred, form));
            s.push('}');
            s
        }
        Type::Tuple(elts, _) => {
            let mut s = String::from("(");
            for (i, e) in elts.iter().enumerate() {
                if i > 0 {
                    s.push(',');
                }
                s.push_str(&type_(e, form));
            }
            s.push(')');
            s
        }
        Type::List(inner, _) => {
            let mut s = String::from("[");
            s.push_str(&type_(inner, form));
            s.push(']');
            s
        }
        Type::Record(fields, _) => {
            let mut s = String::from("{");
            for (i, (n, t)) in fields.iter().enumerate() {
                if i > 0 {
                    s.push(',');
                }
                s.push_str(n);
                s.push(':');
                s.push_str(&type_(t, form));
            }
            s.push('}');
            s
        }
        Type::Sum(parts, _) => parts
            .iter()
            .map(|p| type_(p, form))
            .collect::<Vec<_>>()
            .join("|"),
        Type::Option(inner, _) => {
            let mut s = type_(inner, form);
            s.push('?');
            s
        }
        Type::Confidence { base, p, .. } => {
            let mut s = type_(base, form);
            s.push_str(" ~ confidence(");
            s.push_str(&expr(p, form));
            s.push(')');
            s
        }
        Type::Generic { name, args, .. } => {
            let mut s = name.clone();
            if !args.is_empty() {
                s.push('<');
                for (i, a) in args.iter().enumerate() {
                    if i > 0 {
                        s.push(',');
                    }
                    s.push_str(&type_(a, form));
                }
                s.push('>');
            }
            s
        }
        Type::Adt { ctors, .. } => ctors
            .iter()
            .map(|(cname, fields)| {
                if fields.is_empty() {
                    cname.clone()
                } else {
                    let mut s = cname.clone();
                    s.push('(');
                    for (i, f) in fields.iter().enumerate() {
                        if i > 0 {
                            s.push(',');
                        }
                        s.push_str(&type_(f, form));
                    }
                    s.push(')');
                    s
                }
            })
            .collect::<Vec<_>>()
            .join("|"),
    }
}

pub fn expr(e: &Expr, form: Form) -> String {
    expr_prec(e, form, 0)
}

fn expr_body(e: &Expr, form: Form) -> String {
    expr(e, form)
}

fn expr_prec(e: &Expr, form: Form, parent_bp: u8) -> String {
    match e {
        Expr::Lit(l, _) => lit(l),
        Expr::Var(s, _) => s.clone(),
        Expr::Bin(op, l, r, _) => {
            let (lbp, rbp, my_bp) = op_bp(*op);
            let inner = format!(
                "{}{}{}",
                expr_prec(l, form, lbp),
                if form == Form::Verbose {
                    format!(" {} ", op.as_str())
                } else {
                    op.as_str().to_string()
                },
                expr_prec(r, form, rbp + 1)
            );
            if my_bp < parent_bp {
                format!("({inner})")
            } else {
                inner
            }
        }
        Expr::Un(op, x, _) => format!("{}{}", op.as_str(), expr_prec(x, form, u8::MAX)),
        Expr::Call { callee, args, .. } => {
            let mut s = expr_prec(callee, form, u8::MAX);
            s.push('(');
            for (i, a) in args.iter().enumerate() {
                if i > 0 {
                    s.push(',');
                    if form == Form::Verbose {
                        s.push(' ');
                    }
                }
                if let Some(n) = &a.name {
                    s.push_str(n);
                    s.push('=');
                }
                s.push_str(&expr(&a.value, form));
            }
            s.push(')');
            s
        }
        Expr::Lambda {
            params: ps, body, ..
        } => {
            let mut s = String::from("fn(");
            s.push_str(&params(ps, form));
            s.push_str(") => ");
            s.push_str(&expr(body, form));
            s
        }
        Expr::Let {
            pat, value, body, ..
        } => {
            format!(
                "let {} = {} in {}",
                pattern(pat),
                expr(value, form),
                expr(body, form)
            )
        }
        Expr::If {
            cond,
            then_branch,
            else_branch,
            ..
        } => {
            format!(
                "if {} then {} else {}",
                expr(cond, form),
                expr(then_branch, form),
                expr(else_branch, form)
            )
        }
        Expr::Block { stmts, tail, .. } => {
            let mut s = String::from("{ ");
            for st in stmts {
                match st {
                    Stmt::Let { pat, value, .. } => {
                        s.push_str("let ");
                        s.push_str(&pattern(pat));
                        s.push_str(" = ");
                        s.push_str(&expr(value, form));
                        s.push_str("; ");
                    }
                    Stmt::Expr(e) => {
                        s.push_str(&expr(e, form));
                        s.push_str("; ");
                    }
                }
            }
            if let Some(t) = tail {
                s.push_str(&expr(t, form));
                s.push(' ');
            }
            s.push('}');
            s
        }
        Expr::Record(fields, _) => {
            let mut s = String::from("{");
            for (i, (n, v)) in fields.iter().enumerate() {
                if i > 0 {
                    s.push(',');
                }
                s.push_str(n);
                s.push(':');
                s.push_str(&expr(v, form));
            }
            s.push('}');
            s
        }
        Expr::Tuple(elts, _) => {
            let mut s = String::from("(");
            for (i, x) in elts.iter().enumerate() {
                if i > 0 {
                    s.push(',');
                }
                s.push_str(&expr(x, form));
            }
            s.push(')');
            s
        }
        Expr::List(elts, _) => {
            let mut s = String::from("[");
            for (i, x) in elts.iter().enumerate() {
                if i > 0 {
                    s.push(',');
                }
                s.push_str(&expr(x, form));
            }
            s.push(']');
            s
        }
        Expr::Field(e, n, _) => format!("{}.{}", expr_prec(e, form, u8::MAX), n),
        Expr::Index(e, i, _) => format!("{}[{}]", expr_prec(e, form, u8::MAX), expr(i, form)),
        Expr::Match { .. } => "<match>".to_string(),
        Expr::Confident { value, p, .. } => {
            format!("confident({},{})", expr(value, form), expr(p, form))
        }
        Expr::Assume(p, _) => format!("assume({})", expr(p, form)),
        Expr::Annot { expr: e, ty, .. } => format!("({}: {})", expr(e, form), type_(ty, form)),
        Expr::StrInterp { parts, .. } => {
            let mut s = String::from("\"");
            for p in parts {
                match p {
                    StrPart::Lit(t) => {
                        for c in t.chars() {
                            match c {
                                '\\' => s.push_str("\\\\"),
                                '"' => s.push_str("\\\""),
                                '\n' => s.push_str("\\n"),
                                c => s.push(c),
                            }
                        }
                    }
                    StrPart::Expr(e) => {
                        s.push_str("${");
                        s.push_str(&expr(e, form));
                        s.push('}');
                    }
                }
            }
            s.push('"');
            s
        }
    }
}
fn pattern(p: &Pattern) -> String {
    match p {
        Pattern::Wild(_) => "_".to_string(),
        Pattern::Var(s, _) => s.clone(),
        Pattern::Lit(l, _) => lit(l),
        Pattern::Tuple(ps, _) => {
            let inner = ps.iter().map(pattern).collect::<Vec<_>>().join(",");
            format!("({inner})")
        }
        Pattern::Record(fs, _) => {
            let inner = fs
                .iter()
                .map(|(n, p)| format!("{n}:{}", pattern(p)))
                .collect::<Vec<_>>()
                .join(",");
            format!("{{{inner}}}")
        }
        Pattern::Ctor { name, args, .. } => {
            if args.is_empty() {
                name.clone()
            } else {
                let inner = args.iter().map(pattern).collect::<Vec<_>>().join(",");
                format!("{name}({inner})")
            }
        }
    }
}

fn lit(l: &Lit) -> String {
    match l {
        Lit::Int(n) => n.to_string(),
        Lit::Float(f) => {
            let s = f.to_string();
            if s.contains('.') {
                s
            } else {
                format!("{s}.0")
            }
        }
        Lit::Bool(b) => b.to_string(),
        Lit::Str(s) => format!("{s:?}"),
        Lit::Unit => "()".to_string(),
    }
}

fn op_bp(op: BinOp) -> (u8, u8, u8) {
    match op {
        BinOp::Or => (1, 2, 1),
        BinOp::And => (3, 4, 3),
        BinOp::Eq | BinOp::Neq | BinOp::Implies => (5, 6, 5),
        BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => (7, 8, 7),
        BinOp::Add | BinOp::Sub | BinOp::Concat => (9, 10, 9),
        BinOp::Mul | BinOp::Div | BinOp::Mod => (11, 12, 11),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_module;

    fn rt(src: &str) -> (String, String) {
        let m = parse_module(FileId(0), src).unwrap();
        (module(&m, Form::Compact), module(&m, Form::Verbose))
    }

    #[test]
    fn roundtrip_compact() {
        let src = "add(x:I,y:I):I!{} = x+y";
        let (compact, _) = rt(src);
        // Parse the compact emission again — should be valid Aether.
        let _ = parse_module(FileId(0), &compact).expect("re-parse compact");
    }

    #[test]
    fn roundtrip_verbose() {
        let src = "fn add(x: Int, y: Int) -> Int effects {} { x + y }";
        let (_, verbose) = rt(src);
        let _ = parse_module(FileId(0), &verbose).expect("re-parse verbose");
    }

    #[test]
    fn refinement_emits() {
        let src = "pos(n:I{n: n>0}):I!{} = n";
        let (compact, verbose) = rt(src);
        assert!(compact.contains("I{n:"));
        assert!(verbose.contains("Int{n:"));
    }
}

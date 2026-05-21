//! Runtime values with attached provenance.

use aether_ast::*;
use std::cmp::Ordering;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub enum Value {
    Int(i64, ProvChain),
    Float(f64, ProvChain),
    Bool(bool, ProvChain),
    Str(String, ProvChain),
    Unit(ProvChain),
    Tuple(Vec<Value>, ProvChain),
    List(Vec<Value>, ProvChain),
    Record(Vec<(String, Value)>, ProvChain),
    /// Reference to a user-defined function by name (closures unsupported in MVP).
    Fn(String, ProvChain),
    /// Result of `introspect(...)`.
    ModuleSurface(crate::builtins::ModuleSurface, ProvChain),
    /// A first-class provenance chain (returned by `provenance(...)`).
    ProvHandle(ProvChain, ProvChain),
    /// `confident(value, p)` wrapper.
    Confident {
        value: Box<Value>,
        p: f64,
        prov: ProvChain,
    },
    /// A first-class lambda capturing its definition-time environment.
    Closure {
        params: Vec<aether_ast::decl::Param>,
        body: Box<aether_ast::expr::Expr>,
        env: crate::env::Env,
        prov: ProvChain,
    },
    /// A tagged ADT value: `Circle(5.0)` → `Ctor { name: "Circle", args: [Float(5.0)], .. }`.
    Ctor {
        name: String,
        args: Vec<Value>,
        prov: ProvChain,
    },
}

impl Value {
    pub fn prov(&self) -> &ProvChain {
        use Value::*;
        match self {
            Int(_, p)
            | Float(_, p)
            | Bool(_, p)
            | Str(_, p)
            | Unit(p)
            | Tuple(_, p)
            | List(_, p)
            | Record(_, p)
            | Fn(_, p)
            | ModuleSurface(_, p)
            | ProvHandle(_, p) => p,
            Confident { prov, .. } => prov,
            Closure { prov, .. } => prov,
            Ctor { prov, .. } => prov,
        }
    }

    pub fn with_prov(self, prov: ProvChain) -> Self {
        use Value::*;
        match self {
            Int(n, _) => Int(n, prov),
            Float(f, _) => Float(f, prov),
            Bool(b, _) => Bool(b, prov),
            Str(s, _) => Str(s, prov),
            Unit(_) => Unit(prov),
            Tuple(v, _) => Tuple(v, prov),
            List(v, _) => List(v, prov),
            Record(v, _) => Record(v, prov),
            Fn(n, _) => Fn(n, prov),
            ModuleSurface(s, _) => ModuleSurface(s, prov),
            ProvHandle(c, _) => ProvHandle(c, prov),
            Confident { value, p, .. } => Confident { value, p, prov },
            Closure {
                params, body, env, ..
            } => Closure {
                params,
                body,
                env,
                prov,
            },
            Ctor { name, args, .. } => Ctor { name, args, prov },
        }
    }

    pub fn type_name(&self) -> &'static str {
        use Value::*;
        match self {
            Int(..) => "Int",
            Float(..) => "Float",
            Bool(..) => "Bool",
            Str(..) => "Str",
            Unit(_) => "Unit",
            Tuple(..) => "Tuple",
            List(..) => "List",
            Record(..) => "Record",
            Fn(..) => "Fn",
            ModuleSurface(..) => "ModuleSurface",
            ProvHandle(..) => "ProvChain",
            Confident { .. } => "Confidence",
            Closure { .. } => "Closure",
            Ctor { name, .. } => {
                // Return a static str isn't possible for a dynamic name, but type_name
                // is only used for error messages. Leak the string (acceptable in error path).
                Box::leak(name.clone().into_boxed_str())
            }
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        if let Value::Int(n, _) = self {
            Some(*n)
        } else {
            None
        }
    }
    pub fn as_float(&self) -> Option<f64> {
        match self {
            Value::Float(f, _) => Some(*f),
            Value::Int(n, _) => Some(*n as f64),
            _ => None,
        }
    }
    pub fn as_bool(&self) -> Option<bool> {
        if let Value::Bool(b, _) = self {
            Some(*b)
        } else {
            None
        }
    }
    pub fn as_str(&self) -> Option<&str> {
        if let Value::Str(s, _) = self {
            Some(s)
        } else {
            None
        }
    }

    /// Equality ignoring provenance.
    pub fn eq_val(&self, other: &Value) -> bool {
        use Value::*;
        match (self, other) {
            (Int(a, _), Int(b, _)) => a == b,
            (Float(a, _), Float(b, _)) => a == b,
            (Bool(a, _), Bool(b, _)) => a == b,
            (Str(a, _), Str(b, _)) => a == b,
            (Unit(_), Unit(_)) => true,
            (Tuple(a, _), Tuple(b, _)) | (List(a, _), List(b, _)) => {
                a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.eq_val(y))
            }
            (Record(a, _), Record(b, _)) => {
                a.len() == b.len()
                    && a.iter()
                        .zip(b)
                        .all(|((n1, v1), (n2, v2))| n1 == n2 && v1.eq_val(v2))
            }
            _ => false,
        }
    }

    /// Comparison ignoring provenance. Returns `None` if the values aren't ordered.
    pub fn cmp_val(&self, other: &Value) -> Option<Ordering> {
        use Value::*;
        match (self, other) {
            (Int(a, _), Int(b, _)) => Some(a.cmp(b)),
            (Float(a, _), Float(b, _)) => a.partial_cmp(b),
            (Str(a, _), Str(b, _)) => Some(a.cmp(b)),
            (Bool(a, _), Bool(b, _)) => Some(a.cmp(b)),
            _ => None,
        }
    }

    pub fn from_lit(l: &Lit, span: Span, arena: &Arc<ProvArena>) -> Self {
        let prov = ProvChain::singleton(arena.clone(), ProvOp::Lit, span);
        match l {
            Lit::Int(n) => Value::Int(*n, prov),
            Lit::Float(f) => Value::Float(*f, prov),
            Lit::Bool(b) => Value::Bool(*b, prov),
            Lit::Str(s) => Value::Str(s.clone(), prov),
            Lit::Unit => Value::Unit(prov),
        }
    }

    pub fn unit(arena: Arc<ProvArena>, span: Span) -> Self {
        Value::Unit(ProvChain::singleton(
            arena,
            ProvOp::Synthetic("unit".into()),
            span,
        ))
    }

    pub fn unit_with_prov(prov: ProvChain) -> Self {
        Value::Unit(prov)
    }

    /// Render the value for display (ignores provenance).
    pub fn display(&self) -> String {
        use Value::*;
        match self {
            Int(n, _) => n.to_string(),
            Float(f, _) => f.to_string(),
            Bool(b, _) => b.to_string(),
            Str(s, _) => s.clone(),
            Unit(_) => "()".to_string(),
            Tuple(v, _) => format!(
                "({})",
                v.iter().map(Value::display).collect::<Vec<_>>().join(", ")
            ),
            List(v, _) => format!(
                "[{}]",
                v.iter().map(Value::display).collect::<Vec<_>>().join(", ")
            ),
            Record(fs, _) => format!(
                "{{{}}}",
                fs.iter()
                    .map(|(n, v)| format!("{n}: {}", v.display()))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Fn(n, _) => format!("<fn {n}>"),
            ModuleSurface(ms, _) => ms.format(),
            ProvHandle(c, _) => format!("<prov head={}>", c.head),
            Confident { value, p, .. } => format!("{} ~confidence({p})", value.display()),
            Closure { params, .. } => {
                let names: Vec<&str> = params.iter().map(|p| p.name.as_str()).collect();
                format!("<closure({})>", names.join(", "))
            }
            Ctor { name, args, .. } => {
                if args.is_empty() {
                    name.clone()
                } else {
                    format!(
                        "{}({})",
                        name,
                        args.iter()
                            .map(Value::display)
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                }
            }
        }
    }
}

//! Type AST: base types, function types, refinements, effect rows.

use crate::expr::Expr;
use crate::span::Span;

/// Concrete base type constructors.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TyCon {
    Int,
    Float,
    Bool,
    Str,
    Bytes,
    Unit,
    /// Built-in result of `introspect(...)`.
    ModuleSurface,
    /// Built-in provenance chain type.
    ProvChain,
}

impl TyCon {
    pub fn compact_name(self) -> &'static str {
        match self {
            TyCon::Int => "I",
            TyCon::Float => "F",
            TyCon::Bool => "B",
            TyCon::Str => "Str",
            TyCon::Bytes => "Bytes",
            TyCon::Unit => "U",
            TyCon::ModuleSurface => "ModuleSurface",
            TyCon::ProvChain => "ProvChain",
        }
    }

    pub fn verbose_name(self) -> &'static str {
        match self {
            TyCon::Int => "Int",
            TyCon::Float => "Float",
            TyCon::Bool => "Bool",
            TyCon::Str => "Str",
            TyCon::Bytes => "Bytes",
            TyCon::Unit => "Unit",
            TyCon::ModuleSurface => "ModuleSurface",
            TyCon::ProvChain => "ProvChain",
        }
    }

    pub fn from_str(s: &str) -> Option<TyCon> {
        Some(match s {
            "I" | "Int" => TyCon::Int,
            "F" | "Float" => TyCon::Float,
            "B" | "Bool" => TyCon::Bool,
            "Str" => TyCon::Str,
            "Bytes" => TyCon::Bytes,
            "U" | "Unit" => TyCon::Unit,
            "ModuleSurface" => TyCon::ModuleSurface,
            "ProvChain" => TyCon::ProvChain,
            _ => return None,
        })
    }
}

/// Built-in effect labels. `Custom` is reserved for user-declared effects.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Effect {
    IO,
    Net,
    FS,
    State,
    Rand,
    Async,
    Throw,
    Custom(String),
}

impl Effect {
    pub fn as_str(&self) -> &str {
        match self {
            Effect::IO => "IO",
            Effect::Net => "Net",
            Effect::FS => "FS",
            Effect::State => "State",
            Effect::Rand => "Rand",
            Effect::Async => "Async",
            Effect::Throw => "Throw",
            Effect::Custom(s) => s,
        }
    }

    pub fn from_str(s: &str) -> Effect {
        match s {
            "IO" => Effect::IO,
            "Net" => Effect::Net,
            "FS" => Effect::FS,
            "State" => Effect::State,
            "Rand" => Effect::Rand,
            "Async" => Effect::Async,
            "Throw" => Effect::Throw,
            other => Effect::Custom(other.to_string()),
        }
    }
}

/// A row of effects. `tail` is `Some(name)` for row-polymorphic functions
/// (e.g. `f<E>: T -> U !E`).
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectRow {
    pub effects: Vec<Effect>,
    pub tail: Option<String>,
}

impl EffectRow {
    pub fn pure_() -> Self {
        EffectRow { effects: vec![], tail: None }
    }

    pub fn from_iter<I: IntoIterator<Item = Effect>>(iter: I) -> Self {
        let mut effects: Vec<Effect> = iter.into_iter().collect();
        effects.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        effects.dedup();
        EffectRow { effects, tail: None }
    }

    pub fn is_pure(&self) -> bool {
        self.effects.is_empty() && self.tail.is_none()
    }

    pub fn union(&self, other: &EffectRow) -> EffectRow {
        let mut effects = self.effects.clone();
        for e in &other.effects {
            if !effects.contains(e) {
                effects.push(e.clone());
            }
        }
        effects.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        let tail = match (&self.tail, &other.tail) {
            (Some(t), _) | (None, Some(t)) => Some(t.clone()),
            (None, None) => None,
        };
        EffectRow { effects, tail }
    }
}

/// A refinement: a predicate `pred` over a bound name `binder` of the base type.
///
/// `Int{n: n > 0}` becomes `Refinement { binder: "n", pred: parse("n > 0") }`.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq)]
pub struct Refinement {
    pub binder: String,
    pub pred: Box<Expr>,
    pub span: Span,
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    /// An inference / type variable. `String` is the variable name (e.g. `a`, `t0`).
    Var(String, Span),
    /// A base type constructor (`Int`, `Float`, etc.).
    Con(TyCon, Span),
    /// `(params) -> ret !{effects}`.
    Fun {
        params: Vec<Type>,
        ret: Box<Type>,
        effects: EffectRow,
        span: Span,
    },
    /// `Base{binder: pred}` — a refinement type.
    Refined {
        base: Box<Type>,
        refinement: Refinement,
        span: Span,
    },
    Tuple(Vec<Type>, Span),
    List(Box<Type>, Span),
    Record(Vec<(String, Type)>, Span),
    /// `T | U | ...`
    Sum(Vec<Type>, Span),
    /// `T?` — optional.
    Option(Box<Type>, Span),
    /// `T ~ confidence(p)` — confidence-tagged value.
    Confidence { base: Box<Type>, p: Box<Expr>, span: Span },
    /// User-defined or stdlib parameterized type, e.g. `Map<K, V>`.
    Generic { name: String, args: Vec<Type>, span: Span },
    /// Algebraic data type: `type Shape = Circle(Float) | Square(Float)`.
    /// `name` is the ADT name; `ctors` is the list of `(CtorName, [FieldType])`.
    Adt { name: String, ctors: Vec<(String, Vec<Type>)>, span: Span },
}

impl Type {
    pub fn span(&self) -> Span {
        match self {
            Type::Var(_, s) | Type::Con(_, s) | Type::Fun { span: s, .. }
            | Type::Refined { span: s, .. } | Type::Tuple(_, s) | Type::List(_, s)
            | Type::Record(_, s) | Type::Sum(_, s) | Type::Option(_, s)
            | Type::Confidence { span: s, .. } | Type::Generic { span: s, .. }
            | Type::Adt { span: s, .. } => *s,
        }
    }

    /// True if the type is syntactically pure (no refinement, no row effects, base only).
    pub fn is_base(&self) -> bool {
        matches!(self, Type::Con(_, _))
    }

    /// Strip any refinement / confidence layer and return the base type.
    pub fn unrefined(&self) -> &Type {
        match self {
            Type::Refined { base, .. } | Type::Confidence { base, .. } => base.unrefined(),
            other => other,
        }
    }
}

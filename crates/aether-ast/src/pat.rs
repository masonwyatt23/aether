//! Pattern AST.

use crate::expr::Lit;
use crate::span::Span;

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq)]
pub enum Pattern {
    Wild(Span),
    Var(String, Span),
    Lit(Lit, Span),
    Tuple(Vec<Pattern>, Span),
    Record(Vec<(String, Pattern)>, Span),
    /// `Some(x)`, `Ok(v)`, etc.
    Ctor {
        name: String,
        args: Vec<Pattern>,
        span: Span,
    },
}

impl Pattern {
    pub fn span(&self) -> Span {
        match self {
            Pattern::Wild(s)
            | Pattern::Var(_, s)
            | Pattern::Lit(_, s)
            | Pattern::Tuple(_, s)
            | Pattern::Record(_, s)
            | Pattern::Ctor { span: s, .. } => *s,
        }
    }
}

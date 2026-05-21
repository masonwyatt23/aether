//! Expression AST.

use crate::pat::Pattern;
use crate::span::Span;
use crate::ty::Type;

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq)]
pub enum Lit {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    Unit,
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BinOp {
    Add, Sub, Mul, Div, Mod,
    Eq, Neq, Lt, Le, Gt, Ge,
    And, Or,
    Concat,        // ++ on strings / lists
    Implies,       // => for refinements
}

impl BinOp {
    pub fn as_str(self) -> &'static str {
        match self {
            BinOp::Add => "+", BinOp::Sub => "-", BinOp::Mul => "*",
            BinOp::Div => "/", BinOp::Mod => "%",
            BinOp::Eq => "==", BinOp::Neq => "!=",
            BinOp::Lt => "<", BinOp::Le => "<=",
            BinOp::Gt => ">", BinOp::Ge => ">=",
            BinOp::And => "&&", BinOp::Or => "||",
            BinOp::Concat => "++",
            BinOp::Implies => "=>",
        }
    }
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UnOp {
    Neg,
    Not,
}

impl UnOp {
    pub fn as_str(self) -> &'static str {
        match self {
            UnOp::Neg => "-",
            UnOp::Not => "!",
        }
    }
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq)]
pub struct Arg {
    /// Some(name) for keyword arguments like `f(x=1)`, None for positional.
    pub name: Option<String>,
    pub value: Expr,
    pub span: Span,
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq)]
pub struct MatchArm {
    pub pat: Pattern,
    pub guard: Option<Expr>,
    pub body: Expr,
    pub span: Span,
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    Let {
        pat: Pattern,
        ty: Option<Type>,
        value: Expr,
        span: Span,
    },
    Expr(Expr),
}

impl Stmt {
    pub fn span(&self) -> Span {
        match self {
            Stmt::Let { span, .. } => *span,
            Stmt::Expr(e) => e.span(),
        }
    }
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Lit(Lit, Span),
    Var(String, Span),
    Bin(BinOp, Box<Expr>, Box<Expr>, Span),
    Un(UnOp, Box<Expr>, Span),
    Call { callee: Box<Expr>, args: Vec<Arg>, span: Span },
    Lambda { params: Vec<crate::decl::Param>, ret: Option<Type>, body: Box<Expr>, span: Span },
    Let { pat: Pattern, ty: Option<Type>, value: Box<Expr>, body: Box<Expr>, span: Span },
    If { cond: Box<Expr>, then_branch: Box<Expr>, else_branch: Box<Expr>, span: Span },
    Block { stmts: Vec<Stmt>, tail: Option<Box<Expr>>, span: Span },
    Record(Vec<(String, Expr)>, Span),
    Tuple(Vec<Expr>, Span),
    List(Vec<Expr>, Span),
    Field(Box<Expr>, String, Span),
    Index(Box<Expr>, Box<Expr>, Span),
    Match { scrutinee: Box<Expr>, arms: Vec<MatchArm>, span: Span },
    /// `confident(value, p)` - tag a value with confidence probability.
    Confident { value: Box<Expr>, p: Box<Expr>, span: Span },
    /// `assume(predicate)` - inject an axiom into the solver context, surfaces in provenance.
    Assume(Box<Expr>, Span),
    /// `value : Type` ascription.
    Annot { expr: Box<Expr>, ty: Type, span: Span },
    /// Interpolated string `"hello ${name}, you are ${age} years"`.
    /// `parts` alternates literal text chunks and embedded expressions.
    StrInterp { parts: Vec<StrPart>, span: Span },
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq)]
pub enum StrPart {
    Lit(String),
    Expr(Expr),
}

impl Expr {
    pub fn span(&self) -> Span {
        match self {
            Expr::Lit(_, s) | Expr::Var(_, s) | Expr::Record(_, s) | Expr::Tuple(_, s)
            | Expr::List(_, s) | Expr::Assume(_, s) => *s,
            Expr::Bin(_, _, _, s) | Expr::Un(_, _, s) | Expr::Call { span: s, .. }
            | Expr::Lambda { span: s, .. } | Expr::Let { span: s, .. }
            | Expr::If { span: s, .. } | Expr::Block { span: s, .. }
            | Expr::Field(_, _, s) | Expr::Index(_, _, s)
            | Expr::Match { span: s, .. } | Expr::Confident { span: s, .. }
            | Expr::Annot { span: s, .. } | Expr::StrInterp { span: s, .. } => *s,
        }
    }
}

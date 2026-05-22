//! Declarations: functions, lets, modules, imports, specs, tools, type aliases.

use crate::expr::Expr;
use crate::pat::Pattern;
use crate::span::Span;
use crate::ty::{EffectRow, Type};

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: String,
    pub ty: Type,
    pub default: Option<Expr>,
    pub span: Span,
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SpecBlock {
    pub requires: Vec<Expr>,
    pub ensures: Vec<Expr>,
    /// Effects declared in a `spec { effects {...} }` block (cumulative with header).
    pub effects: Option<EffectRow>,
    /// Termination measure from a `decreases <expr>` clause. Checked against
    /// every direct self-call; `None` means recursion is not termination-checked.
    /// Boxed so an absent measure does not inflate every `SpecBlock`.
    pub decreases: Option<Box<Expr>>,
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq)]
pub struct FnDecl {
    pub name: String,
    pub generics: Vec<String>,
    pub params: Vec<Param>,
    pub ret: Type,
    pub effects: EffectRow,
    pub spec: SpecBlock,
    pub body: Expr,
    pub doc: Option<String>,
    /// True if marked with `@no_prov` (opt out of provenance for hot paths).
    pub no_prov: bool,
    pub span: Span,
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq)]
pub struct LetDecl {
    pub name: String,
    pub ty: Option<Type>,
    pub value: Expr,
    pub doc: Option<String>,
    pub span: Span,
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq)]
pub struct TypeAliasDecl {
    pub name: String,
    pub generics: Vec<String>,
    pub ty: Type,
    pub span: Span,
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq)]
pub struct ImportDecl {
    pub path: Vec<String>,
    pub names: Vec<String>,
    pub alias: Option<String>,
    pub span: Span,
}

/// `tool name(args) -> ret !{Net,Throw}` — first-class external tool declaration.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq)]
pub struct ToolDecl {
    pub name: String,
    pub params: Vec<Param>,
    pub ret: Type,
    pub effects: EffectRow,
    pub doc: Option<String>,
    pub span: Span,
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq)]
pub enum Decl {
    Fn(FnDecl),
    Let(LetDecl),
    TypeAlias(TypeAliasDecl),
    Import(ImportDecl),
    Tool(ToolDecl),
}

impl Decl {
    pub fn name(&self) -> &str {
        match self {
            Decl::Fn(f) => &f.name,
            Decl::Let(l) => &l.name,
            Decl::TypeAlias(t) => &t.name,
            Decl::Tool(t) => &t.name,
            Decl::Import(_) => "<import>",
        }
    }

    pub fn doc(&self) -> Option<&str> {
        match self {
            Decl::Fn(f) => f.doc.as_deref(),
            Decl::Let(l) => l.doc.as_deref(),
            Decl::Tool(t) => t.doc.as_deref(),
            _ => None,
        }
    }

    pub fn span(&self) -> Span {
        match self {
            Decl::Fn(f) => f.span,
            Decl::Let(l) => l.span,
            Decl::TypeAlias(t) => t.span,
            Decl::Import(i) => i.span,
            Decl::Tool(t) => t.span,
        }
    }
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq)]
pub struct Module {
    pub name: Option<String>,
    pub doc: Option<String>,
    pub decls: Vec<Decl>,
    pub span: Span,
}

#[allow(dead_code)]
fn _unused_pattern_marker(_: &Pattern) {}

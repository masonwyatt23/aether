//! Aether AST.
//!
//! One canonical AST is shared by the compact and verbose surface syntaxes,
//! by every compiler pass (lex / parse / type / eval), and by the runtime
//! provenance machinery. Span tracking is mandatory on every node so that
//! diagnostics, introspection, and provenance trails all line up with source.

#![allow(clippy::module_inception)]

pub mod span;
pub mod expr;
pub mod pat;
pub mod ty;
pub mod decl;
pub mod prov;

pub use decl::{Decl, FnDecl, ImportDecl, LetDecl, Module, Param, SpecBlock, ToolDecl, TypeAliasDecl};
pub use expr::{Arg, BinOp, Expr, Lit, MatchArm, Stmt, StrPart, UnOp};
pub use pat::Pattern;
pub use prov::{ProvArena, ProvChain, ProvNode, ProvOp};
pub use span::{FileId, SourceMap, Span};
pub use ty::{Effect, EffectRow, Refinement, TyCon, Type};

#[cfg(all(test, feature = "serde"))]
mod serde_tests {
    use super::*;

    fn make_module() -> Module {
        Module {
            name: Some("hello".into()),
            doc: None,
            decls: vec![Decl::Fn(FnDecl {
                name: "main".into(),
                generics: vec![],
                params: vec![],
                ret: Type::Con(TyCon::Unit, Span::DUMMY),
                effects: EffectRow::from_iter([Effect::IO]),
                spec: SpecBlock::default(),
                body: Expr::Lit(Lit::Str("Hello, Aether!".into()), Span::DUMMY),
                doc: None,
                no_prov: false,
                span: Span::DUMMY,
            })],
            span: Span::DUMMY,
        }
    }

    #[test]
    fn roundtrip() {
        let m = make_module();
        let json = serde_json::to_string(&m).expect("serialize");
        let m2: Module = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(m, m2);
    }

    #[test]
    fn spans_preserved() {
        let fid = FileId(42);
        let span = Span::new(fid, 10..20);
        let json = serde_json::to_string(&span).expect("serialize span");
        let s2: Span = serde_json::from_str(&json).expect("deserialize span");
        assert_eq!(s2.file.0, 42);
        assert_eq!(s2.start, 10);
        assert_eq!(s2.end, 20);
    }

    #[test]
    fn deterministic() {
        let m = make_module();
        let j1 = serde_json::to_string(&m).expect("first serialize");
        let j2 = serde_json::to_string(&m).expect("second serialize");
        assert_eq!(j1, j2);
    }
}

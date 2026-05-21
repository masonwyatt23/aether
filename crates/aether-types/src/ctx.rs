//! Symbol tables for the type checker.

use aether_ast::*;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct FnSig {
    /// Type-parameter names declared as `fn name<A, B>(...)`. Empty for a
    /// non-generic function. At a call site these are instantiated to the
    /// concrete argument types.
    pub generics: Vec<String>,
    pub params: Vec<(String, Type)>,
    pub ret: Type,
    pub effects: EffectRow,
    pub requires: Vec<Expr>,
    pub ensures: Vec<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, Default)]
pub struct TypeCtx {
    /// Function signatures (user fns + tools + builtins).
    pub funs: HashMap<String, FnSig>,
    /// Top-level `let` bindings.
    pub lets: HashMap<String, Type>,
    /// Type aliases: name -> (generics, body).
    pub aliases: HashMap<String, (Vec<String>, Type)>,
    /// Constructor table: CtorName -> (AdtName, field_types).
    /// Built during pass 1 from every `type T = A(T1) | B(T2, T3)` declaration.
    pub ctors: HashMap<String, (String, Vec<Type>)>,
}

impl TypeCtx {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn lookup_fn(&self, name: &str) -> Option<&FnSig> {
        self.funs.get(name)
    }

    pub fn lookup_let(&self, name: &str) -> Option<&Type> {
        self.lets.get(name)
    }

    pub fn insert_fn(&mut self, name: String, sig: FnSig) {
        self.funs.insert(name, sig);
    }

    pub fn insert_let(&mut self, name: String, ty: Type) {
        self.lets.insert(name, ty);
    }

    pub fn insert_ctor(&mut self, ctor_name: String, adt_name: String, fields: Vec<Type>) {
        self.ctors.insert(ctor_name, (adt_name, fields));
    }

    pub fn lookup_ctor(&self, ctor_name: &str) -> Option<&(String, Vec<Type>)> {
        self.ctors.get(ctor_name)
    }
}

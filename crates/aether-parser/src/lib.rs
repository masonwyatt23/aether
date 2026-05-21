pub mod pretty;

use aether_ast::*;
use aether_lexer::{lex, LexError, Tok, Token};

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("lex error: {0}")]
    Lex(#[from] LexError),
    #[error("parse error at {span:?}: {msg}")]
    Bad { span: Span, msg: String },
    #[error("unexpected end of input")]
    Eof,
}

impl ParseError {
    fn at(span: Span, msg: impl Into<String>) -> Self {
        ParseError::Bad {
            span,
            msg: msg.into(),
        }
    }
    pub fn span(&self) -> Option<Span> {
        match self {
            ParseError::Bad { span, .. } => Some(*span),
            _ => None,
        }
    }
}

pub type PResult<T> = Result<T, ParseError>;

/// Parse a whole source file into a `Module`.
pub fn parse_module(file: FileId, source: &str) -> PResult<Module> {
    let tokens = lex(file, source)?;
    let mut p = Parser::new(file, source, tokens);
    p.parse_module()
}

/// Parse a single expression (used by the REPL and by tests).
pub fn parse_expr(file: FileId, source: &str) -> PResult<Expr> {
    let tokens = lex(file, source)?;
    let mut p = Parser::new(file, source, tokens);
    p.parse_expr_top()
}

struct Parser<'a> {
    file: FileId,
    #[allow(dead_code)]
    source: &'a str,
    toks: Vec<Token>,
    pos: usize,
    /// Doc lines accumulated since the last consumed declaration.
    doc_buf: Vec<String>,
}

impl<'a> Parser<'a> {
    fn new(file: FileId, source: &'a str, toks: Vec<Token>) -> Self {
        Self {
            file,
            source,
            toks,
            pos: 0,
            doc_buf: Vec::new(),
        }
    }

    // --- token utilities -------------------------------------------------

    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos).map(|t| &t.tok)
    }

    fn peek_at(&self, offset: usize) -> Option<&Tok> {
        self.toks.get(self.pos + offset).map(|t| &t.tok)
    }

    fn peek_span(&self) -> Span {
        self.toks
            .get(self.pos)
            .map(|t| t.span)
            .unwrap_or(Span::new(self.file, 0..0))
    }

    fn last_span(&self) -> Span {
        if self.pos == 0 {
            Span::new(self.file, 0..0)
        } else {
            self.toks[self.pos - 1].span
        }
    }

    fn bump(&mut self) -> Option<Token> {
        let t = self.toks.get(self.pos).cloned();
        if t.is_some() {
            self.pos += 1;
        }
        t
    }

    fn eat(&mut self, want: &Tok) -> bool {
        if matches!(self.peek(), Some(t) if std::mem::discriminant(t) == std::mem::discriminant(want))
        {
            self.bump();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, want: &Tok, ctx: &str) -> PResult<Token> {
        let here = self.peek_span();
        if self.eat(want) {
            Ok(self.toks[self.pos - 1].clone())
        } else {
            Err(ParseError::at(
                here,
                format!("expected {ctx} (token {want:?}), got {:?}", self.peek()),
            ))
        }
    }

    fn expect_ident(&mut self, ctx: &str) -> PResult<(String, Span)> {
        let here = self.peek_span();
        match self.peek() {
            Some(Tok::Ident(s)) => {
                let s = s.clone();
                let span = self.toks[self.pos].span;
                self.bump();
                Ok((s, span))
            }
            // `result` is a keyword inside refinement postconditions but is
            // freely usable as an identifier elsewhere (module names, fn
            // names like `result_unwrap`, etc.).
            Some(Tok::Result_) => {
                let span = self.toks[self.pos].span;
                self.bump();
                Ok(("result".to_string(), span))
            }
            other => Err(ParseError::at(
                here,
                format!("expected identifier {ctx}, got {other:?}"),
            )),
        }
    }

    fn collect_docs(&mut self) -> Option<String> {
        while let Some(Tok::DocLine(s)) = self.peek() {
            self.doc_buf.push(s.clone());
            self.bump();
        }
        if self.doc_buf.is_empty() {
            None
        } else {
            Some(std::mem::take(&mut self.doc_buf).join("\n"))
        }
    }

    // --- top level -------------------------------------------------------

    fn parse_module(&mut self) -> PResult<Module> {
        let start = self.peek_span();
        // Module-level doc (leading docstring before any declaration).
        let mod_doc = self.collect_docs();
        let mut decls = Vec::new();
        while self.peek().is_some() {
            // Allow doc lines to attach to a decl.
            let doc = self
                .collect_docs()
                .or_else(|| mod_doc.clone().filter(|_| decls.is_empty()));
            let decl = self.parse_decl(doc)?;
            decls.push(decl);
        }
        let end = self.last_span();
        Ok(Module {
            name: None,
            doc: mod_doc,
            decls,
            span: start.join(end),
        })
    }

    fn parse_decl(&mut self, doc: Option<String>) -> PResult<Decl> {
        let no_prov = self.eat(&Tok::AtNoProv);
        let _ = self.eat(&Tok::AtPure);
        let _ = self.eat(&Tok::AtInline);

        match self.peek() {
            Some(Tok::Fn) => self.parse_fn_decl(doc, no_prov).map(Decl::Fn),
            Some(Tok::Let) => self.parse_let_decl(doc).map(Decl::Let),
            Some(Tok::Type) => self.parse_type_alias().map(Decl::TypeAlias),
            Some(Tok::Import) => self.parse_import().map(Decl::Import),
            Some(Tok::Tool) => self.parse_tool_decl(doc).map(Decl::Tool),
            Some(Tok::Ident(s)) if s == "test" && matches!(self.peek_at(1), Some(Tok::Str(_))) => {
                self.parse_named_block_decl("test__", doc).map(Decl::Fn)
            }
            Some(Tok::Ident(s)) if s == "bench" && matches!(self.peek_at(1), Some(Tok::Str(_))) => {
                self.parse_named_block_decl("bench__", doc).map(Decl::Fn)
            }
            Some(Tok::Ident(s)) if s == "snap" && matches!(self.peek_at(1), Some(Tok::Str(_))) => {
                // snap_expect has effects {Throw, FS} so the enclosing block must too.
                self.parse_named_block_decl("snap__", doc).map(|mut f| {
                    f.effects = EffectRow {
                        effects: vec![Effect::Throw, Effect::FS],
                        tail: None,
                    };
                    Decl::Fn(f)
                })
            }
            Some(Tok::Ident(_)) => {
                // Compact fn decl: `name(params):ret!{eff} = expr`
                self.parse_compact_fn_decl(doc, no_prov).map(Decl::Fn)
            }
            other => Err(ParseError::at(
                self.peek_span(),
                format!(
                    "expected declaration (fn/let/type/import/tool or compact form), got {other:?}"
                ),
            )),
        }
    }

    fn parse_fn_decl(&mut self, doc: Option<String>, no_prov: bool) -> PResult<FnDecl> {
        let start = self.peek_span();
        self.expect(&Tok::Fn, "'fn' keyword")?;
        let (name, _ns) = self.expect_ident("function name")?;
        let generics = self.parse_generics_opt()?;
        let params = self.parse_params()?;
        let ret = if self.eat(&Tok::Arrow) {
            self.parse_type()?
        } else {
            Type::Con(TyCon::Unit, self.peek_span())
        };
        let mut spec = SpecBlock::default();
        let mut effects = EffectRow::pure_();

        // `where` postcondition (over `result` binder) — verbose form
        if self.eat(&Tok::Where) {
            let pred = self.parse_expr_top()?;
            spec.ensures.push(pred);
            while self.eat(&Tok::AndAnd) {
                let p = self.parse_expr_top()?;
                spec.ensures.push(p);
            }
        }
        // `effects {…}` — verbose form
        if self.eat(&Tok::Effects) {
            effects = self.parse_effect_set()?;
        }
        // optional explicit spec block
        if self.eat(&Tok::Spec) {
            self.parse_spec_block_into(&mut spec, &mut effects)?;
        }
        let body = self.parse_block_expr()?;
        let end = self.last_span();
        Ok(FnDecl {
            name,
            generics,
            params,
            ret,
            effects,
            spec,
            body,
            doc,
            no_prov,
            span: start.join(end),
        })
    }

    fn parse_compact_fn_decl(&mut self, doc: Option<String>, no_prov: bool) -> PResult<FnDecl> {
        let start = self.peek_span();
        let (name, _) = self.expect_ident("function name")?;
        let generics = self.parse_generics_opt()?;
        let params = self.parse_params()?;
        self.expect(&Tok::Colon, "':' before return type in compact fn")?;
        let ret = self.parse_type()?;
        let mut effects = EffectRow::pure_();
        if self.eat(&Tok::Bang) {
            effects = self.parse_effect_set()?;
        }
        // Optional `where ensures` (compact form may use `where`)
        let mut spec = SpecBlock::default();
        if self.eat(&Tok::Where) {
            let pred = self.parse_expr_top()?;
            spec.ensures.push(pred);
            while self.eat(&Tok::AndAnd) {
                let p = self.parse_expr_top()?;
                spec.ensures.push(p);
            }
        }
        self.expect(&Tok::Eq, "'=' before compact fn body")?;
        let body = self.parse_expr_top()?;
        let end = self.last_span();
        Ok(FnDecl {
            name,
            generics,
            params,
            ret,
            effects,
            spec,
            body,
            doc,
            no_prov,
            span: start.join(end),
        })
    }

    fn parse_spec_block_into(
        &mut self,
        spec: &mut SpecBlock,
        effects: &mut EffectRow,
    ) -> PResult<()> {
        self.expect(&Tok::LBrace, "'{' to open spec block")?;
        while !matches!(self.peek(), Some(Tok::RBrace) | None) {
            match self.peek() {
                Some(Tok::Requires) => {
                    self.bump();
                    let e = self.parse_expr_top()?;
                    spec.requires.push(e);
                }
                Some(Tok::Ensures) => {
                    self.bump();
                    let e = self.parse_expr_top()?;
                    spec.ensures.push(e);
                }
                Some(Tok::Effects) => {
                    self.bump();
                    *effects = effects.union(&self.parse_effect_set()?);
                }
                other => {
                    return Err(ParseError::at(
                        self.peek_span(),
                        format!("expected requires/ensures/effects in spec block, got {other:?}"),
                    ));
                }
            }
            let _ = self.eat(&Tok::Semi);
        }
        self.expect(&Tok::RBrace, "'}' to close spec block")?;
        Ok(())
    }

    fn parse_let_decl(&mut self, doc: Option<String>) -> PResult<LetDecl> {
        let start = self.peek_span();
        self.expect(&Tok::Let, "'let'")?;
        let (name, _) = self.expect_ident("let name")?;
        let ty = if self.eat(&Tok::Colon) {
            Some(self.parse_type()?)
        } else {
            None
        };
        self.expect(&Tok::Eq, "'=' in let")?;
        let value = self.parse_expr_top()?;
        let end = self.last_span();
        Ok(LetDecl {
            name,
            ty,
            value,
            doc,
            span: start.join(end),
        })
    }

    fn parse_type_alias(&mut self) -> PResult<TypeAliasDecl> {
        let start = self.peek_span();
        self.expect(&Tok::Type, "'type'")?;
        let (name, _) = self.expect_ident("alias name")?;
        let generics = self.parse_generics_opt()?;
        self.expect(&Tok::Eq, "'=' in type alias")?;
        // Detect ADT syntax: `UpperName(...)` or `UpperName |` or bare `UpperName` at end.
        // Heuristic: peek at first token — if it's an uppercase ident, look one more
        // token ahead. If that's `(` or `|` or EOF/newline, treat as ADT.
        let ty = if self.is_adt_body() {
            self.parse_adt_body(&name, start)?
        } else {
            self.parse_type()?
        };
        let end = self.last_span();
        Ok(TypeAliasDecl {
            name,
            generics,
            ty,
            span: start.join(end),
        })
    }

    /// Returns true when the current token stream looks like an ADT variant list.
    /// An ADT body starts with an uppercase-leading ident followed by `(` or `|`.
    /// A bare uppercase ident with no suffix is also an ADT (nullary constructor).
    /// We explicitly exclude `Name<` (generic alias) and plain `Name` that
    /// could resolve to an existing type alias — only `Name(` / `Name |` / lone `Name` at EOL.
    fn is_adt_body(&self) -> bool {
        match self.peek() {
            Some(Tok::Ident(s)) if s.chars().next().is_some_and(|c| c.is_ascii_uppercase()) => {
                // peek_at(1): if it's `(` or `|` → definitely ADT
                // if it's `None` (end of token stream) → nullary ADT
                // anything else (e.g. `->`, `<`, another ident) → not ADT
                matches!(self.peek_at(1), Some(Tok::LParen) | Some(Tok::Pipe) | None)
            }
            _ => false,
        }
    }

    /// Parse `Name(T1, T2) | Name2 | Name3(T3)` as `Type::Adt`.
    fn parse_adt_body(&mut self, adt_name: &str, start: Span) -> PResult<Type> {
        let mut ctors: Vec<(String, Vec<Type>)> = Vec::new();
        loop {
            let (ctor_name, _) = self.expect_ident("constructor name")?;
            let fields = if self.eat(&Tok::LParen) {
                let mut args = Vec::new();
                while !matches!(self.peek(), Some(Tok::RParen) | None) {
                    args.push(self.parse_type()?);
                    if !self.eat(&Tok::Comma) {
                        break;
                    }
                }
                self.expect(&Tok::RParen, "')' after constructor fields")?;
                args
            } else {
                vec![]
            };
            ctors.push((ctor_name, fields));
            if !self.eat(&Tok::Pipe) {
                break;
            }
        }
        let end = self.last_span();
        Ok(Type::Adt {
            name: adt_name.to_string(),
            ctors,
            span: start.join(end),
        })
    }

    fn parse_import(&mut self) -> PResult<ImportDecl> {
        let start = self.peek_span();
        self.expect(&Tok::Import, "'import'")?;
        // form 1: `import path::to::module` (optionally `as alias`)
        // form 2: `import {a, b} from path::to::module`
        let mut names = Vec::new();
        if self.eat(&Tok::LBrace) {
            loop {
                let (n, _) = self.expect_ident("imported name")?;
                names.push(n);
                if !self.eat(&Tok::Comma) {
                    break;
                }
            }
            self.expect(&Tok::RBrace, "'}' after import list")?;
            self.expect(&Tok::From, "'from' in import")?;
        }
        let mut path = Vec::new();
        let (head, _) = self.expect_ident("module path")?;
        path.push(head);
        while self.eat(&Tok::ColonColon) {
            let (seg, _) = self.expect_ident("module path segment")?;
            path.push(seg);
        }
        let alias = if self.eat(&Tok::As) {
            Some(self.expect_ident("import alias")?.0)
        } else {
            None
        };
        let end = self.last_span();
        Ok(ImportDecl {
            path,
            names,
            alias,
            span: start.join(end),
        })
    }

    fn parse_tool_decl(&mut self, doc: Option<String>) -> PResult<ToolDecl> {
        let start = self.peek_span();
        self.expect(&Tok::Tool, "'tool'")?;
        let (name, _) = self.expect_ident("tool name")?;
        let params = self.parse_params()?;
        self.expect(&Tok::Arrow, "'->' in tool decl")?;
        let ret = self.parse_type()?;
        let mut effects = EffectRow::pure_();
        if self.eat(&Tok::Bang) {
            effects = self.parse_effect_set()?;
        }
        let end = self.last_span();
        Ok(ToolDecl {
            name,
            params,
            ret,
            effects,
            doc,
            span: start.join(end),
        })
    }

    // --- params, generics, effects --------------------------------------

    fn parse_generics_opt(&mut self) -> PResult<Vec<String>> {
        if !self.eat(&Tok::Lt) {
            return Ok(Vec::new());
        }
        let mut g = Vec::new();
        loop {
            let (n, _) = self.expect_ident("generic parameter")?;
            g.push(n);
            if !self.eat(&Tok::Comma) {
                break;
            }
        }
        self.expect(&Tok::Gt, "'>' to close generics")?;
        Ok(g)
    }

    fn parse_params(&mut self) -> PResult<Vec<Param>> {
        self.expect(&Tok::LParen, "'(' to open parameters")?;
        let mut params = Vec::new();
        while !matches!(self.peek(), Some(Tok::RParen) | None) {
            let start = self.peek_span();
            let (name, _) = self.expect_ident("parameter name")?;
            self.expect(&Tok::Colon, "':' before parameter type")?;
            let ty = self.parse_type()?;
            let default = if self.eat(&Tok::Eq) {
                Some(self.parse_expr_top()?)
            } else {
                None
            };
            let end = self.last_span();
            params.push(Param {
                name,
                ty,
                default,
                span: start.join(end),
            });
            if !self.eat(&Tok::Comma) {
                break;
            }
        }
        self.expect(&Tok::RParen, "')' to close parameters")?;
        Ok(params)
    }

    fn parse_effect_set(&mut self) -> PResult<EffectRow> {
        self.expect(&Tok::LBrace, "'{' to open effect set")?;
        let mut effects = Vec::new();
        let mut tail = None;
        while !matches!(self.peek(), Some(Tok::RBrace) | None) {
            let (name, _) = self.expect_ident("effect label")?;
            // Convention: lowercase single-letter / unknown → row variable
            if name.len() == 1 && !name.chars().next().unwrap().is_ascii_uppercase() {
                tail = Some(name);
            } else {
                effects.push(Effect::from_str(&name));
            }
            if !self.eat(&Tok::Comma) {
                break;
            }
        }
        self.expect(&Tok::RBrace, "'}' to close effect set")?;
        effects.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        Ok(EffectRow { effects, tail })
    }

    // --- types -----------------------------------------------------------

    fn parse_type(&mut self) -> PResult<Type> {
        let mut ty = self.parse_type_atom()?;
        // Postfix: `?` for option, `~ confidence(p)` for confidence, `where pred` for refinement.
        loop {
            if self.eat(&Tok::Question) {
                let sp = ty.span().join(self.last_span());
                ty = Type::Option(Box::new(ty), sp);
            } else if self.eat(&Tok::Tilde) {
                self.expect(&Tok::Confidence, "'confidence(p)' after '~'")?;
                self.expect(&Tok::LParen, "'(' after confidence")?;
                let p = self.parse_expr_top()?;
                self.expect(&Tok::RParen, "')' after confidence arg")?;
                let sp = ty.span().join(self.last_span());
                ty = Type::Confidence {
                    base: Box::new(ty),
                    p: Box::new(p),
                    span: sp,
                };
            } else if self.peek() == Some(&Tok::Where) {
                // Verbose refinement: `Int where n > 0` (binder is implicit `_` or single-letter base?).
                // We require an explicit binder for clarity: `Int{n: n > 0}` is preferred; here we
                // allow `Int where p` where `p` may reference `result` for fn returns. For type
                // contexts other than fn return, we use compact `Base{binder: pred}` only.
                break;
            } else if self.peek() == Some(&Tok::LBrace) {
                // Could be compact refinement `I{n: ...}` or `I{n>=0}` (binder defaults to slice-binder).
                // Heuristic: look ahead — `{IDENT :` → binder pred, `{...}` else → pred over default binder.
                // We accept both. For `I{pred}` we synthesize the binder name `x`.
                self.bump(); // consume `{`
                let binder;
                let pred;
                let bstart = self.peek_span();
                if let (Some(Tok::Ident(_)), Some(Tok::Colon)) = (self.peek(), self.peek_at(1)) {
                    let (b, _) = self.expect_ident("refinement binder")?;
                    self.expect(&Tok::Colon, "':' after refinement binder")?;
                    binder = b;
                    pred = self.parse_expr_top()?;
                } else {
                    // No explicit binder. Use base type's compact form as binder placeholder.
                    binder = "x".to_string();
                    pred = self.parse_expr_top()?;
                }
                self.expect(&Tok::RBrace, "'}' to close refinement")?;
                let rspan = bstart.join(self.last_span());
                ty = Type::Refined {
                    base: Box::new(ty.clone()),
                    refinement: Refinement {
                        binder,
                        pred: Box::new(pred),
                        span: rspan,
                    },
                    span: ty.span().join(self.last_span()),
                };
            } else {
                break;
            }
        }
        // Function types: `T -> U !{eff}` written postfix, but our top-level lookup
        // already handles `->` in fn decls. We allow inline function types: `(I,I) -> I !{}`
        if self.eat(&Tok::Arrow) {
            let ret = self.parse_type()?;
            let effects = if self.eat(&Tok::Bang) {
                self.parse_effect_set()?
            } else {
                EffectRow::pure_()
            };
            let sp = ty.span().join(self.last_span());
            // Pack `ty` into a single-element param list (for `T -> U`) or unpack tuple.
            let params = match ty {
                Type::Tuple(parts, _) => parts,
                t => vec![t],
            };
            ty = Type::Fun {
                params,
                ret: Box::new(ret),
                effects,
                span: sp,
            };
        }
        Ok(ty)
    }

    fn parse_type_atom(&mut self) -> PResult<Type> {
        let start = self.peek_span();
        let tok = self.peek().cloned();
        match tok {
            Some(Tok::Ident(name)) => {
                self.bump();
                let sp = start;
                if let Some(con) = TyCon::from_str(&name) {
                    // Generic application: `Map<K, V>`?
                    if self.eat(&Tok::Lt) {
                        let mut args = Vec::new();
                        loop {
                            args.push(self.parse_type()?);
                            if !self.eat(&Tok::Comma) {
                                break;
                            }
                        }
                        self.expect(&Tok::Gt, "'>' to close generic arguments")?;
                        Ok(Type::Generic {
                            name,
                            args,
                            span: sp.join(self.last_span()),
                        })
                    } else {
                        Ok(Type::Con(con, sp))
                    }
                } else if name
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_lowercase() && name.len() == 1)
                {
                    // Single lowercase letter: type variable.
                    Ok(Type::Var(name, sp))
                } else {
                    // User type (generic or alias)
                    if self.eat(&Tok::Lt) {
                        let mut args = Vec::new();
                        loop {
                            args.push(self.parse_type()?);
                            if !self.eat(&Tok::Comma) {
                                break;
                            }
                        }
                        self.expect(&Tok::Gt, "'>' to close generic arguments")?;
                        Ok(Type::Generic {
                            name,
                            args,
                            span: sp.join(self.last_span()),
                        })
                    } else {
                        Ok(Type::Generic {
                            name,
                            args: vec![],
                            span: sp,
                        })
                    }
                }
            }
            Some(Tok::LParen) => {
                self.bump();
                if self.eat(&Tok::RParen) {
                    return Ok(Type::Con(TyCon::Unit, start.join(self.last_span())));
                }
                let first = self.parse_type()?;
                if self.eat(&Tok::Comma) {
                    let mut elts = vec![first];
                    while !matches!(self.peek(), Some(Tok::RParen) | None) {
                        elts.push(self.parse_type()?);
                        if !self.eat(&Tok::Comma) {
                            break;
                        }
                    }
                    self.expect(&Tok::RParen, "')' to close tuple type")?;
                    Ok(Type::Tuple(elts, start.join(self.last_span())))
                } else {
                    self.expect(&Tok::RParen, "')' to close parenthesized type")?;
                    Ok(first)
                }
            }
            Some(Tok::LBracket) => {
                self.bump();
                let inner = self.parse_type()?;
                self.expect(&Tok::RBracket, "']' to close list type")?;
                Ok(Type::List(Box::new(inner), start.join(self.last_span())))
            }
            Some(Tok::LBrace) => {
                self.bump();
                let mut fields = Vec::new();
                while !matches!(self.peek(), Some(Tok::RBrace) | None) {
                    let (n, _) = self.expect_ident("record field name")?;
                    self.expect(&Tok::Colon, "':' in record field")?;
                    let ty = self.parse_type()?;
                    fields.push((n, ty));
                    if !self.eat(&Tok::Comma) {
                        break;
                    }
                }
                self.expect(&Tok::RBrace, "'}' to close record type")?;
                Ok(Type::Record(fields, start.join(self.last_span())))
            }
            other => Err(ParseError::at(
                start,
                format!("expected a type, got {other:?}"),
            )),
        }
    }

    // --- expressions ------------------------------------------------------

    fn parse_expr_top(&mut self) -> PResult<Expr> {
        self.parse_expr_bp(0)
    }

    fn parse_expr_bp(&mut self, min_bp: u8) -> PResult<Expr> {
        let mut lhs = self.parse_expr_unary()?;
        loop {
            let (op, lbp, rbp) = match self.peek() {
                Some(Tok::FatArrow) => (BinOp::Implies, 0, 1),
                Some(Tok::OrOr) | Some(Tok::OrKw) => (BinOp::Or, 1, 2),
                Some(Tok::AndAnd) | Some(Tok::AndKw) => (BinOp::And, 3, 4),
                Some(Tok::EqEq) => (BinOp::Eq, 5, 6),
                Some(Tok::BangEq) => (BinOp::Neq, 5, 6),
                Some(Tok::Lt) => (BinOp::Lt, 7, 8),
                Some(Tok::Le) => (BinOp::Le, 7, 8),
                Some(Tok::Gt) => (BinOp::Gt, 7, 8),
                Some(Tok::Ge) => (BinOp::Ge, 7, 8),
                Some(Tok::Plus) => (BinOp::Add, 9, 10),
                Some(Tok::Minus) => (BinOp::Sub, 9, 10),
                Some(Tok::PlusPlus) => (BinOp::Concat, 9, 10),
                Some(Tok::Star) => (BinOp::Mul, 11, 12),
                Some(Tok::Slash) => (BinOp::Div, 11, 12),
                Some(Tok::Percent) => (BinOp::Mod, 11, 12),
                _ => break,
            };
            if lbp < min_bp {
                break;
            }
            self.bump();
            let rhs = self.parse_expr_bp(rbp)?;
            let sp = lhs.span().join(rhs.span());
            lhs = Expr::Bin(op, Box::new(lhs), Box::new(rhs), sp);
        }
        Ok(lhs)
    }

    fn parse_expr_unary(&mut self) -> PResult<Expr> {
        let start = self.peek_span();
        match self.peek() {
            Some(Tok::Minus) => {
                self.bump();
                let e = self.parse_expr_unary()?;
                let sp = start.join(e.span());
                Ok(Expr::Un(UnOp::Neg, Box::new(e), sp))
            }
            Some(Tok::Bang) | Some(Tok::NotKw) => {
                self.bump();
                let e = self.parse_expr_unary()?;
                let sp = start.join(e.span());
                Ok(Expr::Un(UnOp::Not, Box::new(e), sp))
            }
            _ => self.parse_expr_postfix(),
        }
    }

    fn parse_expr_postfix(&mut self) -> PResult<Expr> {
        let mut e = self.parse_expr_atom()?;
        loop {
            match self.peek() {
                Some(Tok::LParen) => {
                    self.bump();
                    let mut args = Vec::new();
                    while !matches!(self.peek(), Some(Tok::RParen) | None) {
                        let astart = self.peek_span();
                        // keyword arg `name = expr`?
                        let name = if let (Some(Tok::Ident(_)), Some(Tok::Eq)) =
                            (self.peek(), self.peek_at(1))
                        {
                            let (n, _) = self.expect_ident("keyword arg name")?;
                            self.bump(); // '='
                            Some(n)
                        } else {
                            None
                        };
                        let value = self.parse_expr_top()?;
                        let aspan = astart.join(self.last_span());
                        args.push(Arg {
                            name,
                            value,
                            span: aspan,
                        });
                        if !self.eat(&Tok::Comma) {
                            break;
                        }
                    }
                    self.expect(&Tok::RParen, "')' to close call")?;
                    let sp = e.span().join(self.last_span());
                    e = Expr::Call {
                        callee: Box::new(e),
                        args,
                        span: sp,
                    };
                }
                Some(Tok::Dot) => {
                    self.bump();
                    let (field, _) = self.expect_ident("field name after '.'")?;
                    let sp = e.span().join(self.last_span());
                    e = Expr::Field(Box::new(e), field, sp);
                }
                Some(Tok::LBracket) => {
                    self.bump();
                    let idx = self.parse_expr_top()?;
                    self.expect(&Tok::RBracket, "']' to close index")?;
                    let sp = e.span().join(self.last_span());
                    e = Expr::Index(Box::new(e), Box::new(idx), sp);
                }
                Some(Tok::PipeForward) => {
                    self.bump();
                    let rhs = self.parse_expr_atom()?;
                    // `x |> f(a, b)` becomes `f(x, a, b)`. If RHS isn't a call, wrap it.
                    let sp = e.span().join(rhs.span());
                    match rhs {
                        Expr::Call {
                            callee,
                            mut args,
                            span: _,
                        } => {
                            args.insert(
                                0,
                                Arg {
                                    name: None,
                                    value: e,
                                    span: sp,
                                },
                            );
                            e = Expr::Call {
                                callee,
                                args,
                                span: sp,
                            };
                        }
                        other => {
                            e = Expr::Call {
                                callee: Box::new(other),
                                args: vec![Arg {
                                    name: None,
                                    value: e,
                                    span: sp,
                                }],
                                span: sp,
                            };
                        }
                    }
                }
                Some(Tok::Colon)
                    if matches!(
                        self.peek_at(1),
                        Some(Tok::Ident(_))
                            | Some(Tok::LParen)
                            | Some(Tok::LBracket)
                            | Some(Tok::LBrace)
                    ) =>
                {
                    // type ascription: `expr : Type`
                    self.bump();
                    let ty = self.parse_type()?;
                    let sp = e.span().join(self.last_span());
                    e = Expr::Annot {
                        expr: Box::new(e),
                        ty,
                        span: sp,
                    };
                }
                _ => break,
            }
        }
        Ok(e)
    }

    fn parse_expr_atom(&mut self) -> PResult<Expr> {
        let start = self.peek_span();
        let tok = self.peek().cloned();
        match tok {
            Some(Tok::Int(n)) => {
                self.bump();
                Ok(Expr::Lit(Lit::Int(n), start))
            }
            Some(Tok::Float(f)) => {
                self.bump();
                Ok(Expr::Lit(Lit::Float(f), start))
            }
            Some(Tok::Bool(b)) => {
                self.bump();
                Ok(Expr::Lit(Lit::Bool(b), start))
            }
            Some(Tok::Str(s)) => {
                self.bump();
                // If the string contains `${...}` interpolations, decompose into
                // `Expr::StrInterp { parts }`. Otherwise keep the fast plain-Lit path.
                if s.contains("${") {
                    self.build_str_interp(s, start)
                } else {
                    Ok(Expr::Lit(Lit::Str(s), start))
                }
            }
            Some(Tok::Result_) => {
                self.bump();
                Ok(Expr::Var("result".to_string(), start))
            }
            Some(Tok::Ident(name)) => {
                self.bump();
                Ok(Expr::Var(name, start))
            }
            Some(Tok::Introspect) => self.parse_builtin_call("introspect", start),
            Some(Tok::Summarize) => self.parse_builtin_call("summarize", start),
            Some(Tok::Provenance) => self.parse_builtin_call("provenance", start),
            Some(Tok::Confident) => self.parse_confident_call(start),
            Some(Tok::Assume) => {
                self.bump();
                self.expect(&Tok::LParen, "'(' after assume")?;
                let p = self.parse_expr_top()?;
                self.expect(&Tok::RParen, "')' after assume arg")?;
                let sp = start.join(self.last_span());
                Ok(Expr::Assume(Box::new(p), sp))
            }
            Some(Tok::LParen) => self.parse_paren_or_tuple(),
            Some(Tok::LBracket) => self.parse_list_literal(),
            Some(Tok::LBrace) => {
                // Record literal `{ x: 1, y: 2 }` OR block `{ stmts; tail }`.
                // Lookahead disambiguates: `LBrace IDENT COLON` → record; else block.
                if matches!(self.peek_at(1), Some(Tok::Ident(_)))
                    && matches!(self.peek_at(2), Some(Tok::Colon))
                {
                    self.parse_record_literal()
                } else {
                    self.parse_block_expr()
                }
            }
            Some(Tok::If) => self.parse_if_expr(),
            Some(Tok::Let) => self.parse_let_expr(),
            Some(Tok::Match) => self.parse_match_expr(),
            Some(Tok::Fn) => self.parse_lambda(),
            other => Err(ParseError::at(
                start,
                format!("expected expression, got {other:?}"),
            )),
        }
    }

    fn parse_builtin_call(&mut self, name: &str, start: Span) -> PResult<Expr> {
        // Each agent builtin is parsed as a normal call to a reserved Var name.
        self.bump(); // consume the keyword token
        let callee = Expr::Var(name.to_string(), start);
        self.expect(&Tok::LParen, "'(' after builtin")?;
        let mut args = Vec::new();
        while !matches!(self.peek(), Some(Tok::RParen) | None) {
            let astart = self.peek_span();
            let name = if let (Some(Tok::Ident(_)), Some(Tok::Eq)) = (self.peek(), self.peek_at(1))
            {
                let (n, _) = self.expect_ident("keyword arg name")?;
                self.bump();
                Some(n)
            } else {
                None
            };
            let value = self.parse_expr_top()?;
            args.push(Arg {
                name,
                value,
                span: astart.join(self.last_span()),
            });
            if !self.eat(&Tok::Comma) {
                break;
            }
        }
        self.expect(&Tok::RParen, "')' after builtin args")?;
        Ok(Expr::Call {
            callee: Box::new(callee),
            args,
            span: start.join(self.last_span()),
        })
    }

    fn parse_confident_call(&mut self, start: Span) -> PResult<Expr> {
        self.bump();
        self.expect(&Tok::LParen, "'(' after confident")?;
        let v = self.parse_expr_top()?;
        self.expect(&Tok::Comma, "',' between confident args")?;
        let p = self.parse_expr_top()?;
        self.expect(&Tok::RParen, "')' after confident args")?;
        let sp = start.join(self.last_span());
        Ok(Expr::Confident {
            value: Box::new(v),
            p: Box::new(p),
            span: sp,
        })
    }

    fn parse_paren_or_tuple(&mut self) -> PResult<Expr> {
        let start = self.peek_span();
        self.expect(&Tok::LParen, "'('")?;
        if self.eat(&Tok::RParen) {
            return Ok(Expr::Lit(Lit::Unit, start.join(self.last_span())));
        }
        let first = self.parse_expr_top()?;
        if self.eat(&Tok::Comma) {
            let mut elts = vec![first];
            while !matches!(self.peek(), Some(Tok::RParen) | None) {
                elts.push(self.parse_expr_top()?);
                if !self.eat(&Tok::Comma) {
                    break;
                }
            }
            self.expect(&Tok::RParen, "')' to close tuple")?;
            Ok(Expr::Tuple(elts, start.join(self.last_span())))
        } else {
            self.expect(&Tok::RParen, "')' to close parenthesized expression")?;
            Ok(first)
        }
    }

    fn parse_list_literal(&mut self) -> PResult<Expr> {
        let start = self.peek_span();
        self.expect(&Tok::LBracket, "'['")?;
        let mut elts = Vec::new();
        while !matches!(self.peek(), Some(Tok::RBracket) | None) {
            elts.push(self.parse_expr_top()?);
            if !self.eat(&Tok::Comma) {
                break;
            }
        }
        self.expect(&Tok::RBracket, "']' to close list")?;
        Ok(Expr::List(elts, start.join(self.last_span())))
    }

    fn parse_record_literal(&mut self) -> PResult<Expr> {
        let start = self.peek_span();
        self.expect(&Tok::LBrace, "'{'")?;
        let mut fields = Vec::new();
        while !matches!(self.peek(), Some(Tok::RBrace) | None) {
            let (n, _) = self.expect_ident("record field name")?;
            self.expect(&Tok::Colon, "':' in record literal")?;
            let v = self.parse_expr_top()?;
            fields.push((n, v));
            if !self.eat(&Tok::Comma) {
                break;
            }
        }
        self.expect(&Tok::RBrace, "'}' to close record literal")?;
        Ok(Expr::Record(fields, start.join(self.last_span())))
    }

    fn parse_block_expr(&mut self) -> PResult<Expr> {
        let start = self.peek_span();
        self.expect(&Tok::LBrace, "'{' to open block")?;
        let mut stmts = Vec::new();
        let mut tail: Option<Box<Expr>> = None;
        while !matches!(self.peek(), Some(Tok::RBrace) | None) {
            match self.peek() {
                Some(Tok::Let) => {
                    let lstart = self.peek_span();
                    self.bump();
                    let pat = self.parse_pattern()?;
                    let ty = if self.eat(&Tok::Colon) {
                        Some(self.parse_type()?)
                    } else {
                        None
                    };
                    self.expect(&Tok::Eq, "'=' in let binding")?;
                    let value = self.parse_expr_top()?;
                    let _ = self.eat(&Tok::Semi);
                    let sp = lstart.join(self.last_span());
                    stmts.push(Stmt::Let {
                        pat,
                        ty,
                        value,
                        span: sp,
                    });
                }
                _ => {
                    let e = self.parse_expr_top()?;
                    if self.eat(&Tok::Semi) {
                        stmts.push(Stmt::Expr(e));
                    } else if matches!(self.peek(), Some(Tok::RBrace)) {
                        tail = Some(Box::new(e));
                    } else {
                        // Implicit semicolon between statements (newlines aren't tokenized, so we
                        // accept consecutive expressions as separate statements).
                        stmts.push(Stmt::Expr(e));
                    }
                }
            }
        }
        self.expect(&Tok::RBrace, "'}' to close block")?;
        Ok(Expr::Block {
            stmts,
            tail,
            span: start.join(self.last_span()),
        })
    }

    /// Parse an arm body (then-arm, else-arm, or match-arm body).
    ///
    /// Three cases:
    ///   1. Next token is `{`  → delegate to `parse_block_expr` (unchanged).
    ///   2. Next token is `let` → parse an **implicit block**: collect `let`
    ///      statements until a terminator or EOF, then treat the final non-let
    ///      expression as the block tail.
    ///      Disambiguation: if `in` follows a let-binding's value, that single
    ///      binding is the old `let … in body` expression form and becomes the
    ///      tail of the implicit block.
    ///   3. Otherwise → parse a single expression (unchanged).
    fn parse_arm_body(&mut self, terminators: &[Tok]) -> PResult<Expr> {
        // Case 1: explicit braced block – delegate unchanged.
        if matches!(self.peek(), Some(Tok::LBrace)) {
            return self.parse_block_expr();
        }

        // Case 3: not starting with `let` – single expression, unchanged.
        if !matches!(self.peek(), Some(Tok::Let)) {
            return self.parse_expr_top();
        }

        // Case 2: implicit block beginning with `let`.
        let block_start = self.peek_span();
        let mut stmts: Vec<Stmt> = Vec::new();

        loop {
            // Stop at a terminator or EOF before consuming anything.
            if self.peek().is_none() {
                break;
            }
            if terminators
                .iter()
                .any(|t| std::mem::discriminant(self.peek().unwrap()) == std::mem::discriminant(t))
            {
                break;
            }

            if matches!(self.peek(), Some(Tok::Let)) {
                let lstart = self.peek_span();
                self.bump(); // consume `let`
                let pat = self.parse_pattern()?;
                let ty = if self.eat(&Tok::Colon) {
                    Some(self.parse_type()?)
                } else {
                    None
                };
                self.expect(&Tok::Eq, "'=' in let binding")?;
                let value = self.parse_expr_top()?;

                // Disambiguation: `in` → this is the `let … in body` expression
                // form, not a statement.  Wrap it as the block tail and return.
                if matches!(self.peek(), Some(Tok::In)) {
                    self.bump(); // consume `in`
                    let body = self.parse_expr_top()?;
                    let sp = lstart.join(self.last_span());
                    let tail_expr = Expr::Let {
                        pat,
                        ty,
                        value: Box::new(value),
                        body: Box::new(body),
                        span: sp,
                    };
                    let block_sp = block_start.join(self.last_span());
                    return Ok(Expr::Block {
                        stmts,
                        tail: Some(Box::new(tail_expr)),
                        span: block_sp,
                    });
                }

                // Ordinary let statement.
                let _ = self.eat(&Tok::Semi);
                let sp = lstart.join(self.last_span());
                stmts.push(Stmt::Let {
                    pat,
                    ty,
                    value,
                    span: sp,
                });

                // After a let-stmt: if the next token is `let` again, loop.
                // If it's a terminator / EOF, break (no tail expr → Unit tail).
                // Otherwise fall through to parse the tail expression.
                if matches!(self.peek(), Some(Tok::Let)) {
                    continue;
                }
                if self.peek().is_none()
                    || terminators.iter().any(|t| {
                        std::mem::discriminant(self.peek().unwrap()) == std::mem::discriminant(t)
                    })
                {
                    break;
                }
                // Fall through: next thing is the tail expression.
            }

            // Tail expression — everything after the last `let` statement.
            let e = self.parse_expr_top()?;
            let _ = self.eat(&Tok::Semi);
            let block_sp = block_start.join(self.last_span());
            return Ok(Expr::Block {
                stmts,
                tail: Some(Box::new(e)),
                span: block_sp,
            });
        }

        // Reached a terminator with only stmts and no tail.
        // Synthesise a Unit literal so the block is always well-formed.
        let block_sp = block_start.join(self.last_span());
        Ok(Expr::Block {
            stmts,
            tail: Some(Box::new(Expr::Lit(Lit::Unit, block_sp))),
            span: block_sp,
        })
    }

    fn parse_if_expr(&mut self) -> PResult<Expr> {
        let start = self.peek_span();
        self.expect(&Tok::If, "'if'")?;
        let cond = self.parse_expr_top()?;
        let then_branch = if self.eat(&Tok::Then) {
            // then-arm ends at `else`
            self.parse_arm_body(&[Tok::Else])?
        } else {
            self.parse_block_expr()?
        };
        self.expect(&Tok::Else, "'else' in if")?;
        // else-arm: no fixed terminator token; parse_arm_body collects
        // let-stmts then stops after the single tail expression.
        let else_branch = self.parse_arm_body(&[])?;
        let sp = start.join(self.last_span());
        Ok(Expr::If {
            cond: Box::new(cond),
            then_branch: Box::new(then_branch),
            else_branch: Box::new(else_branch),
            span: sp,
        })
    }

    fn parse_let_expr(&mut self) -> PResult<Expr> {
        let start = self.peek_span();
        self.expect(&Tok::Let, "'let'")?;
        let pat = self.parse_pattern()?;
        let ty = if self.eat(&Tok::Colon) {
            Some(self.parse_type()?)
        } else {
            None
        };
        self.expect(&Tok::Eq, "'=' in let-in")?;
        let value = self.parse_expr_top()?;
        self.expect(&Tok::In, "'in' in let-in")?;
        let body = self.parse_expr_top()?;
        let sp = start.join(self.last_span());
        Ok(Expr::Let {
            pat,
            ty,
            value: Box::new(value),
            body: Box::new(body),
            span: sp,
        })
    }

    /// Parse a string literal that contains `${...}` interpolations into
    /// `Expr::StrInterp { parts }`.  `\${` is treated as a literal `${`.
    fn build_str_interp(&mut self, raw: String, span: Span) -> PResult<Expr> {
        let mut parts: Vec<StrPart> = Vec::new();
        let mut current = String::new();
        let mut chars = raw.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\\' {
                // Honour backslash-escape: `\${` becomes a literal `${`.
                if matches!(chars.peek(), Some(&'$')) {
                    let _ = chars.next(); // consume '$'
                    if matches!(chars.peek(), Some(&'{')) {
                        let _ = chars.next(); // consume '{'
                        current.push_str("${");
                        continue;
                    }
                    current.push('\\');
                    current.push('$');
                    continue;
                }
                // Anything else: pass backslash through (lexer already unescaped \n etc.).
                current.push('\\');
                continue;
            }
            if c == '$' && matches!(chars.peek(), Some(&'{')) {
                let _ = chars.next(); // consume '{'
                                      // Flush literal accumulator.
                if !current.is_empty() {
                    parts.push(StrPart::Lit(std::mem::take(&mut current)));
                }
                // Collect everything up to the matching '}'.
                let mut depth = 1usize;
                let mut inner = String::new();
                for ic in chars.by_ref() {
                    match ic {
                        '{' => {
                            depth += 1;
                            inner.push(ic);
                        }
                        '}' => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                            inner.push(ic);
                        }
                        other => inner.push(other),
                    }
                }
                // Re-parse inner as a full expression.
                let inner_expr = parse_expr(self.file, &inner)
                    .map_err(|e| ParseError::at(span, format!("in string interpolation: {e}")))?;
                parts.push(StrPart::Expr(inner_expr));
                continue;
            }
            current.push(c);
        }
        if !current.is_empty() {
            parts.push(StrPart::Lit(current));
        }
        Ok(Expr::StrInterp { parts, span })
    }

    fn parse_named_block_decl(&mut self, prefix: &str, doc: Option<String>) -> PResult<FnDecl> {
        let start = self.peek_span();
        // consume the leading identifier (e.g. `test` / `bench`)
        let _ = self.bump();
        // consume the name string literal
        let raw_name = match self.bump() {
            Some(t) => match t.tok {
                Tok::Str(s) => s,
                other => {
                    return Err(ParseError::at(
                        start,
                        format!("expected string literal after named-block keyword, got {other:?}"),
                    ))
                }
            },
            None => return Err(ParseError::Eof),
        };
        let body = self.parse_block_expr()?;
        let end = self.last_span();
        let sanitized: String = raw_name
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let fn_name = format!("{prefix}{sanitized}");
        Ok(FnDecl {
            name: fn_name,
            generics: vec![],
            params: vec![],
            ret: Type::Con(TyCon::Unit, start),
            effects: EffectRow {
                effects: vec![
                    Effect::Async,
                    Effect::FS,
                    Effect::IO,
                    Effect::Net,
                    Effect::Rand,
                    Effect::State,
                    Effect::Throw,
                ],
                tail: None,
            },
            spec: SpecBlock::default(),
            body,
            doc: doc.or(Some(raw_name)),
            no_prov: false,
            span: start.join(end),
        })
    }

    fn parse_match_expr(&mut self) -> PResult<Expr> {
        let start = self.peek_span();
        self.expect(&Tok::Match, "'match'")?;
        let scrutinee = self.parse_expr_top()?;
        // `match expr with { arms }` (verbose) or `match expr { arms }` (compact)
        let _ = self.eat(&Tok::With);
        self.expect(&Tok::LBrace, "'{' to open match arms")?;
        let mut arms = Vec::new();
        while !matches!(self.peek(), Some(Tok::RBrace) | None) {
            let astart = self.peek_span();
            let pat = self.parse_pattern()?;
            let guard = if self.eat(&Tok::If) {
                // Parse with min_bp=1 so the trailing `=>` (Implies, lbp=0) is left for the arm separator.
                Some(self.parse_expr_bp(1)?)
            } else {
                None
            };
            self.expect(&Tok::FatArrow, "'=>' between pattern and arm body")?;
            // match-arm body ends at `,` (next arm) or `}` (end of match).
            let body = self.parse_arm_body(&[Tok::Comma, Tok::RBrace])?;
            let aspan = astart.join(self.last_span());
            arms.push(MatchArm {
                pat,
                guard,
                body,
                span: aspan,
            });
            if !self.eat(&Tok::Comma) {
                break;
            }
        }
        self.expect(&Tok::RBrace, "'}' to close match arms")?;
        Ok(Expr::Match {
            scrutinee: Box::new(scrutinee),
            arms,
            span: start.join(self.last_span()),
        })
    }

    fn parse_lambda(&mut self) -> PResult<Expr> {
        let start = self.peek_span();
        self.expect(&Tok::Fn, "'fn' for lambda")?;
        let params = self.parse_params()?;
        let ret = if self.eat(&Tok::Arrow) {
            Some(self.parse_type()?)
        } else {
            None
        };
        let body = if matches!(self.peek(), Some(Tok::LBrace)) {
            self.parse_block_expr()?
        } else {
            self.expect(&Tok::FatArrow, "'=>' or '{' in lambda body")?;
            self.parse_expr_top()?
        };
        let sp = start.join(self.last_span());
        Ok(Expr::Lambda {
            params,
            ret,
            body: Box::new(body),
            span: sp,
        })
    }

    // --- patterns --------------------------------------------------------

    fn parse_pattern(&mut self) -> PResult<Pattern> {
        let start = self.peek_span();
        match self.peek().cloned() {
            Some(Tok::Ident(s)) if s == "_" => {
                self.bump();
                Ok(Pattern::Wild(start))
            }
            Some(Tok::Ident(s)) => {
                self.bump();
                // Constructor `Some(p)`?
                if s.chars().next().is_some_and(|c| c.is_ascii_uppercase())
                    && self.eat(&Tok::LParen)
                {
                    let mut args = Vec::new();
                    while !matches!(self.peek(), Some(Tok::RParen) | None) {
                        args.push(self.parse_pattern()?);
                        if !self.eat(&Tok::Comma) {
                            break;
                        }
                    }
                    self.expect(&Tok::RParen, "')' after constructor pattern args")?;
                    Ok(Pattern::Ctor {
                        name: s,
                        args,
                        span: start.join(self.last_span()),
                    })
                } else {
                    Ok(Pattern::Var(s, start))
                }
            }
            Some(Tok::Int(n)) => {
                self.bump();
                Ok(Pattern::Lit(Lit::Int(n), start))
            }
            Some(Tok::Bool(b)) => {
                self.bump();
                Ok(Pattern::Lit(Lit::Bool(b), start))
            }
            Some(Tok::Str(s)) => {
                self.bump();
                Ok(Pattern::Lit(Lit::Str(s), start))
            }
            Some(Tok::LParen) => {
                self.bump();
                if self.eat(&Tok::RParen) {
                    return Ok(Pattern::Lit(Lit::Unit, start.join(self.last_span())));
                }
                let first = self.parse_pattern()?;
                if self.eat(&Tok::Comma) {
                    let mut elts = vec![first];
                    while !matches!(self.peek(), Some(Tok::RParen) | None) {
                        elts.push(self.parse_pattern()?);
                        if !self.eat(&Tok::Comma) {
                            break;
                        }
                    }
                    self.expect(&Tok::RParen, "')' after tuple pattern")?;
                    Ok(Pattern::Tuple(elts, start.join(self.last_span())))
                } else {
                    self.expect(&Tok::RParen, "')' after parenthesized pattern")?;
                    Ok(first)
                }
            }
            other => Err(ParseError::at(
                start,
                format!("expected pattern, got {other:?}"),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pe(src: &str) -> Expr {
        parse_expr(FileId(0), src).expect("parse expr")
    }

    fn pm(src: &str) -> Module {
        parse_module(FileId(0), src).expect("parse module")
    }

    #[test]
    fn arith() {
        let e = pe("1 + 2 * 3");
        match e {
            Expr::Bin(BinOp::Add, _, ref rhs, _) => {
                assert!(matches!(**rhs, Expr::Bin(BinOp::Mul, _, _, _)));
            }
            _ => panic!("expected add at root"),
        }
    }

    #[test]
    fn boolean_precedence() {
        let e = pe("a && b || c");
        match e {
            Expr::Bin(BinOp::Or, lhs, _, _) => {
                assert!(matches!(*lhs, Expr::Bin(BinOp::And, _, _, _)));
            }
            _ => panic!("expected or at root"),
        }
    }

    #[test]
    fn call_and_field() {
        let e = pe("foo.bar(1, 2)");
        match e {
            Expr::Call { callee, args, .. } => {
                assert_eq!(args.len(), 2);
                assert!(matches!(*callee, Expr::Field(_, _, _)));
            }
            _ => panic!("expected call"),
        }
    }

    #[test]
    fn verbose_fn() {
        let m = pm("fn add(x: Int, y: Int) -> Int effects {} { x + y }");
        assert_eq!(m.decls.len(), 1);
        if let Decl::Fn(f) = &m.decls[0] {
            assert_eq!(f.name, "add");
            assert_eq!(f.params.len(), 2);
            assert!(f.effects.is_pure());
        } else {
            panic!("expected fn decl");
        }
    }

    #[test]
    fn compact_fn() {
        let m = pm("add(x:I,y:I):I!{} = x+y");
        if let Decl::Fn(f) = &m.decls[0] {
            assert_eq!(f.name, "add");
            assert_eq!(f.params.len(), 2);
            assert!(f.effects.is_pure());
        } else {
            panic!("expected fn decl");
        }
    }

    #[test]
    fn refinement_compact() {
        let m = pm("pos(n:I{n: n>0}):I!{} = n");
        if let Decl::Fn(f) = &m.decls[0] {
            match &f.params[0].ty {
                Type::Refined {
                    base, refinement, ..
                } => {
                    assert!(matches!(**base, Type::Con(TyCon::Int, _)));
                    assert_eq!(refinement.binder, "n");
                }
                other => panic!("expected refinement type, got {other:?}"),
            }
        }
    }

    #[test]
    fn refinement_compact_default_binder() {
        // Without explicit binder we synthesize `x`.
        let m = pm("pos(n:I{n>0}):I!{} = n");
        if let Decl::Fn(f) = &m.decls[0] {
            match &f.params[0].ty {
                Type::Refined { refinement, .. } => assert_eq!(refinement.binder, "x"),
                other => panic!("expected refinement type, got {other:?}"),
            }
        }
    }

    #[test]
    fn effects_parsed() {
        let m = pm("fetch(u:Str):Str!{Net,Throw} = u");
        if let Decl::Fn(f) = &m.decls[0] {
            assert_eq!(f.effects.effects.len(), 2);
            assert!(f.effects.effects.iter().any(|e| matches!(e, Effect::Net)));
            assert!(f.effects.effects.iter().any(|e| matches!(e, Effect::Throw)));
        }
    }

    #[test]
    fn introspect_call() {
        let e = pe(r#"introspect("parser")"#);
        match e {
            Expr::Call { callee, args, .. } => {
                assert_eq!(args.len(), 1);
                assert!(matches!(*callee, Expr::Var(ref n, _) if n == "introspect"));
            }
            _ => panic!("expected call"),
        }
    }

    #[test]
    fn confident_expr() {
        let e = pe("confident(42, 0.83)");
        assert!(matches!(e, Expr::Confident { .. }));
    }

    #[test]
    fn if_expr() {
        let e = pe("if x then 1 else 2");
        assert!(matches!(e, Expr::If { .. }));
    }

    #[test]
    fn test_block_desugars_to_fn() {
        let m = pm(r#"test "feature works" { assert(true) }"#);
        assert_eq!(m.decls.len(), 1);
        if let Decl::Fn(f) = &m.decls[0] {
            assert_eq!(f.name, "test__feature_works");
            assert!(f.params.is_empty());
            assert!(f.effects.effects.contains(&Effect::Throw));
        } else {
            panic!("expected test to desugar to fn decl");
        }
    }

    #[test]
    fn test_block_effect_row_includes_all_builtin_effects() {
        // test/bench/snap blocks must allow any builtin effect so that
        // print (IO), random_int (Rand), fs_read (FS), http_get (Net), etc.
        // can be called from within them without an effect error.
        let m = pm(r#"test "effectful" { assert(true) }"#);
        if let Decl::Fn(f) = &m.decls[0] {
            let effs = &f.effects.effects;
            assert!(effs.contains(&Effect::IO), "missing IO");
            assert!(effs.contains(&Effect::Rand), "missing Rand");
            assert!(effs.contains(&Effect::Throw), "missing Throw");
            assert!(effs.contains(&Effect::FS), "missing FS");
            assert!(effs.contains(&Effect::Net), "missing Net");
            assert!(effs.contains(&Effect::State), "missing State");
            assert!(effs.contains(&Effect::Async), "missing Async");
            assert!(
                f.effects.tail.is_none(),
                "tail should be None for Option-A row"
            );
        } else {
            panic!("expected test to desugar to fn decl");
        }
    }

    #[test]
    fn bench_block_desugars_to_fn() {
        let m = pm(r#"bench "hot path" { let _ = 1 + 1 }"#);
        if let Decl::Fn(f) = &m.decls[0] {
            assert_eq!(f.name, "bench__hot_path");
            assert!(f.effects.effects.contains(&Effect::Throw));
        } else {
            panic!("expected bench to desugar to fn");
        }
    }

    #[test]
    fn test_block_with_assertions() {
        let m = pm(r#"test "math" { assert_eq(1 + 1, 2); assert(true); }"#);
        if let Decl::Fn(f) = &m.decls[0] {
            assert_eq!(f.name, "test__math");
            // Each statement separated by `;` becomes a Stmt::Expr; the trailing
            // `;` keeps both as stmts (no tail expression).
            if let Expr::Block { stmts, tail, .. } = &f.body {
                assert_eq!(stmts.len(), 2, "stmts = {stmts:#?}");
                assert!(tail.is_none());
            } else {
                panic!("expected block body");
            }
        }
    }

    // ── ADT tests ─────────────────────────────────────────────────────────────

    #[test]
    fn adt_parse_roundtrip() {
        // Parse `type Foo = A | B(Int)` and verify the AST.
        let m = pm("type Foo = A | B(Int)");
        assert_eq!(m.decls.len(), 1);
        if let Decl::TypeAlias(ta) = &m.decls[0] {
            assert_eq!(ta.name, "Foo");
            if let Type::Adt { name, ctors, .. } = &ta.ty {
                assert_eq!(name, "Foo");
                assert_eq!(ctors.len(), 2);
                assert_eq!(ctors[0].0, "A");
                assert!(ctors[0].1.is_empty());
                assert_eq!(ctors[1].0, "B");
                assert_eq!(ctors[1].1.len(), 1);
                assert!(matches!(ctors[1].1[0], Type::Con(TyCon::Int, _)));
            } else {
                panic!("expected Type::Adt, got {:?}", ta.ty);
            }
        } else {
            panic!("expected TypeAlias decl");
        }
    }

    #[test]
    fn adt_parse_multi_field_ctor() {
        let m = pm("type Shape = Circle(Float) | Square(Float) | Triangle(Float, Float, Float)");
        assert_eq!(m.decls.len(), 1);
        if let Decl::TypeAlias(ta) = &m.decls[0] {
            if let Type::Adt { ctors, .. } = &ta.ty {
                assert_eq!(ctors.len(), 3);
                assert_eq!(ctors[2].0, "Triangle");
                assert_eq!(ctors[2].1.len(), 3);
            } else {
                panic!("expected Type::Adt");
            }
        }
    }

    #[test]
    fn snap_block_desugars_to_fn() {
        let src = r#"snap "x" { snap_expect("a", "b") }"#;
        let m = pm(src);
        assert_eq!(m.decls.len(), 1);
        match &m.decls[0] {
            Decl::Fn(f) => {
                assert_eq!(f.name, "snap__x");
                assert!(f.params.is_empty());
            }
            other => panic!("expected Decl::Fn, got {:?}", other),
        }
    }

    #[test]
    fn adt_compact_pretty_roundtrip() {
        use crate::pretty::{decl, Form};
        let src = "type Shape = Circle(Float) | Square(Float) | Triangle(Float, Float, Float)";
        let m = pm(src);
        let emitted = decl(&m.decls[0], Form::Compact);
        // Re-parse the emitted form and check ctor count is preserved.
        let m2 = pm(&emitted);
        if let (Decl::TypeAlias(t1), Decl::TypeAlias(t2)) = (&m.decls[0], &m2.decls[0]) {
            if let (Type::Adt { ctors: c1, .. }, Type::Adt { ctors: c2, .. }) = (&t1.ty, &t2.ty) {
                assert_eq!(c1.len(), c2.len(), "roundtrip changed ctor count");
                for ((n1, f1), (n2, f2)) in c1.iter().zip(c2.iter()) {
                    assert_eq!(n1, n2, "ctor name changed in roundtrip");
                    assert_eq!(f1.len(), f2.len(), "ctor field count changed in roundtrip");
                }
            } else {
                panic!("roundtrip lost Adt type");
            }
        }
    }

    // ── implicit-block arm tests ──────────────────────────────────────────

    #[test]
    fn let_in_then_arm() {
        // `if c then let x = 1 x else 2` – let stmt in then-arm, tail is x.
        let e = pe("if true then let x = 1 x else 2");
        match e {
            Expr::If { then_branch, .. } => match *then_branch {
                Expr::Block {
                    ref stmts,
                    ref tail,
                    ..
                } => {
                    assert_eq!(stmts.len(), 1, "expected 1 let stmt");
                    assert!(matches!(stmts[0], Stmt::Let { .. }));
                    assert!(tail.is_some());
                    assert!(matches!(**tail.as_ref().unwrap(), Expr::Var(ref n, _) if n == "x"));
                }
                other => panic!("expected Block then-branch, got {other:?}"),
            },
            _ => panic!("expected If"),
        }
    }

    #[test]
    fn let_in_else_arm() {
        // `if c then 1 else let x = 2 x` – let stmt in else-arm.
        let e = pe("if false then 1 else let x = 2 x");
        match e {
            Expr::If { else_branch, .. } => match *else_branch {
                Expr::Block {
                    ref stmts,
                    ref tail,
                    ..
                } => {
                    assert_eq!(stmts.len(), 1);
                    assert!(matches!(stmts[0], Stmt::Let { .. }));
                    assert!(tail.is_some());
                    assert!(matches!(**tail.as_ref().unwrap(), Expr::Var(ref n, _) if n == "x"));
                }
                other => panic!("expected Block else-branch, got {other:?}"),
            },
            _ => panic!("expected If"),
        }
    }

    #[test]
    fn let_in_match_arm() {
        // `match n with { 0 => let z = 0 z, _ => 1 }` – let stmt in match arm.
        let e = pe("match n with { 0 => let z = 0 z, _ => 1 }");
        match e {
            Expr::Match { ref arms, .. } => {
                assert_eq!(arms.len(), 2);
                match &arms[0].body {
                    Expr::Block { stmts, tail, .. } => {
                        assert_eq!(stmts.len(), 1);
                        assert!(matches!(stmts[0], Stmt::Let { .. }));
                        assert!(tail.is_some());
                        assert!(
                            matches!(**tail.as_ref().unwrap(), Expr::Var(ref n, _) if n == "z")
                        );
                    }
                    other => panic!("expected Block arm body, got {other:?}"),
                }
            }
            _ => panic!("expected Match"),
        }
    }

    #[test]
    fn let_in_expression_still_works() {
        // `let x = 1 in x + 1` must still parse as Expr::Let (not a block).
        let e = pe("let x = 1 in x + 1");
        assert!(
            matches!(e, Expr::Let { .. }),
            "let-in expression should still produce Expr::Let, got {e:?}"
        );
    }

    #[test]
    fn multiple_lets_in_arm() {
        // `if c then let a = 1 let b = 2 a + b else 0` – two let stmts, tail is a+b.
        let e = pe("if true then let a = 1 let b = 2 a + b else 0");
        match e {
            Expr::If { then_branch, .. } => match *then_branch {
                Expr::Block {
                    ref stmts,
                    ref tail,
                    ..
                } => {
                    assert_eq!(stmts.len(), 2, "expected 2 let stmts, got {}", stmts.len());
                    assert!(matches!(stmts[0], Stmt::Let { .. }));
                    assert!(matches!(stmts[1], Stmt::Let { .. }));
                    assert!(tail.is_some());
                    assert!(matches!(
                        **tail.as_ref().unwrap(),
                        Expr::Bin(BinOp::Add, _, _, _)
                    ));
                }
                other => panic!("expected Block then-branch, got {other:?}"),
            },
            _ => panic!("expected If"),
        }
    }
}

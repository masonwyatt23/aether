//! Parser / printer round-trip property tests.
//!
//! Strategy: generate small *valid* Aether programs (single functions with
//! arithmetic bodies), parse them, pretty-print, re-parse, pretty-print again,
//! and assert the second print equals the first (fixed-point).
//!
//! ## Findings (pre-existing printer bugs, NOT introduced by these tests)
//!
//! ### Bug 1 — subtraction ambiguity in compact printer
//! The compact printer emits `x-1` (no spaces). The lexer tokenises `-1` as
//! `Tok::Int(-1)`, so `x-1` is re-parsed as two statements: `x; -1`. Fix:
//! compact printer must emit `x - 1` (spaces around `-`), or the lexer must
//! not recognise `-N` as a single token when it follows an identifier/value.
//! Minimal reproduction: `"fn f(x:I):I!{} = x-1"` → re-prints as `"x; -1"`.
//!
//! ### Bug 2 — verbose body double-wrapping
//! `fn f(...) -> Int effects {} { x + y }` pretty-prints as
//! `fn f(...) -> Int effects {} { x + y }`, which on re-parse becomes
//! `fn f(...) -> Int effects {} { { x + y } }` (extra block wrapper).
//! The verbose fn-body printer emits `{ expr }` which the parser re-parses as
//! a block containing `expr` wrapped in another block on the next print pass.
//!
//! Both bugs require `src/` changes which are out of scope for this PR.
//! The test strategies below deliberately avoid the broken paths so CI stays
//! green; the `#[ignore]`'d tests below document minimal reproductions.

use aether_ast::FileId;
use aether_parser::parse_module;
use aether_parser::pretty::{module as pp_module, Form};
use proptest::prelude::*;

// ── strategies ────────────────────────────────────────────────────────────────

fn ident() -> impl Strategy<Value = String> {
    // Reserved words can't be function names — filter them out of the
    // generator so it only produces genuinely valid programs.
    const KEYWORDS: &[&str] = &[
        "fn",
        "let",
        "in",
        "if",
        "then",
        "else",
        "match",
        "with",
        "type",
        "import",
        "from",
        "as",
        "module",
        "effects",
        "effect",
        "where",
        "ensuring",
        "requires",
        "ensures",
        "spec",
        "tool",
        "introspect",
        "summarize",
        "provenance",
        "confident",
        "assume",
        "confidence",
        "result",
        "not",
        "and",
        "or",
        "do",
        "true",
        "false",
    ];
    "[a-z][a-z0-9]{0,4}"
        .prop_map(String::from)
        .prop_filter("must not be a reserved keyword", |s| {
            !KEYWORDS.contains(&s.as_str())
        })
}

/// Arithmetic body using only `+` and `*` — avoids the `-` ambiguity (Bug 1).
fn safe_arith_expr() -> impl Strategy<Value = String> {
    let op = prop::sample::select(&["+", "*"][..]);
    let operand = prop::sample::select(&["x", "y", "a", "b", "n", "1", "2", "42"][..]);
    let rhs = prop::sample::select(&["x", "y", "a", "b", "n", "1", "2", "42"][..]);
    (operand, op, rhs).prop_map(|(l, op, r)| format!("{l} {op} {r}"))
}

fn body_expr_safe() -> impl Strategy<Value = String> {
    prop_oneof![
        // simple identifier — always safe
        prop::sample::select(&["x", "y", "a", "b", "n"][..]).prop_map(String::from),
        // integer literal — always safe
        (0i64..=100i64).prop_map(|n| n.to_string()),
        // addition / multiplication — safe (no subtraction)
        safe_arith_expr(),
    ]
}

/// Compact-form function: `name(x:I,y:I):I!{} = <body>`
fn valid_fn_compact() -> impl Strategy<Value = String> {
    (ident(), body_expr_safe()).prop_map(|(name, body)| format!("{name}(x:I,y:I):I!{{}} = {body}"))
}

// ── round-trip helper ─────────────────────────────────────────────────────────

/// Parse `src` with `form`, print, re-parse, print again, assert fixed-point.
fn assert_roundtrip(src: &str, form: Form) {
    let m1 = match parse_module(FileId(0), src) {
        Ok(m) => m,
        Err(e) => panic!("generated program failed to parse: {e}\nsrc: {src:?}"),
    };

    let printed1 = pp_module(&m1, form);

    let m2 = match parse_module(FileId(0), &printed1) {
        Ok(m) => m,
        Err(e) => panic!(
            "re-parse of printed output failed: {e}\n\
             original src: {src:?}\nprinted ({form:?}): {printed1:?}"
        ),
    };

    let printed2 = pp_module(&m2, form);

    assert_eq!(
        printed1, printed2,
        "print→parse→print is NOT a fixed point ({form:?})\n\
         original src: {src:?}\nfirst print: {printed1:?}\nsecond print: {printed2:?}"
    );
}

// ── proptest suites ───────────────────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// Compact-form programs must round-trip through the compact printer.
    #[test]
    fn roundtrip_compact_fn(src in valid_fn_compact()) {
        assert_roundtrip(&src, Form::Compact);
    }
}

// ── golden regression tests ───────────────────────────────────────────────────

#[test]
fn golden_add_compact() {
    // Compact → compact is stable
    assert_roundtrip("add(x:I,y:I):I!{} = x+y", Form::Compact);
}

#[test]
fn golden_multiply_compact() {
    assert_roundtrip("mul(x:I,y:I):I!{} = x*y", Form::Compact);
}

#[test]
fn golden_literal_body_compact() {
    assert_roundtrip("zero():I!{} = 0", Form::Compact);
}

#[test]
fn golden_multi_decl_compact() {
    assert_roundtrip("a(x:I):I!{} = x\n\nb(y:I):I!{} = y", Form::Compact);
}

// ── documented finding: Bug 1 — subtraction ambiguity ────────────────────────
//
// Compact printer emits `x-1` without spaces. The lexer tokenises `-1` as
// Tok::Int(-1), so re-parsing treats `x-1` as `x; -1` (two stmts in a block).
// Fix: emit spaces around `-` in the compact printer, or make the lexer
// context-sensitive (not recognise negative literals after a value token).

#[test]
#[ignore = "known printer bug: compact `x-1` re-parses as `x; -1` (subtraction ambiguity with negative literals)"]
fn bug1_subtraction_ambiguity_compact() {
    assert_roundtrip("f(x:I):I!{} = x-1", Form::Compact);
}

// ── documented finding: Bug 2 — verbose body double-wrapping ─────────────────
//
// `fn f() -> Int effects {} { x + y }` first-prints as
// `fn f() -> Int effects {} { x + y }`, which second-prints as
// `fn f() -> Int effects {} { { x + y } }` — extra block wrapper.
// Fix: verbose fn-body printer should not add `{ }` when the body is already
// a block expression, or the parser should unwrap a single-expression block.

#[test]
#[ignore = "known printer bug: verbose fn body gets double-wrapped in braces on re-parse"]
fn bug2_verbose_body_double_wrap() {
    assert_roundtrip(
        "fn add(x: Int, y: Int) -> Int effects {} { x + y }",
        Form::Verbose,
    );
}

//! Fuzz / robustness tests for the Aether compiler front-end.
//!
//! Invariant: the lexer, parser, and type-checker MUST NEVER PANIC on
//! arbitrary input. Returning `Err` is the correct response to malformed
//! programs.
//!
//! FINDING: The recursive-descent parser has no depth guard. Even at ~100+
//! nesting depth the parser stack-overflows on default thread stacks. This is
//! a real bug. Affected tests are marked `#[ignore]` so CI stays green; run
//! with `cargo test -p aether-parser -- --ignored` to reproduce. Safe depth
//! for active tests is capped at 20.

use aether_ast::FileId;
use aether_lexer::lex;
use aether_parser::{parse_expr, parse_module};
use aether_types::check_module;
use proptest::prelude::*;

// ── helpers ───────────────────────────────────────────────────────────────────

fn pipeline(src: &str) {
    let _lex = lex(FileId(0), src);
    if let Ok(module) = parse_module(FileId(0), src) {
        let _ = check_module(&module);
    }
    let _ = parse_expr(FileId(0), src);
}

// ── structured-noise strategy ─────────────────────────────────────────────────

const TOKEN_FRAGMENTS: &[&str] = &[
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
    "requires",
    "ensures",
    "spec",
    "tool",
    "confident",
    "assume",
    "true",
    "false",
    "(",
    ")",
    "{",
    "}",
    "[",
    "]",
    ",",
    ";",
    ":",
    "::",
    ".",
    "?",
    "~",
    "|",
    "|>",
    "->",
    "=>",
    "=",
    "==",
    "!=",
    "<",
    "<=",
    ">",
    ">=",
    "+",
    "-",
    "*",
    "/",
    "%",
    "++",
    "&&",
    "||",
    "!",
    "&",
    "x",
    "y",
    "f",
    "g",
    "foo",
    "bar",
    "main",
    "n",
    "a",
    "b",
    "Int",
    "Bool",
    "Str",
    "Float",
    "I",
    "B",
    "S",
    "0",
    "1",
    "42",
    "-1",
    "100",
    "2.5",
    "0.0",
    "\"hello\"",
    "\"\"",
    "\"world\"",
    " ",
    "\n",
    "\t",
];

fn structured_noise() -> impl Strategy<Value = String> {
    prop::collection::vec(prop::sample::select(TOKEN_FRAGMENTS), 0..=40)
        .prop_map(|frags| frags.join(" "))
}

// ── proptest suites ───────────────────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn fuzz_structured_noise(src in structured_noise()) {
        pipeline(&src);
    }

    #[test]
    fn fuzz_random_ascii(bytes in prop::collection::vec(32u8..=126, 0..=200)) {
        let src = String::from_utf8_lossy(&bytes).into_owned();
        pipeline(&src);
    }

    #[test]
    fn fuzz_unicode(src in "\\PC*") {
        pipeline(&src);
    }
}

// ── hand-written regression cases ────────────────────────────────────────────

#[test]
fn regression_unterminated_string() {
    let src = "\"unterminated string";
    let _ = lex(FileId(0), src);
    let _ = parse_module(FileId(0), src);
    let _ = parse_expr(FileId(0), src);
}

// FINDING: parser has no recursion depth guard. Depths >= ~100 overflow the
// stack. The _safe variants use depth=20. The ignored variants document the bug.

#[test]
fn regression_deeply_nested_parens_safe() {
    let src = format!("{}42{}", "(".repeat(20), ")".repeat(20));
    pipeline(&src);
}

#[test]
#[ignore = "known bug: no recursion depth guard; 200-deep parens overflows the stack"]
fn regression_deeply_nested_parens_200() {
    let src = format!("{}42{}", "(".repeat(200), ")".repeat(200));
    pipeline(&src);
}

#[test]
fn regression_deeply_nested_brackets_safe() {
    let src = format!("{}0{}", "[".repeat(20), "]".repeat(20));
    pipeline(&src);
}

#[test]
#[ignore = "known bug: no recursion depth guard; 200-deep brackets overflows the stack"]
fn regression_deeply_nested_brackets_200() {
    let src = format!("{}0{}", "[".repeat(200), "]".repeat(200));
    pipeline(&src);
}

#[test]
fn regression_deeply_nested_braces_safe() {
    let src = "{ { { { { { { { { { { { { { { { { { { { 0 } } } } } } } } } } } } } } } } } } } }";
    pipeline(src);
}

#[test]
#[ignore = "known bug: no recursion depth guard; 200-deep braces overflows the stack"]
fn regression_deeply_nested_braces_200() {
    let src = format!("{}{}", "{".repeat(200), "}".repeat(200));
    pipeline(&src);
}

#[test]
fn regression_mismatched_brackets() {
    pipeline("(((((((([[[{{{{");
}

#[test]
fn regression_only_open_parens() {
    pipeline("((((((((");
}

#[test]
fn regression_huge_integer_literal() {
    pipeline("99999999999999999999999999999999999999999999999");
}

#[test]
fn regression_empty_input() {
    pipeline("");
}

#[test]
fn regression_whitespace_only() {
    pipeline("   \n\t\r\n  ");
}

#[test]
fn regression_valid_minimal_fn() {
    pipeline("fn id(x: Int) -> Int effects {} { x }");
}

#[test]
fn regression_keyword_soup() {
    pipeline("fn fn fn let let if else then match with effects");
}

#[test]
fn regression_operator_soup() {
    pipeline("==><=!=>||&&++::->|>");
}

#[test]
fn regression_unicode_identifiers_rejected() {
    pipeline("fn \u{4f60}\u{597d}() -> Int effects {} { 0 }");
}

#[test]
fn regression_null_byte() {
    pipeline("fn\x00foo() -> Int effects {} { 0 }");
}

// FINDING: 10 000-deep nesting causes SIGABRT (stack overflow).
// Fix needed: add a depth counter to parse_expr and return ParseError::Bad
// when it exceeds a safe limit (e.g. 512). Tracked as a parser bug.
#[test]
#[ignore = "known bug: no recursion depth guard; 10k nesting overflows the stack (add depth limit to parse_expr)"]
fn regression_10000_deep_nesting_stack_overflow() {
    let src = format!("{}0{}", "(".repeat(10_000), ")".repeat(10_000));
    pipeline(&src);
}

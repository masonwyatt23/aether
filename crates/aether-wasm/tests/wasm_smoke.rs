//! Native (non-wasm) smoke tests for the aether-wasm API layer.
//!
//! These call `check_source` / `run_source` directly — no wasm-pack or
//! headless browser needed. They prove the pipeline compiles and executes
//! correctly on the native target, giving confidence that the same code will
//! work in the browser once compiled to wasm32.

use aether_wasm::{check_source, run_source};

#[test]
fn check_clean_program_has_no_errors() {
    let src = r#"fn main() -> Unit effects {IO} { print("hello from wasm") }"#;
    let result = check_source(src);
    assert!(
        result.ok,
        "expected no errors, got: {:?}",
        result.errors.iter().map(|e| &e.message).collect::<Vec<_>>()
    );
    assert!(result.errors.is_empty());
}

#[test]
fn check_type_error_is_detected() {
    // `not_a_bool` is an Int, but the `if` condition must be Bool.
    let src = r#"fn main() -> Unit effects {} { if 42 then () else () }"#;
    let result = check_source(src);
    assert!(!result.ok, "expected errors for type mismatch");
    assert!(!result.errors.is_empty());
}

#[test]
fn run_hello_world_captures_stdout() {
    let src = r#"fn main() -> Unit effects {IO} { print("hello, playground") }"#;
    let result = run_source(src);
    assert!(result.ok, "run failed: {:?}", result.error);
    assert!(
        result.stdout.contains("hello, playground"),
        "stdout was: {:?}",
        result.stdout
    );
}

#[test]
fn run_refinement_example() {
    let src = r#"
fn min(a: Int, b: Int) -> Int
  where result <= a && result <= b
  effects {} {
  if a <= b then a else b
}

fn main() -> Unit effects {IO} {
  let m = min(3, 7)
  print(str(m))
}
"#;
    let result = run_source(src);
    assert!(result.ok, "run failed: {:?}", result.error);
    assert!(
        result.stdout.contains('3'),
        "expected '3' in stdout, got: {:?}",
        result.stdout
    );
}

#[test]
fn run_returns_value_for_non_unit() {
    let src = r#"fn main() -> Int effects {} { 42 }"#;
    let result = run_source(src);
    assert!(result.ok, "run failed: {:?}", result.error);
    assert_eq!(result.value.as_deref(), Some("42"));
}

#[test]
fn run_runtime_error_reported() {
    // Division by zero should produce an error.
    let src = r#"fn main() -> Int effects {} { 10 / 0 }"#;
    let result = run_source(src);
    assert!(!result.ok, "expected runtime error");
    assert!(result.error.is_some());
}

#[test]
fn check_diagnostics_have_line_col() {
    // Parse error: intentionally malformed source.
    let src = "fn main() -> Unit effects {} { ???garbage??? }";
    let result = check_source(src);
    assert!(!result.ok);
    // At minimum one diagnostic should have line >= 1, col >= 1.
    let has_location = result.errors.iter().any(|d| d.line >= 1 && d.col >= 1);
    assert!(
        has_location,
        "expected diagnostics with line/col: {:?}",
        result
            .errors
            .iter()
            .map(|e| (e.line, e.col))
            .collect::<Vec<_>>()
    );
}

#[test]
fn format_compact_roundtrips() {
    let src = r#"fn add(x: Int, y: Int) -> Int effects {} { x + y }"#;
    let formatted = aether_wasm::aether_format(src, false);
    assert!(
        !formatted.starts_with("error:"),
        "format failed: {}",
        formatted
    );
    // Re-check the formatted source — it should still parse cleanly.
    let re_checked = check_source(&formatted);
    assert!(
        re_checked.ok,
        "re-check of formatted source failed: {:?}",
        re_checked.errors
    );
}

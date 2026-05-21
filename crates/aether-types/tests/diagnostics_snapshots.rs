//! Snapshot tests for compiler diagnostic output.
//!
//! Each test compiles a deliberately-broken Aether program and snapshots the
//! rendered diagnostics.  The `.snap` files under `tests/snapshots/` are the
//! regression baseline: a diff here means an error *message* changed, which
//! should always be a conscious decision.
//!
//! When an error message is intentionally improved:
//!   1. Run `cargo insta review` (or rename the `.snap.new` → `.snap` manually).
//!   2. Commit the updated snapshot alongside the code change.
//!
//! Snapshots are deterministic:
//! - Spans are rendered as `start..end` byte offsets only — no `FileId` or
//!   file names that vary between machines.
//! - The SMT escalation path (z3) is disabled for every test via the
//!   `AETHER_DISABLE_SMT` env var so results are identical regardless of
//!   whether z3 is installed.  Mutation of a process-wide env var is
//!   serialised with a `Mutex` so parallel test threads don't interfere.
//! - Diagnostics are sorted by `(start, end, severity_ord, msg)` before
//!   joining, so insertion-order differences can't flip a snapshot.

use aether_ast::FileId;
use aether_parser::parse_module;
use aether_types::check::check_module;
use aether_types::check::Severity;
use std::sync::Mutex;

// ------------------------------------------------------------------
// Helpers
// ------------------------------------------------------------------

/// Global mutex that serialises every test touching AETHER_DISABLE_SMT so that
/// tests running in parallel cannot observe a partially-set env var.
static SMT_LOCK: Mutex<()> = Mutex::new(());

/// Parse `src`, run `check_module`, then render and sort diagnostics into a
/// stable multi-line string.  Format per line:
///   `{Severity} @ {start}..{end}: {msg}`
fn diagnostics(src: &str) -> String {
    // Hold the lock for the duration of parse + check so no other test
    // racing this one can clear the var before the solver is called.
    let _guard = SMT_LOCK.lock().unwrap();
    // Disable z3 escalation so results are deterministic.
    // SAFETY: single-threaded within the lock above.
    unsafe {
        std::env::set_var("AETHER_DISABLE_SMT", "1");
    }
    let m = parse_module(FileId(0), src).expect("parse error in test source");
    let (_, diags) = check_module(&m);
    unsafe {
        std::env::remove_var("AETHER_DISABLE_SMT");
    }

    let mut lines: Vec<String> = diags
        .iter()
        .map(|d| {
            let sev = match d.severity {
                Severity::Error => "Error",
                Severity::Warning => "Warning",
                Severity::Note => "Note",
            };
            format!("{} @ {}..{}: {}", sev, d.span.start, d.span.end, d.msg)
        })
        .collect();

    // Sort for determinism: by start offset, then end, then text.
    lines.sort();
    lines.join("\n")
}

// ------------------------------------------------------------------
// 1. Undeclared effect — function calls `print` but declares `effects {}`
// ------------------------------------------------------------------
#[test]
fn snap_undeclared_effect_io() {
    let src = r#"fn greet(name: Str) -> Unit effects {} { print(name) }"#;
    insta::assert_snapshot!(diagnostics(src));
}

// ------------------------------------------------------------------
// 2. Effect-row mismatch — uses IO *and* Net, only Net is declared
// ------------------------------------------------------------------
#[test]
fn snap_effect_row_mismatch() {
    let src = r#"fn combined(u: Str) -> Str effects {Net} { print(u) }"#;
    insta::assert_snapshot!(diagnostics(src));
}

// ------------------------------------------------------------------
// 3. Type mismatch — function declared to return Int, body is Str literal
// ------------------------------------------------------------------
#[test]
fn snap_return_type_mismatch_int_vs_str() {
    let src = r#"fn get_count() -> Int effects {} { "forty-two" }"#;
    insta::assert_snapshot!(diagnostics(src));
}

// ------------------------------------------------------------------
// 4. Unbound identifier — "did you mean" suggestion expected
// ------------------------------------------------------------------
#[test]
fn snap_unbound_identifier_with_suggestion() {
    // `prnt` is 1 edit from builtin `print`
    let src = r#"fn f() -> Unit effects {IO} { prnt("hello") }"#;
    insta::assert_snapshot!(diagnostics(src));
}

// ------------------------------------------------------------------
// 5. Unbound identifier — no close match, no suggestion
// ------------------------------------------------------------------
#[test]
fn snap_unbound_identifier_no_suggestion() {
    let src = r#"fn f() -> Unit effects {} { xyzqwerty123 }"#;
    insta::assert_snapshot!(diagnostics(src));
}

// ------------------------------------------------------------------
// 6. Calling an unknown function — suggestion toward nearest builtin
// ------------------------------------------------------------------
#[test]
fn snap_unknown_function_with_suggestion() {
    // `prnt` is used as a call — should suggest `print`
    let src = r#"fn f() -> Unit effects {IO} { prnt("world") }"#;
    insta::assert_snapshot!(diagnostics(src));
}

// ------------------------------------------------------------------
// 7. Wrong argument count — too many args
// ------------------------------------------------------------------
#[test]
fn snap_too_many_arguments() {
    let src = "fn add(x: Int, y: Int) -> Int effects {} { x + y }\n\
               fn main() -> Int effects {} { add(1, 2, 3) }";
    insta::assert_snapshot!(diagnostics(src));
}

// ------------------------------------------------------------------
// 8. Wrong argument type — Str where Int expected
// ------------------------------------------------------------------
#[test]
fn snap_wrong_argument_type() {
    let src = r#"fn double(n: Int) -> Int effects {} { n + n }
fn main() -> Int effects {} { double("oops") }"#;
    insta::assert_snapshot!(diagnostics(src));
}

// ------------------------------------------------------------------
// 9. ADT constructor arity error — Circle takes 1 field, given 2
// ------------------------------------------------------------------
#[test]
fn snap_adt_ctor_arity_error() {
    let src = "type Shape = Circle(Float) | Square(Float)\n\
               fn bad() -> Shape effects {} { Circle(1.0, 2.0) }";
    insta::assert_snapshot!(diagnostics(src));
}

// ------------------------------------------------------------------
// 10. Non-exhaustive match warning — missing Triangle constructor
// ------------------------------------------------------------------
#[test]
fn snap_non_exhaustive_match_warning() {
    let src = "type Shape = Circle(Float) | Square(Float) | Triangle(Float, Float, Float)\n\
               fn area(s: Shape) -> Float effects {} {\n\
                 match s with {\n\
                   Circle(r)    => r * r * 3.14,\n\
                   Square(side) => side * side\n\
                 }\n\
               }";
    insta::assert_snapshot!(diagnostics(src));
}

// ------------------------------------------------------------------
// 11. Refuted refinement postcondition — counterexample witness in message
// ------------------------------------------------------------------
#[test]
fn snap_refuted_postcondition_with_witness() {
    // result > 0 is false when body is -1; linear solver finds witness
    let src = "fn always_negative() -> Int where result > 0 effects {} { -1 }";
    insta::assert_snapshot!(diagnostics(src));
}

// ------------------------------------------------------------------
// 12. Unprovable (non-linear) postcondition → "could not be verified" warning
//     AETHER_DISABLE_SMT=1 forces linear-only path so non-linear → Unknown → Warning
// ------------------------------------------------------------------
#[test]
fn snap_unprovable_nonlinear_postcondition() {
    // x * x >= 0 is always true but outside the linear fragment; with SMT
    // disabled the solver returns Unknown, downgraded to a warning.
    let src = "fn square(x: Int) -> Int where result >= 0 effects {} { x * x }";
    insta::assert_snapshot!(diagnostics(src));
}

// ------------------------------------------------------------------
// 13. Generic function call type error — id(42) used where Str expected
// ------------------------------------------------------------------
#[test]
fn snap_generic_call_return_type_mismatch() {
    let src = "fn id<A>(x: A) -> A effects {} { x }\n\
               fn main() -> Str effects {} { id(42) }";
    insta::assert_snapshot!(diagnostics(src));
}

// ------------------------------------------------------------------
// 14. Branch type mismatch in `if`
// ------------------------------------------------------------------
#[test]
fn snap_if_branch_type_mismatch() {
    let src = r#"fn pick(flag: Bool) -> Int effects {} {
  if flag { 1 } else { "wrong" }
}"#;
    insta::assert_snapshot!(diagnostics(src));
}

// ------------------------------------------------------------------
// 15. Generic arg consistency — same<A>(Int, Str) is a type error
// ------------------------------------------------------------------
#[test]
fn snap_generic_arg_consistency_error() {
    let src = "fn same<A>(x: A, y: A) -> A effects {} { x }\n\
               fn main() -> Int effects {} { same(1, \"two\") }";
    insta::assert_snapshot!(diagnostics(src));
}

//! End-to-end tests for `aether verify`.
//!
//! Tests spawn the built `aether` binary and assert on exit codes and output,
//! following the same pattern as the other `tests/*.rs` files.
//!
//! Several tests set `AETHER_DISABLE_SMT=1` to suppress z3 escalation so
//! that non-linear postconditions produce `Verdict::Unknown` (Warning)
//! rather than being proved by z3.  This makes the check-vs-verify contrast
//! deterministic whether or not z3 is installed.

use std::process::Command;

fn aether() -> Command {
    Command::new(env!("CARGO_BIN_EXE_aether"))
}

fn tempfile_path(name: &str) -> std::path::PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("aether_verify_{}_{name}", std::process::id()));
    p
}

/// `aether verify` exits 0 on a file whose postconditions are all provable
/// by the linear-arithmetic solver (no SMT needed).
#[test]
fn verify_passes_on_proven_contract() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("examples")
        .join("31_verify_gate.ae");

    let out = aether()
        .arg("verify")
        .arg(&dir)
        .output()
        .expect("invoke aether verify");

    assert!(
        out.status.success(),
        "aether verify should exit 0 on a fully-proven file:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("verified"),
        "stdout should contain 'verified', got: {stdout}"
    );
}

/// `aether verify` exits non-zero on a file with a non-linear postcondition
/// that the linear solver cannot prove (Unknown -> Warning).
/// `aether check` on the exact same file exits 0 (lenient -- tolerates warnings).
///
/// `AETHER_DISABLE_SMT=1` suppresses z3 escalation so the result is
/// deterministic whether or not z3 is on PATH.
#[test]
fn verify_fails_on_unproven_contract() {
    let tmp = tempfile_path("unproven.ae");
    std::fs::write(
        &tmp,
        "fn mul(x: Int, y: Int) -> Int where result == x * y effects {} { x * y }\n         fn main() -> Unit effects {IO} { print(str(mul(3, 4))) }\n",
    )
    .unwrap();

    // `aether check` exits 0 -- warnings (unproven contracts) are tolerated.
    let check_out = aether()
        .arg("check")
        .arg(&tmp)
        .env("AETHER_DISABLE_SMT", "1")
        .output()
        .expect("invoke aether check");
    assert!(
        check_out.status.success(),
        "aether check should exit 0 (lenient) on an unproven contract:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&check_out.stdout),
        String::from_utf8_lossy(&check_out.stderr),
    );

    // `aether verify` exits non-zero -- unproven contracts are hard failures.
    let verify_out = aether()
        .arg("verify")
        .arg(&tmp)
        .env("AETHER_DISABLE_SMT", "1")
        .output()
        .expect("invoke aether verify");
    assert!(
        !verify_out.status.success(),
        "aether verify should exit non-zero on an unproven contract:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&verify_out.stdout),
        String::from_utf8_lossy(&verify_out.stderr),
    );

    let stdout = String::from_utf8_lossy(&verify_out.stdout);
    assert!(
        stdout.contains("verification failed") || stdout.contains("unproven"),
        "failure output should mention verification failure, got: {stdout}"
    );
}

/// `aether verify --json` emits parseable JSON with a `verified` boolean.
#[test]
fn verify_json_shape() {
    // Passing case: proven file -> verified:true.
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("examples")
        .join("31_verify_gate.ae");

    let out = aether()
        .arg("verify")
        .arg("--json")
        .arg(&dir)
        .output()
        .expect("invoke aether verify --json");

    assert!(
        out.status.success(),
        "aether verify --json should exit 0 on a proven file:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );

    let stdout = String::from_utf8_lossy(&out.stdout);
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("--json output should be valid JSON");

    assert_eq!(parsed["verified"], serde_json::Value::Bool(true));
    assert!(parsed["errors"].is_number(), "errors should be a number");
    assert!(
        parsed["warnings"].is_number(),
        "warnings should be a number"
    );
    assert!(
        parsed["diagnostics"].is_array(),
        "diagnostics should be an array"
    );
    assert_eq!(parsed["errors"], 0);
    assert_eq!(parsed["warnings"], 0);

    // Failing case: non-linear postcondition with SMT disabled -> verified:false.
    let tmp = tempfile_path("unproven_json.ae");
    std::fs::write(
        &tmp,
        "fn mul(x: Int, y: Int) -> Int where result == x * y effects {} { x * y }\n         fn main() -> Unit effects {IO} { print(str(mul(3, 4))) }\n",
    )
    .unwrap();

    let fail_out = aether()
        .arg("verify")
        .arg("--json")
        .arg(&tmp)
        .env("AETHER_DISABLE_SMT", "1")
        .output()
        .expect("invoke aether verify --json on unproven");

    assert!(
        !fail_out.status.success(),
        "aether verify --json should exit non-zero on an unproven file"
    );

    let fail_stdout = String::from_utf8_lossy(&fail_out.stdout);
    let fail_parsed: serde_json::Value =
        serde_json::from_str(&fail_stdout).expect("--json failure output should be valid JSON");

    assert_eq!(
        fail_parsed["verified"],
        serde_json::Value::Bool(false),
        "verified field should be false for an unproven file"
    );
    assert!(
        fail_parsed["warnings"]
            .as_u64()
            .map(|n| n > 0)
            .unwrap_or(false),
        "warnings should be > 0 for an unproven file, got: {fail_parsed}"
    );
}

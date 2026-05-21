//! End-to-end tests for the multi-file module loader and import resolver.

use std::path::PathBuf;
use std::process::Command;

fn aether() -> Command {
    Command::new(env!("CARGO_BIN_EXE_aether"))
}

fn examples_dir() -> PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("examples")
}

fn tmp(name: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("aether_imports_{}_{name}", std::process::id()));
    p
}

// ── import_stdlib_works ───────────────────────────────────────────────────────

/// A program that imports `std::iter` and calls `clamp` must run cleanly.
#[test]
fn import_stdlib_works() {
    let src = tmp("stdlib_works.ae");
    std::fs::write(
        &src,
        r#"import std::iter

fn main() -> Unit effects {IO} {
  print(str(clamp(50, 0, 100)))
  print(str(clamp(-5, 0, 100)))
  print(str(clamp(200, 0, 100)))
}
"#,
    )
    .unwrap();

    // check
    let out = aether().arg("check").arg(&src).output().expect("aether check");
    assert!(
        out.status.success(),
        "check failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    // run
    let out = aether().arg("run").arg(&src).output().expect("aether run");
    assert!(
        out.status.success(),
        "run failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("50"), "expected '50' in output, got: {stdout}");
    assert!(stdout.contains("0"),  "expected '0'  in output, got: {stdout}");
    assert!(stdout.contains("100"), "expected '100' in output, got: {stdout}");
}

// ── cycle_detected ────────────────────────────────────────────────────────────

/// Two files that import each other must produce a clear cycle-detection error.
#[test]
fn cycle_detected() {
    // Create two files in a shared temp directory so relative paths work.
    let dir = tmp("cycle_dir");
    std::fs::create_dir_all(&dir).unwrap();

    let a = dir.join("cycle_a.ae");
    let b = dir.join("cycle_b.ae");

    std::fs::write(
        &a,
        r#"import cycle_b

fn main() -> Unit effects {} { }
"#,
    )
    .unwrap();
    std::fs::write(
        &b,
        r#"import cycle_a

fn helper() -> Int effects {} { 1 }
"#,
    )
    .unwrap();

    let out = aether().arg("run").arg(&a).output().expect("aether run cycle");
    assert!(
        !out.status.success(),
        "expected cycle error but exit was success"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("cycle") || stderr.contains("Cycle"),
        "expected 'cycle' in stderr, got: {stderr}"
    );
}

// ── unknown_module_errors ─────────────────────────────────────────────────────

/// Importing a module that doesn't exist in stdlib or on disk must produce a
/// clear "unknown module" error.
#[test]
fn unknown_module_errors() {
    let src = tmp("unknown_mod.ae");
    std::fs::write(
        &src,
        r#"import std::nonexistent_module_xyz

fn main() -> Unit effects {} { }
"#,
    )
    .unwrap();

    let out = aether().arg("run").arg(&src).output().expect("aether run");
    assert!(
        !out.status.success(),
        "expected error for unknown module but got success"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("unknown module") || stderr.contains("NotFound") || stderr.contains("nonexistent"),
        "expected module-not-found message in stderr, got: {stderr}"
    );
}

// ── example_08_imports_check_and_run ─────────────────────────────────────────

/// The shipped example 08_imports.ae must check and run cleanly end-to-end.
#[test]
fn example_08_imports_check_and_run() {
    let path = examples_dir().join("08_imports.ae");
    assert!(path.exists(), "examples/08_imports.ae does not exist");

    let check = aether().arg("check").arg(&path).output().expect("aether check");
    assert!(
        check.status.success(),
        "08_imports.ae failed check:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );

    let run = aether().arg("run").arg(&path).output().expect("aether run");
    assert!(
        run.status.success(),
        "08_imports.ae failed run:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("50"),  "expected clamp(50,0,100)=50");
    assert!(stdout.contains("0"),   "expected clamp(-5,0,100)=0");
    assert!(stdout.contains("100"), "expected clamp(200,0,100)=100");
}

// ── transitive_import_works ───────────────────────────────────────────────────

/// Module A imports module B which imports std::iter; A should see clamp.
#[test]
fn transitive_import_works() {
    let dir = tmp("transitive_dir");
    std::fs::create_dir_all(&dir).unwrap();

    let math = dir.join("math.ae");
    let entry = dir.join("entry.ae");

    // math.ae re-exports (by importing) std::iter so entry.ae can use clamp
    // via the transitive merge.
    std::fs::write(
        &math,
        r#"import std::iter

fn double(x: Int) -> Int effects {} { x + x }
"#,
    )
    .unwrap();

    std::fs::write(
        &entry,
        r#"import math

fn main() -> Unit effects {IO} {
  print(str(clamp(5, 0, 10)))
  print(str(double(7)))
}
"#,
    )
    .unwrap();

    let out = aether().arg("run").arg(&entry).output().expect("aether run transitive");
    assert!(
        out.status.success(),
        "transitive import failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("5"),  "expected clamp result 5");
    assert!(stdout.contains("14"), "expected double(7)=14");
}

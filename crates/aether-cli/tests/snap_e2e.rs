//! End-to-end tests for `aether snap`.

use std::process::Command;

fn aether() -> Command {
    Command::new(env!("CARGO_BIN_EXE_aether"))
}

/// Write a temporary `.ae` file and return its path + the temp dir (keep it alive).
fn write_temp_ae(content: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("snap_test.ae");
    std::fs::write(&path, content).expect("write temp .ae");
    (dir, path)
}

const SNAP_SRC: &str = r#"
snap "math" {
    snap_expect("add", str(1 + 2))
    snap_expect("mul", str(3 * 4))
}

fn main() -> Unit effects {} { () }
"#;

#[test]
fn snap_first_run_captures() {
    let (_dir, path) = write_temp_ae(SNAP_SRC);
    let out = aether().arg("snap").arg(&path).output().expect("invoke aether snap");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "first snap run should succeed (capture):\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(stdout.contains("captured"), "expected 'captured' in output:\n{stdout}");

    // Golden file should exist now.
    let golden = path.with_extension("snap");
    assert!(golden.exists(), ".snap file should have been created");
    let golden_content = std::fs::read_to_string(&golden).expect("read .snap");
    assert!(golden_content.contains("[math]"), "section [math] missing from golden:\n{golden_content}");
    assert!(golden_content.contains("add = \"3\""), "add entry missing:\n{golden_content}");
    assert!(golden_content.contains("mul = \"12\""), "mul entry missing:\n{golden_content}");
}

#[test]
fn snap_second_run_verifies_pass() {
    let (_dir, path) = write_temp_ae(SNAP_SRC);
    // First run — capture.
    let _ = aether().arg("snap").arg(&path).output().expect("first snap");
    // Second run — verify.
    let out = aether().arg("snap").arg(&path).output().expect("second snap");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "second snap run (verify) should pass:\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(stdout.contains("ok") || stdout.contains("passed"), "expected pass output:\n{stdout}");
}

#[test]
fn snap_mismatch_fails() {
    let (_dir, path) = write_temp_ae(SNAP_SRC);
    // First run — capture golden with add="3", mul="12".
    let _ = aether().arg("snap").arg(&path).output().expect("first snap");

    // Write a different source that produces different values.
    let broken_src = r#"
snap "math" {
    snap_expect("add", str(1 + 99))
    snap_expect("mul", str(3 * 4))
}

fn main() -> Unit effects {} { () }
"#;
    std::fs::write(&path, broken_src).expect("overwrite .ae");

    let out = aether().arg("snap").arg(&path).output().expect("mismatch snap");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !out.status.success(),
        "mismatch should exit non-zero:\nstdout:\n{stdout}"
    );
    assert!(stdout.contains("mismatch") || stdout.contains("✗"), "expected mismatch in output:\n{stdout}");
}

#[test]
fn snap_update_overwrites_golden() {
    let (_dir, path) = write_temp_ae(SNAP_SRC);
    // Capture initial.
    let _ = aether().arg("snap").arg(&path).output().expect("first snap");

    // Now change source values and use --update.
    let new_src = r#"
snap "math" {
    snap_expect("add", str(10 + 20))
    snap_expect("mul", str(5 * 6))
}

fn main() -> Unit effects {} { () }
"#;
    std::fs::write(&path, new_src).expect("overwrite .ae");
    let out = aether().arg("snap").arg(&path).arg("--update").output().expect("update snap");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "update should succeed:\n{stdout}");

    // Verify new golden has updated values.
    let golden = std::fs::read_to_string(path.with_extension("snap")).expect("read .snap");
    assert!(golden.contains("add = \"30\""), "updated add missing:\n{golden}");
    assert!(golden.contains("mul = \"30\""), "updated mul missing:\n{golden}");
}

#[test]
fn verify_snapshots_run() {
    // Run the canonical example file through aether snap.
    let example = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("examples")
        .join("17_snapshots.ae");

    // First: capture (golden may or may not exist already; --update is idempotent).
    let out = aether()
        .arg("snap")
        .arg(&example)
        .arg("--update")
        .output()
        .expect("invoke aether snap on example");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "17_snapshots.ae snap --update failed:\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    // Second: verify pass.
    let out2 = aether()
        .arg("snap")
        .arg(&example)
        .output()
        .expect("invoke aether snap verify on example");
    let stdout2 = String::from_utf8_lossy(&out2.stdout);
    let stderr2 = String::from_utf8_lossy(&out2.stderr);
    assert!(
        out2.status.success(),
        "17_snapshots.ae snap verify failed:\nstdout:\n{stdout2}\nstderr:\n{stderr2}"
    );
}

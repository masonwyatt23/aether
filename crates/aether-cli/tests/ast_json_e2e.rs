//! End-to-end tests for `aether ast --json`.

use std::process::Command;

fn aether() -> Command {
    Command::new(env!("CARGO_BIN_EXE_aether"))
}

fn examples_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("examples")
}

#[test]
fn ast_json_pretty_parses_and_contains_main() {
    let hello = examples_dir().join("01_hello.ae");
    let out = aether()
        .arg("ast")
        .arg("--json")
        .arg("--pretty")
        .arg(&hello)
        .output()
        .expect("invoke aether ast --json --pretty");

    assert!(
        out.status.success(),
        "aether ast --json --pretty failed:\nstderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let stdout = String::from_utf8_lossy(&out.stdout);

    // Must be valid JSON
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("stdout must be valid JSON");

    // Top-level envelope fields
    assert_eq!(v["version"], "0.1", "envelope version must be \"0.1\"");
    assert!(
        v["module"].is_object(),
        "envelope must have a 'module' object"
    );

    // The hello example declares fn main — name must appear in the JSON
    assert!(
        stdout.contains("\"main\""),
        "JSON output must contain the string \"main\"; got:\n{stdout}"
    );
}

#[test]
fn ast_default_still_emits_debug_repr() {
    let hello = examples_dir().join("01_hello.ae");
    let out = aether()
        .arg("ast")
        .arg(&hello)
        .output()
        .expect("invoke aether ast (no --json)");

    assert!(out.status.success(), "aether ast failed");
    let stdout = String::from_utf8_lossy(&out.stdout);
    // Debug repr starts with "Module {" not "{"
    assert!(
        stdout.contains("Module {"),
        "default ast output should be debug repr; got:\n{stdout}"
    );
}

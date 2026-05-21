//! End-to-end test: drive the `aether` binary against every example program
//! and assert it checks + runs without errors.

use std::process::Command;

fn aether() -> Command {
    Command::new(env!("CARGO_BIN_EXE_aether"))
}

#[test]
fn check_all_examples() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("examples");
    let mut found = 0usize;
    for entry in std::fs::read_dir(&dir).expect("examples dir") {
        let path = entry.unwrap().path();
        if path.extension().and_then(|s| s.to_str()) != Some("ae") {
            continue;
        }
        found += 1;
        let out = aether()
            .arg("check")
            .arg(&path)
            .output()
            .expect("invoke aether check");
        assert!(
            out.status.success(),
            "example {} failed to check:\nstdout:\n{}\nstderr:\n{}",
            path.display(),
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    assert!(found >= 6, "expected at least 6 examples, found {found}");
}

#[test]
fn run_examples() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("examples");
    for entry in std::fs::read_dir(&dir).expect("examples dir") {
        let path = entry.unwrap().path();
        if path.extension().and_then(|s| s.to_str()) != Some("ae") {
            continue;
        }
        let out = aether()
            .arg("run")
            .arg(&path)
            .output()
            .expect("invoke aether run");
        assert!(
            out.status.success(),
            "example {} failed to run:\nstderr:\n{}",
            path.display(),
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

#[test]
fn rejects_missing_effect() {
    let tmp = tempfile_path("missing_effect.ae");
    std::fs::write(&tmp, "fn fetch(u: Str) -> Str effects {Net} { http_get(u) }\nfn main() -> Unit effects {Net,Throw} { let x = fetch(\"x\"); print(x) }").unwrap();
    let out = aether().arg("check").arg(&tmp).output().unwrap();
    assert!(!out.status.success(), "expected check to fail");
}

fn tempfile_path(name: &str) -> std::path::PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("aether_test_{}_{name}", std::process::id()));
    p
}

// --- doc E2E -----------------------------------------------------------------

#[test]
fn doc_generates_markdown() {
    let tmp = tempfile_path("doc_test.ae");
    std::fs::write(
        &tmp,
        r#"## A math module.

fn add(a: Int, b: Int) -> Int effects {} {
  a + b
}

fn mul(a: Int, b: Int) -> Int effects {} {
  a * b
}
"#,
    )
    .unwrap();

    let out = aether()
        .arg("doc")
        .arg(&tmp)
        .output()
        .expect("invoke aether doc");
    assert!(
        out.status.success(),
        "aether doc failed:\nstderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("### add"), "missing ### add section");
    assert!(stdout.contains("### mul"), "missing ### mul section");
    assert!(stdout.contains("**Signature**"), "missing Signature field");
}

#[test]
fn doc_writes_to_file() {
    let src = tempfile_path("doc_out_src.ae");
    let out_path = tempfile_path("doc_out.md");
    std::fs::write(&src, "fn greet(name: Str) -> Str effects {} { name }\n").unwrap();

    let out = aether()
        .arg("doc")
        .arg(&src)
        .arg("-o")
        .arg(&out_path)
        .output()
        .expect("invoke aether doc -o");
    assert!(out.status.success(), "aether doc -o failed");
    let md = std::fs::read_to_string(&out_path).expect("output file should exist");
    assert!(md.contains("### greet"), "output file missing ### greet");
}

// --- init E2E ----------------------------------------------------------------

#[test]
fn init_creates_project() {
    let dir = tempfile_path("init_proj");
    let out = aether()
        .arg("init")
        .arg(&dir)
        .output()
        .expect("invoke aether init");
    assert!(
        out.status.success(),
        "aether init failed:\nstderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(dir.join("Aether.toml").exists(), "Aether.toml missing");
    assert!(dir.join("main.ae").exists(), "main.ae missing");
    assert!(dir.join("README.md").exists(), "README.md missing");

    let toml = std::fs::read_to_string(dir.join("Aether.toml")).unwrap();
    assert!(toml.contains("[project]"), "missing [project]");

    let main_ae = std::fs::read_to_string(dir.join("main.ae")).unwrap();
    assert!(main_ae.contains("fn main"), "main.ae missing fn main");
}

#[test]
fn init_refuses_second_run() {
    let dir = tempfile_path("init_proj2");
    aether().arg("init").arg(&dir).output().unwrap();
    let second = aether()
        .arg("init")
        .arg(&dir)
        .output()
        .expect("invoke aether init second time");
    assert!(!second.status.success(), "second init should fail");
}

// --- repl smoke E2E ----------------------------------------------------------

#[test]
fn repl_help_flag() {
    // `aether repl --help` should succeed (clap generates it).
    let out = aether()
        .arg("repl")
        .arg("--help")
        .output()
        .expect("invoke aether repl --help");
    assert!(out.status.success(), "aether repl --help failed");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("REPL") || stdout.contains("repl") || stdout.contains("loop"),
        "help text should mention the REPL"
    );
}

// --- init template E2E -------------------------------------------------------

#[test]
fn init_bin_template() {
    let dir = tempfile_path("tpl_bin");
    let out = aether()
        .arg("init")
        .arg(&dir)
        .arg("--template")
        .arg("bin")
        .output()
        .expect("invoke aether init --template bin");
    assert!(
        out.status.success(),
        "aether init --template bin failed:\nstderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(dir.join("Aether.toml").exists(), "bin: Aether.toml missing");
    assert!(dir.join("main.ae").exists(), "bin: main.ae missing");
    assert!(!dir.join("lib.ae").exists(), "bin: lib.ae should not exist");

    let toml = std::fs::read_to_string(dir.join("Aether.toml")).unwrap();
    assert!(
        toml.contains("[project]"),
        "bin: Aether.toml missing [project]"
    );

    let main_ae = std::fs::read_to_string(dir.join("main.ae")).unwrap();
    assert!(main_ae.contains("fn main"), "bin: main.ae missing fn main");

    let gi = std::fs::read_to_string(dir.join(".gitignore")).unwrap();
    assert!(gi.contains("*.aebc"), "bin: .gitignore missing *.aebc");
}

#[test]
fn init_lib_template() {
    let dir = tempfile_path("tpl_lib");
    let out = aether()
        .arg("init")
        .arg(&dir)
        .arg("--template")
        .arg("lib")
        .output()
        .expect("invoke aether init --template lib");
    assert!(
        out.status.success(),
        "aether init --template lib failed:\nstderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(dir.join("Aether.toml").exists(), "lib: Aether.toml missing");
    assert!(dir.join("lib.ae").exists(), "lib: lib.ae missing");
    assert!(
        !dir.join("main.ae").exists(),
        "lib: main.ae should not exist"
    );

    let toml = std::fs::read_to_string(dir.join("Aether.toml")).unwrap();
    assert!(toml.contains("[project]"), "lib: missing [project]");
    assert!(
        toml.contains("[lib]"),
        "lib: Aether.toml missing [lib] section"
    );

    let lib_ae = std::fs::read_to_string(dir.join("lib.ae")).unwrap();
    assert!(
        lib_ae.contains("fn double"),
        "lib: lib.ae missing fn double"
    );
}

#[test]
fn init_agent_template() {
    let dir = tempfile_path("tpl_agent");
    let out = aether()
        .arg("init")
        .arg(&dir)
        .arg("--template")
        .arg("agent")
        .output()
        .expect("invoke aether init --template agent");
    assert!(
        out.status.success(),
        "aether init --template agent failed:\nstderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        dir.join("Aether.toml").exists(),
        "agent: Aether.toml missing"
    );
    assert!(dir.join("main.ae").exists(), "agent: main.ae missing");

    let toml = std::fs::read_to_string(dir.join("Aether.toml")).unwrap();
    assert!(
        toml.contains("[agent]"),
        "agent: Aether.toml missing [agent] section"
    );
    assert!(
        toml.contains("entry = \"main.ae\""),
        "agent: missing entry point"
    );

    let main_ae = std::fs::read_to_string(dir.join("main.ae")).unwrap();
    assert!(
        main_ae.contains("mem_get"),
        "agent: main.ae missing mem_get"
    );
    assert!(
        main_ae.contains("mem_set"),
        "agent: main.ae missing mem_set"
    );
    assert!(
        main_ae.contains("tool "),
        "agent: main.ae missing tool declaration"
    );
}

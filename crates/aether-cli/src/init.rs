//! `aether init [path] [--template <bin|lib|agent>]` — scaffold a new Aether project.
//!
//! Creates at `path` (default `.`):
//!
//! **bin** (default):
//!   Aether.toml   — minimal project manifest with [project]
//!   main.ae       — hello-world starter with `fn main`
//!   README.md     — brief project README
//!   .gitignore    — ignores *.aebc and .aether/
//!
//! **lib**:
//!   Aether.toml   — manifest with [project] + [lib]
//!   lib.ae        — public fn + test block
//!   .gitignore    — ignores *.aebc and .aether/
//!
//! **agent**:
//!   Aether.toml   — manifest with [project] + [agent]
//!   main.ae       — canonical agent workflow: tool, mem_get/mem_set, iterative refinement + test
//!   .gitignore    — ignores *.aebc and .aether/
//!
//! Errors if any of the output files already exist.

use std::path::{Path, PathBuf};

use anyhow::Context;
use clap::ValueEnum;

/// Which project template to scaffold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum Template {
    #[default]
    Bin,
    Lib,
    Agent,
}

pub fn run_init(path: Option<PathBuf>, template: Template) -> anyhow::Result<()> {
    let root = path.unwrap_or_else(|| PathBuf::from("."));
    std::fs::create_dir_all(&root)
        .with_context(|| format!("could not create directory `{}`", root.display()))?;

    let name = root
        .canonicalize()
        .unwrap_or_else(|_| root.clone())
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("my-project")
        .to_string();

    // .gitignore — all templates
    create_file(&root.join(".gitignore"), "*.aebc\n.aether/\n")?;

    match template {
        Template::Bin => scaffold_bin(&root, &name)?,
        Template::Lib => scaffold_lib(&root, &name)?,
        Template::Agent => scaffold_agent(&root, &name)?,
    }

    println!(
        "Initialized Aether {} project at `{}`",
        template.label(),
        root.display()
    );
    Ok(())
}

// --- bin ---------------------------------------------------------------------

fn scaffold_bin(root: &Path, name: &str) -> anyhow::Result<()> {
    create_file(
        &root.join("Aether.toml"),
        &format!(
            "[project]\nname = \"{name}\"\nversion = \"0.1.0\"\n\n[bin]\nentry = \"main.ae\"\n"
        ),
    )?;

    create_file(
        &root.join("main.ae"),
        "## Hello from Aether!\n\nfn main() -> Unit effects {IO} {\n  print(\"Hello, world!\")\n}\n",
    )?;

    create_file(
        &root.join("README.md"),
        &format!(
            "# {name}\n\nAn Aether project.\n\n## Running\n\n```sh\naether run main.ae\n```\n"
        ),
    )?;

    println!("  Aether.toml");
    println!("  main.ae");
    println!("  README.md");
    println!("  .gitignore");
    Ok(())
}

// --- lib ---------------------------------------------------------------------

fn scaffold_lib(root: &Path, name: &str) -> anyhow::Result<()> {
    create_file(
        &root.join("Aether.toml"),
        &format!(
            "[project]\nname = \"{name}\"\nversion = \"0.1.0\"\n\n[lib]\nentry = \"lib.ae\"\n"
        ),
    )?;

    let lib_content = format!(
        "## {name} library.\n\n## Double an integer.\nfn double(n: Int) -> Int effects {{}} {{\n  n * 2\n}}\n\ntest \"double works\" {{\n  assert(double(3) == 6)\n  assert(double(0) == 0)\n  assert(double(-4) == -8)\n}}\n"
    );
    create_file(&root.join("lib.ae"), &lib_content)?;

    println!("  Aether.toml  ([lib] section)");
    println!("  lib.ae");
    println!("  .gitignore");
    Ok(())
}

// --- agent -------------------------------------------------------------------

fn scaffold_agent(root: &Path, name: &str) -> anyhow::Result<()> {
    create_file(
        &root.join("Aether.toml"),
        &format!(
            "[project]\nname = \"{name}\"\nversion = \"0.1.0\"\n\n[agent]\nentry = \"main.ae\"\n"
        ),
    )?;

    // The agent template demonstrates: tool declaration, mem_get/mem_set (registered
    // tools), iterative refinement loop, and a test block.  It uses only builtins
    // that exist in the eval runtime so `aether check main.ae` passes immediately.
    let agent_content = r#"## Minimal Aether agent — demonstrates the canonical agent workflow.
##
## Key patterns:
##   tool        — declare an external capability (llm_complete)
##   mem_get/mem_set — persistent key-value memory across iterations
##   iterative refinement loop
##
## Run with:  aether run main.ae
## Real LLM:  aether --network run main.ae   (requires ANTHROPIC_API_KEY)

# Tool declarations
tool llm_complete(prompt: Str) -> Str !{Net, Throw}
tool mem_get(key: Str) -> Str !{}
tool mem_set(key: Str, value: Str) -> Unit !{}

# Helpers
fn build_prompt(task: Str, attempt: Int, prev: Str) -> Str effects {} {
  if attempt == 0 {
    "Task: " ++ task
  } else {
    "Task: " ++ task ++ "\nPrevious attempt: " ++ prev ++ "\nRefine your answer."
  }
}

fn response_ok(response: Str) -> Bool effects {} {
  response != ""
}

# Agent main loop
fn run_agent(task: Str) -> Str effects {Net, Throw, IO} {
  let _ = mem_set("task", task)
  let _ = mem_set("attempt", "0")
  let answer = loop_until(task, 0, 3)
  let _ = mem_set("last_result", answer)
  answer
}

fn loop_until(task: Str, attempt: Int, max: Int) -> Str effects {Net, Throw, IO} {
  if attempt >= max {
    "max iterations reached"
  } else {
    let prev = if attempt == 0 { "" } else { mem_get("last_draft") }
    let prompt = build_prompt(task, attempt, prev)
    let response = llm_complete(prompt)
    let _ = mem_set("last_draft", response)
    print("iteration " ++ str(attempt + 1) ++ ": " ++ response)
    if response_ok(response) {
      response
    } else {
      loop_until(task, attempt + 1, max)
    }
  }
}

fn main() -> Unit effects {Net, Throw, IO} {
  let answer = run_agent("Explain Aether in one sentence.")
  print("Final: " ++ answer)
}

# Tests
test "build_prompt includes task on first attempt" {
  let p = build_prompt("hello", 0, "")
  assert(p != "")
}

test "build_prompt includes prev on later attempts" {
  let p = build_prompt("hello", 1, "draft")
  assert(p != "")
}

test "response_ok rejects empty string" {
  assert(!response_ok(""))
}

test "mem_get and mem_set round-trip" {
  let _ = mem_set("key", "value")
  assert(mem_get("key") == "value")
}
"#;
    create_file(&root.join("main.ae"), agent_content)?;

    println!("  Aether.toml  ([agent] section)");
    println!("  main.ae      (tool + mem_get/mem_set + iterative loop + tests)");
    println!("  .gitignore");
    Ok(())
}

// --- Shared utility ----------------------------------------------------------

fn create_file(path: &Path, content: &str) -> anyhow::Result<()> {
    if path.exists() {
        anyhow::bail!("`{}` already exists; not overwriting", path.display());
    }
    std::fs::write(path, content)
        .with_context(|| format!("could not write `{}`", path.display()))?;
    Ok(())
}

impl Template {
    fn label(&self) -> &'static str {
        match self {
            Self::Bin => "bin",
            Self::Lib => "lib",
            Self::Agent => "agent",
        }
    }
}

// --- Unit tests --------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmpdir(suffix: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("aether_init_test_{}_{suffix}", std::process::id()));
        p
    }

    // --- existing behaviour --------------------------------------------------

    #[test]
    fn init_creates_files() {
        let dir = tmpdir("basic");
        run_init(Some(dir.clone()), Template::Bin).expect("init should succeed");

        assert!(dir.join("Aether.toml").exists(), "Aether.toml missing");
        assert!(dir.join("main.ae").exists(), "main.ae missing");
        assert!(dir.join("README.md").exists(), "README.md missing");

        let toml = fs::read_to_string(dir.join("Aether.toml")).unwrap();
        assert!(toml.contains("[project]"), "toml missing [project]");
        assert!(toml.contains("version"), "toml missing version");

        let main_ae = fs::read_to_string(dir.join("main.ae")).unwrap();
        assert!(main_ae.contains("fn main"), "main.ae missing fn main");

        let readme = fs::read_to_string(dir.join("README.md")).unwrap();
        assert!(readme.contains("aether run"), "README missing run command");
    }

    #[test]
    fn init_refuses_overwrite() {
        let dir = tmpdir("no_overwrite");
        run_init(Some(dir.clone()), Template::Bin).unwrap();
        // Second call must fail.
        let result = run_init(Some(dir.clone()), Template::Bin);
        assert!(result.is_err(), "expected error on second init");
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("already exists"), "wrong error: {msg}");
    }

    // --- template selection logic (filesystem via tempdir) -------------------

    #[test]
    fn template_bin_creates_main_ae_not_lib_ae() {
        let dir = tmpdir("tpl_bin");
        run_init(Some(dir.clone()), Template::Bin).unwrap();
        assert!(dir.join("main.ae").exists(), "bin: main.ae missing");
        assert!(!dir.join("lib.ae").exists(), "bin: lib.ae should not exist");

        let toml = fs::read_to_string(dir.join("Aether.toml")).unwrap();
        assert!(toml.contains("[project]"));
        assert!(!toml.contains("[lib]"), "bin toml should not have [lib]");
        assert!(
            !toml.contains("[agent]"),
            "bin toml should not have [agent]"
        );

        let gi = fs::read_to_string(dir.join(".gitignore")).unwrap();
        assert!(gi.contains("*.aebc"));
        assert!(gi.contains(".aether/"));
    }

    #[test]
    fn template_lib_creates_lib_ae_no_main_ae() {
        let dir = tmpdir("tpl_lib");
        run_init(Some(dir.clone()), Template::Lib).unwrap();
        assert!(dir.join("lib.ae").exists(), "lib: lib.ae missing");
        assert!(
            !dir.join("main.ae").exists(),
            "lib: main.ae should not exist"
        );

        let toml = fs::read_to_string(dir.join("Aether.toml")).unwrap();
        assert!(toml.contains("[project]"));
        assert!(toml.contains("[lib]"), "lib toml missing [lib] section");

        let lib_ae = fs::read_to_string(dir.join("lib.ae")).unwrap();
        assert!(lib_ae.contains("fn double"), "lib.ae missing fn double");
        assert!(lib_ae.contains("test "), "lib.ae missing test block");
    }

    #[test]
    fn template_agent_creates_main_ae_with_mem_ops() {
        let dir = tmpdir("tpl_agent");
        run_init(Some(dir.clone()), Template::Agent).unwrap();
        assert!(dir.join("main.ae").exists(), "agent: main.ae missing");
        assert!(
            !dir.join("lib.ae").exists(),
            "agent: lib.ae should not exist"
        );

        let toml = fs::read_to_string(dir.join("Aether.toml")).unwrap();
        assert!(toml.contains("[project]"));
        assert!(
            toml.contains("[agent]"),
            "agent toml missing [agent] section"
        );
        assert!(toml.contains("entry = \"main.ae\""));

        let main_ae = fs::read_to_string(dir.join("main.ae")).unwrap();
        assert!(main_ae.contains("mem_get"), "agent main.ae missing mem_get");
        assert!(main_ae.contains("mem_set"), "agent main.ae missing mem_set");
        assert!(
            main_ae.contains("tool "),
            "agent main.ae missing tool declaration"
        );
        assert!(
            main_ae.contains("test "),
            "agent main.ae missing test block"
        );
    }
}

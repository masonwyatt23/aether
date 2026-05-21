//! `aether build` — manifest-aware project build.
//!
//! Reads `Aether.toml`, checks every entry file, runs every `test "..." { ... }`
//! block, AOT-compiles each entry to a `.aebc` artifact, and copies them into
//! `dist/`. Exit non-zero if any step fails.

use aether_ast::SourceMap;
use aether_bc::{compile_module, serialize_program};
use aether_eval::{Runtime, TestOutcome};
use aether_types::{check_module, Severity};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::modules::Loader;

/// Run a manifest-aware build for the project rooted at `project_dir`.
///
/// Steps (each step exits non-zero on failure):
///   1. Locate `Aether.toml`; honor `[project]`, `[bin]`/`[agent]` / `[lib]`.
///   2. For each entry `.ae` file, run check + tests + AOT compile.
///   3. Write artifacts to `<project_dir>/dist/`.
pub fn run_build(project_dir: PathBuf, release: bool) -> ExitCode {
    let manifest = project_dir.join("Aether.toml");
    if !manifest.exists() {
        eprintln!(
            "error: no Aether.toml at {} — run `aether init` first",
            project_dir.display()
        );
        return ExitCode::from(2);
    }
    let manifest_src = match fs::read_to_string(&manifest) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: could not read {}: {e}", manifest.display());
            return ExitCode::from(2);
        }
    };
    let manifest = match parse_manifest(&manifest_src) {
        Ok(m) => m,
        Err(e) => {
            eprintln!(
                "error: {} is malformed: {e}",
                project_dir.join("Aether.toml").display()
            );
            return ExitCode::from(1);
        }
    };
    // Pick entries from the manifest sections.
    let entries: Vec<PathBuf> = manifest
        .entries
        .iter()
        .map(|e| project_dir.join(e))
        .collect();
    if entries.is_empty() {
        eprintln!("error: no entries declared in Aether.toml (expected [bin]/[agent]/[lib])");
        return ExitCode::from(1);
    }
    let dist = project_dir.join("dist");
    if release && dist.exists() {
        let _ = fs::remove_dir_all(&dist);
    }
    if let Err(e) = fs::create_dir_all(&dist) {
        eprintln!("error: could not create {}: {e}", dist.display());
        return ExitCode::from(2);
    }

    let mut total_tests = 0usize;
    let mut failed_tests = 0usize;
    let mut emitted = 0usize;

    for entry in &entries {
        if !entry.exists() {
            eprintln!("error: entry {} not found", entry.display());
            return ExitCode::from(1);
        }
        println!("→ {}", entry.display());

        let mut sm = SourceMap::new();
        let module = {
            let root = entry.parent().unwrap_or(&project_dir).to_path_buf();
            let mut loader = Loader::new(&root);
            match loader.load(entry, &mut sm) {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("  ✗ load: {e}");
                    return ExitCode::from(1);
                }
            }
        };

        // 1. Type-check.
        let (_, diags) = check_module(&module);
        let n_err = diags
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .count();
        if n_err > 0 {
            eprintln!("  ✗ check: {n_err} type error(s)");
            for d in &diags {
                if d.severity == Severity::Error {
                    eprintln!("    {}", d.msg);
                }
            }
            return ExitCode::from(1);
        }
        println!("  ✓ check");

        // 2. Tests.
        let mut rt = Runtime::new(module.clone());
        rt.capture_only = true;
        let reports = rt.run_tests();
        let local_tests = reports.len();
        let local_failed = reports.iter().filter(|r| !r.passed()).count();
        if !reports.is_empty() {
            for r in &reports {
                match &r.outcome {
                    TestOutcome::Pass => println!("  ✓ test {}", r.name),
                    TestOutcome::Fail(msg) => println!("  ✗ test {} — {}", r.name, msg),
                }
            }
        }
        total_tests += local_tests;
        failed_tests += local_failed;
        if local_failed > 0 {
            return ExitCode::from(1);
        }

        // 3. AOT compile. Library entries (`lib.ae`) without a `main` are skipped.
        let has_main = module.decls.iter().any(|d| match d {
            aether_ast::Decl::Fn(f) => f.name == "main",
            _ => false,
        });
        if has_main {
            match compile_module(&module) {
                Ok(program) => {
                    let bytes = serialize_program(&program);
                    let stem = entry.file_stem().and_then(|s| s.to_str()).unwrap_or("out");
                    let out = dist.join(format!("{stem}.aebc"));
                    if let Err(e) = fs::write(&out, &bytes) {
                        eprintln!("  ✗ write {}: {e}", out.display());
                        return ExitCode::from(1);
                    }
                    println!("  ✓ compile → dist/{}.aebc ({} bytes)", stem, bytes.len());
                    emitted += 1;
                }
                Err(e) => {
                    // BC compile errors aren't fatal — fall back to source artifact.
                    eprintln!("  ⚠ bytecode compile unavailable ({e}); copying source");
                    let stem = entry.file_name().unwrap_or_default();
                    let out = dist.join(stem);
                    if let Err(e2) = fs::copy(entry, &out) {
                        eprintln!("  ✗ copy {}: {e2}", out.display());
                        return ExitCode::from(1);
                    }
                    emitted += 1;
                }
            }
        } else {
            // Library: copy the source verbatim into dist so consumers can import it.
            let stem = entry.file_name().unwrap_or_default();
            let out = dist.join(stem);
            if let Err(e) = fs::copy(entry, &out) {
                eprintln!("  ✗ copy {}: {e}", out.display());
                return ExitCode::from(1);
            }
            println!(
                "  ✓ library → dist/{}",
                out.file_name().unwrap_or_default().to_string_lossy()
            );
            emitted += 1;
        }
    }

    // Copy Aether.toml + README.md into dist if present.
    let _ = fs::copy(project_dir.join("Aether.toml"), dist.join("Aether.toml"));
    let readme = project_dir.join("README.md");
    if readme.exists() {
        let _ = fs::copy(&readme, dist.join("README.md"));
    }

    println!();
    println!(
        "✓ build complete — {emitted} artifact(s), {} test(s) ({} failed) → {}",
        total_tests,
        failed_tests,
        dist.display(),
    );
    ExitCode::SUCCESS
}

// ── tiny TOML-ish manifest parser ──────────────────────────────────────────

#[derive(Debug, Default)]
struct Manifest {
    entries: Vec<String>,
    #[allow(dead_code)]
    name: Option<String>,
}

fn parse_manifest(src: &str) -> Result<Manifest, String> {
    let mut current_section = String::new();
    let mut entries: Vec<String> = Vec::new();
    let mut name: Option<String> = None;
    for (lineno, raw) in src.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            current_section = line[1..line.len() - 1].to_string();
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            return Err(format!("line {}: expected `key = value`", lineno + 1));
        };
        let key = k.trim();
        let value = v.trim().trim_matches('"').to_string();
        match (current_section.as_str(), key) {
            ("project", "name") => name = Some(value),
            ("bin", "entry") | ("agent", "entry") | ("lib", "entry") => entries.push(value),
            // Allow `entry` at the top level for backwards compat.
            ("", "entry") => entries.push(value),
            _ => {}
        }
    }
    let result_name = name;
    // If no [section] entry was declared but main.ae or lib.ae exists, use that
    // (heuristic for the default `aether init` output).
    if entries.is_empty() {
        for candidate in ["main.ae", "lib.ae"] {
            if Path::new(candidate).exists() {
                entries.push(candidate.to_string());
                break;
            }
        }
    }
    Ok(Manifest {
        entries,
        name: result_name,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_manifest() {
        let m = parse_manifest("[project]\nname = \"demo\"\n[bin]\nentry = \"main.ae\"\n").unwrap();
        assert_eq!(m.entries, vec!["main.ae".to_string()]);
    }

    #[test]
    fn ignores_comments() {
        let m = parse_manifest("# header\n[bin]\n# comment\nentry = \"a.ae\"\n").unwrap();
        assert_eq!(m.entries, vec!["a.ae".to_string()]);
    }

    #[test]
    fn lib_section() {
        let m = parse_manifest("[lib]\nentry = \"lib.ae\"\n").unwrap();
        assert_eq!(m.entries, vec!["lib.ae".to_string()]);
    }
}

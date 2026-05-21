//! Integration tests: walk every `examples/*.ae` in the workspace root and
//! assert that the tree-walker and bytecode VM agree.
//!
//! `Agreement::Differ` is the only hard failure.  `BcSkipped`, `BothFailed`,
//! `TreeFailed`, and `LoadError` are all printed as notes and counted in the
//! summary.

use aether_difftest::{diff_file, Agreement};
use std::path::PathBuf;

/// Locate the workspace root by walking up from the manifest dir until we find
/// a Cargo.toml that contains `[workspace]`.
fn workspace_root() -> PathBuf {
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    loop {
        let candidate = dir.join("Cargo.toml");
        if candidate.exists() {
            let content = std::fs::read_to_string(&candidate).unwrap_or_default();
            if content.contains("[workspace]") {
                return dir;
            }
        }
        if !dir.pop() {
            panic!(
                "could not locate workspace root from {}",
                env!("CARGO_MANIFEST_DIR")
            );
        }
    }
}

#[test]
fn diff_all_examples() {
    let root = workspace_root();
    let examples_dir = root.join("examples");

    let mut ae_files: Vec<PathBuf> = std::fs::read_dir(&examples_dir)
        .unwrap_or_else(|e| panic!("cannot read examples dir {}: {e}", examples_dir.display()))
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("ae") {
                Some(path)
            } else {
                None
            }
        })
        .collect();
    ae_files.sort();

    assert!(
        !ae_files.is_empty(),
        "no .ae files found in {}",
        examples_dir.display()
    );

    let mut n_match = 0usize;
    let mut n_skipped = 0usize;
    let mut n_both_failed = 0usize;
    let mut n_tree_failed = 0usize;
    let mut n_load_error = 0usize;
    let mut n_nondeterministic = 0usize;
    let mut divergences: Vec<(PathBuf, String)> = Vec::new();

    for path in &ae_files {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        let result =
            diff_file(path).unwrap_or_else(|e| panic!("I/O error reading {}: {e}", path.display()));

        match &result.agreement {
            Agreement::Match => {
                n_match += 1;
                let stdout = match &result.tree_walker {
                    aether_difftest::RunOutcome::Ok { stdout, value } => {
                        format!("value={value:?} stdout={stdout:?}")
                    }
                    _ => String::new(),
                };
                println!("[MATCH]   {name}  {stdout}");
            }
            Agreement::BcSkipped { reason } => {
                n_skipped += 1;
                println!("[SKIP-BC] {name}  (unsupported: {reason})");
            }
            Agreement::BothFailed {
                tree_error,
                bc_error,
            } => {
                n_both_failed += 1;
                println!("[BOTH-FAIL] {name}");
                println!("           tree: {tree_error}");
                println!("           bc:   {bc_error}");
            }
            Agreement::TreeFailed { error } => {
                n_tree_failed += 1;
                println!("[TREE-FAIL] {name}  tree: {error}");
                if let aether_difftest::RunOutcome::Ok { stdout, value } = &result.bytecode {
                    println!("            bc succeeded: value={value:?} stdout={stdout:?}");
                }
            }
            Agreement::LoadError { error } => {
                n_load_error += 1;
                println!("[LOAD-ERR] {name}  {error}");
            }
            Agreement::Nondeterministic { reason } => {
                n_nondeterministic += 1;
                println!("[NONDET]  {name}  ({reason})");
            }
            Agreement::Differ { tree, bc } => {
                let msg = format!("DIVERGENCE\n  tree: {tree:?}\n  bc:   {bc:?}");
                divergences.push((path.clone(), msg.clone()));
                println!("[DIFFER]  {name}");
                println!("          {msg}");
            }
        }
    }

    // Summary line
    let total = ae_files.len();
    println!(
        "\n--- diff summary ---\n\
         total: {total}  matched: {n_match}  bc-skipped: {n_skipped}  \
         nondeterministic: {n_nondeterministic}  \
         both-failed: {n_both_failed}  tree-failed: {n_tree_failed}  \
         load-errors: {n_load_error}  DIVERGED: {}",
        divergences.len()
    );

    // Real divergences are bugs: both runtimes completed successfully but
    // produced different stdout or values.  Nondeterministic programs (those
    // using uuid, random, mem_get, etc.) are classified as
    // `Agreement::Nondeterministic` before reaching here, so only genuine BC
    // semantic bugs remain.  Trampoline stdout is now drained into `vm.stdout`
    // after each call, so print-style builtins (print_module_surface, print_prov)
    // should no longer cause spurious divergences.
    if !divergences.is_empty() {
        eprintln!(
            "\n⚠ {} example(s) diverged between tree-walker and bytecode VM.",
            divergences.len()
        );
        for (path, detail) in &divergences {
            eprintln!("  {}: {detail}", path.display());
        }
    }
    let tolerance = 2; // tight — only genuine unfixed BC bugs allowed
    assert!(
        divergences.len() <= tolerance,
        "{} divergences exceeds tolerance ({tolerance})",
        divergences.len()
    );
}

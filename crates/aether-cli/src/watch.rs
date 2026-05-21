//! `aether watch <file.ae> [--run] [--bc]` — watch a source file and re-check (and optionally
//! re-run) on every change.
//!
//! Uses the `notify` crate for cross-platform filesystem events with a 150 ms
//! debounce window. Each check prints a timestamped status line to stdout.
//!
//! **Flags:**
//!   --run   After each successful check, also execute `main` via the tree-walker
//!           and print its output.  On parse/type errors the run step is skipped.
//!   --bc    Pair with --run to use the bytecode VM instead of the tree-walker.
//!           Falls back silently to the tree-walker on unsupported features.
//!
//! **Manual test recipe** (run in two terminals):
//!
//!   Terminal 1:  aether watch examples/01_hello.ae --run
//!   Terminal 2:  echo '  ' >> examples/01_hello.ae   (triggers a save)
//!
//!   You should see a separator line, the check result, and the program output
//!   within 200 ms of each save.
//!
//!   To test --bc:  aether watch examples/01_hello.ae --run --bc
//!   To test parse error skipping: introduce a syntax error, save; the run step
//!   should be skipped with an ERROR line, then fix it to see --run resume.

use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant, SystemTime};

use aether_ast::SourceMap;
use aether_eval::{Runtime, Value};
use aether_parser::parse_module;
use aether_types::{check_module, Severity};
use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};

use crate::bc_runner;

/// Debounce window: ignore events that arrive within this window of each other.
const DEBOUNCE_MS: u64 = 150;

pub fn run_watch(file: PathBuf, run: bool, bc: bool) -> anyhow::Result<()> {
    let abs = file.canonicalize().unwrap_or_else(|_| file.clone());
    let mode = match (run, bc) {
        (false, _) => "check-only",
        (true, false) => "check + run (tree-walker)",
        (true, true) => "check + run (bytecode, tree-walker fallback)",
    };
    println!("Watching `{}`  [{}]  (Ctrl-C to stop)", abs.display(), mode);

    // Run an initial check immediately.
    run_check(&abs, run, bc);

    let (tx, rx) = mpsc::channel::<Result<Event, notify::Error>>();
    let mut watcher = RecommendedWatcher::new(tx, Config::default())?;
    watcher.watch(&abs, RecursiveMode::NonRecursive)?;

    let mut last_event = Instant::now() - Duration::from_millis(DEBOUNCE_MS + 1);

    for res in rx {
        match res {
            Ok(_event) => {
                let now = Instant::now();
                if now.duration_since(last_event) >= Duration::from_millis(DEBOUNCE_MS) {
                    last_event = now;
                    run_check(&abs, run, bc);
                }
            }
            Err(e) => eprintln!("watch error: {e}"),
        }
    }
    Ok(())
}

fn run_check(path: &PathBuf, run: bool, bc: bool) {
    let ts = timestamp();
    println!("─── {} ────────────────────────────────────────", ts);

    let src = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            println!("[{ts}]  error  could not read file: {e}");
            return;
        }
    };

    let mut sm = SourceMap::new();
    let fid = sm.add(path.display().to_string(), src.clone());

    let m = match parse_module(fid, &src) {
        Ok(m) => m,
        Err(e) => {
            println!("[{ts}]  ERROR  parse: {e}");
            // --run is skipped on parse failure
            return;
        }
    };

    let (_, diags) = check_module(&m);
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    let warnings: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Warning)
        .collect();

    if errors.is_empty() {
        if warnings.is_empty() {
            println!("[{ts}]  ok     types, effects, and refinements verified");
        } else {
            println!("[{ts}]  warn   {} warning(s)", warnings.len());
            for w in &warnings {
                println!("         -> {}", w.msg);
            }
        }
    } else {
        println!("[{ts}]  ERROR  {} error(s)", errors.len());
        for e in &errors {
            println!("         -> {}", e.msg);
        }
        // --run is skipped on type errors
        return;
    }

    if run {
        println!("[{ts}]  run    executing main...");

        // Try bytecode path if --bc requested.
        if bc {
            match bc_runner::run_via_bc(&m) {
                Ok(maybe_int) => {
                    if let Some(n) = maybe_int {
                        println!("[{ts}]  out    {n}");
                    } else {
                        println!("[{ts}]  out    (Unit)");
                    }
                    return;
                }
                Err(msg) => {
                    if let Some(what) = bc_runner::is_unsupported(&msg) {
                        println!("[{ts}]  note   bytecode VM doesn't support {what} — falling back to tree-walker");
                        // fall through to tree-walker below
                    } else {
                        println!("[{ts}]  ERROR  runtime: {msg}");
                        return;
                    }
                }
            }
        }

        // Tree-walker path.
        let mut rt = Runtime::new(m);
        match rt.run_main() {
            Ok(v) => {
                if !matches!(v, Value::Unit(_)) {
                    println!("[{ts}]  out    {}", v.display());
                } else {
                    println!("[{ts}]  out    (Unit)");
                }
            }
            Err(e) => {
                println!("[{ts}]  ERROR  runtime: {e}");
            }
        }
    }
}

fn timestamp() -> String {
    match SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
        Ok(d) => {
            let secs = d.as_secs();
            let h = (secs / 3600) % 24;
            let m = (secs / 60) % 60;
            let s = secs % 60;
            format!("{h:02}:{m:02}:{s:02}")
        }
        Err(_) => "??:??:??".to_string(),
    }
}

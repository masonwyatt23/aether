//! `aether` — command-line driver.
//!
//! Subcommands:
//!   check   — parse + type-check + effect-check + refinement-verify.
//!   run     — check (warnings only) then evaluate `main`.
//!   fmt     — project compact↔verbose.
//!   explain — pretty-print the `introspect()` surface of a module.
//!   ast     — show the canonical AST as debug repr.
//!   repl    — interactive read-eval-print loop.
//!   watch   — watch a file and re-check on every change.
//!   doc     — emit Markdown documentation from a module.
//!   init    — scaffold a new Aether project.
//!   compile — compile a source file to a `.aebc` bytecode file.
//!   exec    — execute a pre-compiled `.aebc` bytecode file.

mod ast_json;
mod bc_runner;
mod build;
mod doc;
mod docgen;
mod init;
mod lint;
mod modules;
mod repl;
mod verify;
mod watch;

use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use aether_ast::{FileId, SourceMap};
use aether_eval::{Runtime, SnapStatus, Value};
use aether_parser::parse_module;
use aether_parser::pretty::{module as pp_module, Form};
use aether_types::{check_module, Diagnostic, Severity};
use ariadne::{Color, Label, Report, ReportKind};
use clap::{Parser as ClapParser, Subcommand};

#[derive(ClapParser)]
#[command(
    name = "aether",
    about = "The Aether programming language — for agents, by agents.\n\nUse --network to enable real HTTP (http_get) and LLM (llm_complete) tools.\nWithout --network, deterministic stubs are used so tests are reproducible.\nLLM calls require ANTHROPIC_API_KEY; model can be overridden via AETHER_LLM_MODEL.",
    version
)]
struct Cli {
    /// Enable real network tools (http_get, llm_complete).
    /// Requires ANTHROPIC_API_KEY for LLM calls.
    /// Without this flag, deterministic stubs are used (safe for tests).
    #[arg(long, global = true)]
    network: bool,

    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Type-check, effect-check, and refinement-verify a program.
    Check {
        file: PathBuf,
        /// Treat solver "Unknown" warnings as errors.
        #[arg(long)]
        strict: bool,
        /// Skip import resolution (treat the file as self-contained).
        #[arg(long)]
        no_imports: bool,
        /// Emit diagnostics as JSON instead of pretty-printed.
        #[arg(long)]
        json: bool,
    },
    /// Run a program (executes `main`).
    Run {
        file: PathBuf,
        /// Skip the type checker before running.
        #[arg(long)]
        no_check: bool,
        /// Skip import resolution (treat the file as self-contained).
        #[arg(long)]
        no_imports: bool,
        /// Use the bytecode VM (falls back to tree-walker for unsupported features).
        #[arg(long)]
        bc: bool,
    },
    /// Format a program; project to compact or verbose form.
    Fmt {
        file: PathBuf,
        /// Output verbose form (default is compact).
        #[arg(long, conflicts_with = "compact")]
        verbose: bool,
        /// Output compact form (default).
        #[arg(long)]
        compact: bool,
        /// Exit non-zero if the file differs from the formatted form (no output).
        #[arg(long)]
        check: bool,
    },
    /// Run static lint checks (over-broad effects, unused lets, missing docstrings, …).
    Lint {
        file: PathBuf,
        /// Skip import resolution.
        #[arg(long)]
        no_imports: bool,
        /// Exit non-zero on any warning.
        #[arg(long)]
        deny_warnings: bool,
    },
    /// Build a manifest-driven project: check, test, AOT compile every entry,
    /// then write artifacts to `dist/`.
    Build {
        /// Project directory containing Aether.toml (default: current directory).
        path: Option<PathBuf>,
        /// Wipe `dist/` before building.
        #[arg(long)]
        release: bool,
    },
    /// Generate a static HTML doc site from every `.ae` file under a project root.
    Docgen {
        /// Project directory to crawl (default: current directory).
        path: Option<PathBuf>,
        /// Output directory.
        #[arg(short, long, default_value = "docs-site")]
        out: PathBuf,
        /// Site title.
        #[arg(long)]
        title: Option<String>,
    },
    /// Pretty-print the module surface (uses `introspect`).
    Explain { file: PathBuf },
    /// Show the canonical AST as JSON-ish debug repr (for tooling).
    Ast {
        file: PathBuf,
        /// Emit machine-readable JSON instead of the Rust debug repr.
        #[arg(long)]
        json: bool,
        /// Pretty-print the JSON output (implies --json).
        #[arg(long)]
        pretty: bool,
    },
    /// Start an interactive read-eval-print loop.
    ///
    /// Special commands inside the REPL:
    ///   :q            quit
    ///   :t <expr>     show type
    ///   :d <expr>     show AST debug repr
    ///   :p <expr>     show provenance chain
    ///   :l <file.ae>  load file into session
    ///   :h            help
    Repl,
    /// Watch a file and re-check it on every change (150 ms debounce).
    Watch {
        /// The Aether source file to watch.
        file: PathBuf,
        /// After each successful check, also run `main` and print its output.
        #[arg(long)]
        run: bool,
        /// Use the bytecode VM for --run (falls back to tree-walker for unsupported features).
        #[arg(long)]
        bc: bool,
    },
    /// Generate Markdown documentation from an Aether source file.
    Doc {
        /// The Aether source file to document.
        file: PathBuf,
        /// Write output to a file instead of stdout.
        #[arg(short, long, value_name = "FILE")]
        output: Option<PathBuf>,
    },
    /// Scaffold a new Aether project at the given path (default: current directory).
    Init {
        /// Directory to initialise. Created if it does not exist.
        path: Option<PathBuf>,
        /// Project template: bin (default), lib, or agent.
        #[arg(long, default_value = "bin")]
        template: init::Template,
    },
    /// Run every `test "..." { ... }` block in an Aether source file.
    Test {
        file: PathBuf,
        /// Skip import resolution (treat the file as self-contained).
        #[arg(long)]
        no_imports: bool,
    },
    /// Run every `bench "..." { ... }` block and report mean iteration time.
    Bench {
        file: PathBuf,
        /// Iterations per benchmark.
        #[arg(long, default_value_t = 100)]
        iters: u32,
        /// Skip import resolution.
        #[arg(long)]
        no_imports: bool,
    },
    /// Run every `snap "..." { ... }` block and compare against golden `.snap` file.
    ///
    /// On first run (no `.snap` file found) values are captured and written.
    /// On subsequent runs values are compared; any mismatch exits with code 1.
    /// Pass `--update` to overwrite the golden file with new values (bless mode).
    Snap {
        file: PathBuf,
        /// Overwrite the golden `.snap` file with current values.
        #[arg(long)]
        update: bool,
        /// Skip import resolution (treat the file as self-contained).
        #[arg(long)]
        no_imports: bool,
    },
    /// Compile a source file to a `.aebc` bytecode file.
    Compile {
        file: PathBuf,
        /// Output path (default: same name with `.aebc` extension).
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Skip import resolution (treat the file as self-contained).
        #[arg(long)]
        no_imports: bool,
    },
    /// Strict verification gate: exit 0 only if every contract is proved and
    /// every effect is sound — for use in CI or an agent loop.
    Verify {
        file: PathBuf,
        /// Skip import resolution (treat the file as self-contained).
        #[arg(long)]
        no_imports: bool,
        /// Emit a machine-readable JSON verdict instead of pretty-printed output.
        #[arg(long)]
        json: bool,
    },
    /// Execute a pre-compiled `.aebc` bytecode file (skips parse/typecheck).
    Exec { file: PathBuf },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let network = cli.network;
    match cli.cmd {
        Cmd::Check {
            file,
            strict,
            no_imports,
            json,
        } => cmd_check(file, strict, no_imports, json),
        Cmd::Lint {
            file,
            no_imports,
            deny_warnings,
        } => cmd_lint(file, no_imports, deny_warnings),
        Cmd::Build { path, release } => build::run_build(
            path.unwrap_or_else(|| std::path::PathBuf::from(".")),
            release,
        ),
        Cmd::Docgen { path, out, title } => docgen::run_docgen(
            path.unwrap_or_else(|| std::path::PathBuf::from(".")),
            out,
            title,
        ),
        Cmd::Run {
            file,
            no_check,
            no_imports,
            bc,
        } => cmd_run(file, no_check, no_imports, network, bc),
        Cmd::Fmt {
            file,
            verbose,
            compact: _,
            check,
        } => cmd_fmt(file, verbose, check),
        Cmd::Explain { file } => cmd_explain(file),
        Cmd::Ast { file, json, pretty } => cmd_ast(file, json || pretty, pretty),
        Cmd::Repl => {
            let version = env!("CARGO_PKG_VERSION");
            // For the REPL, signal network mode via env var since the Engine
            // creates Runtime instances internally in feed().
            if network {
                std::env::set_var("AETHER_NETWORK", "1");
            }
            match repl::run_interactive(version) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("repl error: {e}");
                    ExitCode::from(1)
                }
            }
        }
        Cmd::Watch { file, run, bc } => match watch::run_watch(file, run, bc) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("watch error: {e}");
                ExitCode::from(1)
            }
        },
        Cmd::Doc { file, output } => match doc::run_doc(file, output) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("{e}");
                ExitCode::from(1)
            }
        },
        Cmd::Init { path, template } => match init::run_init(path, template) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("init error: {e}");
                ExitCode::from(1)
            }
        },
        Cmd::Test { file, no_imports } => cmd_test(file, no_imports, network),
        Cmd::Bench {
            file,
            iters,
            no_imports,
        } => cmd_bench(file, iters, no_imports, network),
        Cmd::Snap {
            file,
            update,
            no_imports,
        } => cmd_snap(file, update, no_imports, network),
        Cmd::Compile {
            file,
            output,
            no_imports,
        } => cmd_compile(file, output, no_imports),
        Cmd::Verify {
            file,
            no_imports,
            json,
        } => verify::run_verify(file, no_imports, json),
        Cmd::Exec { file } => cmd_exec(file),
    }
}

fn cmd_test(file: PathBuf, no_imports: bool, network: bool) -> ExitCode {
    let mut sm = SourceMap::new();
    let m = match resolve_module(&file, no_imports, &mut sm) {
        Ok(m) => m,
        Err(code) => return code,
    };
    // Static checks first so type errors don't get masked as runtime failures.
    let (_, diags) = check_module(&m);
    let n_errors = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .count();
    for d in &diags {
        if d.severity == Severity::Error {
            print_diagnostic(&sm, d);
        }
    }
    if n_errors > 0 {
        eprintln!("✗ refusing to run tests: {n_errors} type error(s)");
        return ExitCode::from(1);
    }
    let mut rt = Runtime::new(m);
    if network {
        aether_tools_net::install(&mut rt.tools);
    }
    rt.capture_only = true;
    let reports = rt.run_tests();
    if reports.is_empty() {
        eprintln!("no tests found in {}", file.display());
        return ExitCode::SUCCESS;
    }
    let mut failed = 0usize;
    for r in &reports {
        match &r.outcome {
            aether_eval::TestOutcome::Pass => println!("  ✓ {}", r.name),
            aether_eval::TestOutcome::Fail(msg) => {
                println!("  ✗ {}  — {}", r.name, msg);
                failed += 1;
            }
        }
    }
    let total = reports.len();
    let passed = total - failed;
    println!();
    if failed == 0 {
        println!("✓ {total} test(s) passed");
        ExitCode::SUCCESS
    } else {
        println!("✗ {failed} of {total} tests failed ({passed} passed)");
        ExitCode::from(1)
    }
}

fn cmd_snap(file: PathBuf, update: bool, no_imports: bool, network: bool) -> ExitCode {
    let mut sm = SourceMap::new();
    let m = match resolve_module(&file, no_imports, &mut sm) {
        Ok(m) => m,
        Err(code) => return code,
    };
    let (_, diags) = check_module(&m);
    let n_errors = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .count();
    for d in &diags {
        if d.severity == Severity::Error {
            print_diagnostic(&sm, d);
        }
    }
    if n_errors > 0 {
        eprintln!("refusing to run snapshots: {n_errors} type error(s)");
        return ExitCode::from(1);
    }
    let mut rt = Runtime::new(m);
    if network {
        aether_tools_net::install(&mut rt.tools);
    }
    rt.capture_only = true;
    let reports = rt.run_snapshots(&file, update);
    if reports.is_empty() {
        eprintln!(
            "no `snap \"...\" {{ ... }}` blocks found in {}",
            file.display()
        );
        return ExitCode::SUCCESS;
    }
    let mut failed = 0usize;
    for r in &reports {
        match &r.status {
            SnapStatus::Pass => println!("  ✓ {} — ok", r.name),
            SnapStatus::Captured => println!("  + {} — captured (new)", r.name),
            SnapStatus::Mismatch(mismatches) => {
                println!("  ✗ {} — {} mismatch(es)", r.name, mismatches.len());
                for m in mismatches {
                    println!("      label:    {}", m.label);
                    println!("      expected: {}", m.expected);
                    println!("      got:      {}", m.got);
                }
                failed += 1;
            }
            SnapStatus::Error(msg) => {
                println!("  ✗ {} — error: {}", r.name, msg);
                failed += 1;
            }
        }
    }
    let total = reports.len();
    let passed = total - failed;
    println!();
    if failed == 0 {
        if update {
            println!("✓ {total} snapshot(s) updated");
        } else {
            println!("✓ {total} snapshot(s) passed ({passed} ok)");
        }
        ExitCode::SUCCESS
    } else {
        println!("✗ {failed} of {total} snapshots failed ({passed} passed)");
        ExitCode::from(1)
    }
}

fn cmd_bench(file: PathBuf, iters: u32, no_imports: bool, network: bool) -> ExitCode {
    let mut sm = SourceMap::new();
    let m = match resolve_module(&file, no_imports, &mut sm) {
        Ok(m) => m,
        Err(code) => return code,
    };
    let (_, diags) = check_module(&m);
    let n_errors = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .count();
    for d in &diags {
        if d.severity == Severity::Error {
            print_diagnostic(&sm, d);
        }
    }
    if n_errors > 0 {
        eprintln!("✗ refusing to run benches: {n_errors} type error(s)");
        return ExitCode::from(1);
    }
    let mut rt = Runtime::new(m);
    if network {
        aether_tools_net::install(&mut rt.tools);
    }
    rt.capture_only = true;
    let reports = rt.run_benches(iters);
    if reports.is_empty() {
        eprintln!(
            "no `bench \"...\" {{ ... }}` blocks found in {}",
            file.display()
        );
        return ExitCode::SUCCESS;
    }
    let mut failed = 0usize;
    let name_width = reports.iter().map(|r| r.name.len()).max().unwrap_or(0);
    for r in &reports {
        match &r.outcome {
            aether_eval::TestOutcome::Pass => {
                println!(
                    "  {:<width$}  {} iters   {:>10.2} µs/iter   ({:.2} ms total)",
                    r.name,
                    r.iters,
                    r.mean_us,
                    r.total_us as f64 / 1000.0,
                    width = name_width,
                );
            }
            aether_eval::TestOutcome::Fail(msg) => {
                println!("  ✗ {}  — {}", r.name, msg);
                failed += 1;
            }
        }
    }
    let total = reports.len();
    let passed = total - failed;
    println!();
    if failed == 0 {
        println!("✓ {total} bench(es) completed");
        ExitCode::SUCCESS
    } else {
        println!("✗ {failed} of {total} benches failed ({passed} passed)");
        ExitCode::from(1)
    }
}

fn read(file: &PathBuf) -> Option<String> {
    fs::read_to_string(file)
        .map_err(|e| eprintln!("error: could not read {}: {e}", file.display()))
        .ok()
}

/// Resolve a module for a given file path.
///
/// When `no_imports` is true (or the file contains no `import ` substring) we
/// skip the loader and fall back to a plain `parse_module` call so the existing
/// examples that have no imports continue to work exactly as before.
pub(crate) fn resolve_module(
    file: &PathBuf,
    no_imports: bool,
    sm: &mut SourceMap,
) -> Result<aether_ast::Module, ExitCode> {
    let Some(src) = read(file) else {
        return Err(ExitCode::from(2));
    };

    // Fast-path: skip loader when imports aren't needed.
    if no_imports || !src.contains("import ") {
        let fid = sm.add(file.display().to_string(), src.clone());
        return parse_module(fid, &src).map_err(|e| {
            // fid is already registered in sm under the same name; re-use it.
            print_parse_error(sm, fid, &e);
            ExitCode::from(1)
        });
    }

    let root = file
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .to_path_buf();
    let mut loader = modules::Loader::new(&root);
    loader.load(file, sm).map_err(|e| {
        eprintln!("error: {e}");
        ExitCode::from(1)
    })
}

fn cmd_check(file: PathBuf, strict: bool, no_imports: bool, json: bool) -> ExitCode {
    let mut sm = SourceMap::new();
    let m = match resolve_module(&file, no_imports, &mut sm) {
        Ok(m) => m,
        Err(code) => return code,
    };
    let (_, diags) = check_module(&m);
    let n_errors = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .count();
    let n_warnings = diags
        .iter()
        .filter(|d| d.severity == Severity::Warning)
        .count();
    if json {
        print_json_diagnostics(&sm, &diags, n_errors, n_warnings);
    } else {
        for d in &diags {
            print_diagnostic(&sm, d);
        }
        if n_errors > 0 || (strict && n_warnings > 0) {
            println!("\n✗ {} error(s), {} warning(s)", n_errors, n_warnings);
        } else if n_warnings > 0 {
            println!(
                "✓ types/effects ok ({} warning(s), refinements partially verified)",
                n_warnings
            );
        } else {
            println!("✓ types, effects, and refinements verified");
        }
    }
    if n_errors > 0 || (strict && n_warnings > 0) {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

fn cmd_lint(file: PathBuf, no_imports: bool, deny_warnings: bool) -> ExitCode {
    let mut sm = SourceMap::new();
    let m = match resolve_module(&file, no_imports, &mut sm) {
        Ok(m) => m,
        Err(code) => return code,
    };
    // Run type-check first so lint output isn't drowned in type errors.
    let (_, mut diags) = check_module(&m);
    let type_errors = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .count();
    if type_errors > 0 {
        for d in &diags {
            if d.severity == Severity::Error {
                print_diagnostic(&sm, d);
            }
        }
        eprintln!("✗ {type_errors} type error(s) — fix these before linting");
        return ExitCode::from(1);
    }
    let lints = lint::lint_module(&m);
    diags.extend(lints);
    let warnings = diags
        .iter()
        .filter(|d| d.severity == Severity::Warning)
        .count();
    let notes = diags
        .iter()
        .filter(|d| d.severity == Severity::Note)
        .count();
    for d in &diags {
        if matches!(d.severity, Severity::Warning | Severity::Note) {
            print_diagnostic(&sm, d);
        }
    }
    if warnings == 0 && notes == 0 {
        println!("✓ no lints");
        ExitCode::SUCCESS
    } else {
        println!("\n{warnings} warning(s), {notes} note(s)");
        if deny_warnings && warnings > 0 {
            ExitCode::from(1)
        } else {
            ExitCode::SUCCESS
        }
    }
}

fn print_json_diagnostics(
    sm: &SourceMap,
    diags: &[Diagnostic],
    n_errors: usize,
    n_warnings: usize,
) {
    use std::fmt::Write;
    let mut out = String::from("{\n  \"diagnostics\": [\n");
    for (i, d) in diags.iter().enumerate() {
        if i > 0 {
            out.push_str(",\n");
        }
        let (line, col) = sm.line_col(d.span);
        let sev = match d.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Note => "note",
        };
        let msg_escaped = d
            .msg
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n");
        let _ = write!(
            out,
            "    {{\"severity\":\"{}\",\"file\":\"{}\",\"line\":{},\"col\":{},\"start\":{},\"end\":{},\"message\":\"{}\"}}",
            sev,
            sm.name(d.span.file),
            line,
            col,
            d.span.start,
            d.span.end,
            msg_escaped,
        );
    }
    out.push_str("\n  ],\n");
    let _ = write!(
        out,
        "  \"errors\": {n_errors},\n  \"warnings\": {n_warnings}\n}}\n"
    );
    print!("{out}");
}

fn cmd_run(file: PathBuf, no_check: bool, no_imports: bool, network: bool, bc: bool) -> ExitCode {
    let mut sm = SourceMap::new();
    let m = match resolve_module(&file, no_imports, &mut sm) {
        Ok(m) => m,
        Err(code) => return code,
    };
    if !no_check {
        let (_, diags) = check_module(&m);
        let n_errors = diags
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .count();
        for d in &diags {
            if d.severity == Severity::Error {
                print_diagnostic(&sm, d);
            }
        }
        if n_errors > 0 {
            eprintln!("✗ refusing to run: {n_errors} error(s)");
            return ExitCode::from(1);
        }
    }

    // Bytecode path: try BC, fall back on unsupported features.
    if bc {
        match bc_runner::run_via_bc(&m) {
            Ok(maybe_int) => {
                if let Some(n) = maybe_int {
                    println!("{n}");
                }
                return ExitCode::SUCCESS;
            }
            Err(msg) => {
                if let Some(what) = bc_runner::is_unsupported(&msg) {
                    eprintln!(
                        "note: bytecode VM doesn't support {what} — falling back to tree-walker"
                    );
                    // fall through to tree-walker below
                } else {
                    eprintln!("runtime error: {msg}");
                    return ExitCode::from(1);
                }
            }
        }
    }

    // Tree-walker path (always used when !bc, or as fallback).
    let mut rt = Runtime::new(m);
    if network {
        aether_tools_net::install(&mut rt.tools);
    }
    match rt.run_main() {
        Ok(v) => {
            // Print final value (helpful in tests / agent workflows).
            if !matches!(v, Value::Unit(_)) {
                println!("{}", v.display());
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("runtime error: {e}");
            ExitCode::from(1)
        }
    }
}

fn cmd_fmt(file: PathBuf, verbose: bool, check: bool) -> ExitCode {
    let Some(src) = read(&file) else {
        return ExitCode::from(2);
    };
    let mut sm = SourceMap::new();
    let fid = sm.add(file.display().to_string(), src.clone());
    let m = match parse_module(fid, &src) {
        Ok(m) => m,
        Err(e) => {
            print_parse_error(&sm, fid, &e);
            return ExitCode::from(1);
        }
    };
    let form = if verbose {
        Form::Verbose
    } else {
        Form::Compact
    };
    let formatted = pp_module(&m, form);
    if check {
        // Trim trailing whitespace/newlines before comparing to be lenient.
        if formatted.trim_end() != src.trim_end() {
            eprintln!("✗ {} is not formatted", file.display());
            ExitCode::from(1)
        } else {
            ExitCode::SUCCESS
        }
    } else {
        print!("{formatted}");
        ExitCode::SUCCESS
    }
}

fn cmd_explain(file: PathBuf) -> ExitCode {
    let Some(src) = read(&file) else {
        return ExitCode::from(2);
    };
    let mut sm = SourceMap::new();
    let fid = sm.add(file.display().to_string(), src.clone());
    let m = match parse_module(fid, &src) {
        Ok(m) => m,
        Err(e) => {
            print_parse_error(&sm, fid, &e);
            return ExitCode::from(1);
        }
    };
    let mut rt = Runtime::new(m);
    rt.capture_only = true;
    // Synthesize a call to introspect("current") via the builtin dispatch.
    use aether_ast::Expr;
    let call = Expr::Call {
        callee: Box::new(Expr::Var("introspect".into(), aether_ast::Span::DUMMY)),
        args: vec![aether_ast::Arg {
            name: None,
            value: Expr::Lit(
                aether_ast::Lit::Str("current".into()),
                aether_ast::Span::DUMMY,
            ),
            span: aether_ast::Span::DUMMY,
        }],
        span: aether_ast::Span::DUMMY,
    };
    match rt.eval_root(&call) {
        Ok(v) => {
            print!("{}", v.display());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(1)
        }
    }
}

fn cmd_ast(file: PathBuf, emit_json: bool, pretty: bool) -> ExitCode {
    let Some(src) = read(&file) else {
        return ExitCode::from(2);
    };
    let mut sm = SourceMap::new();
    let fid = sm.add(file.display().to_string(), src.clone());
    match parse_module(fid, &src) {
        Ok(m) => {
            if emit_json {
                println!("{}", ast_json::emit(&m, pretty));
            } else {
                println!("{m:#?}");
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            print_parse_error(&sm, fid, &e);
            ExitCode::from(1)
        }
    }
}

fn cmd_compile(file: PathBuf, output: Option<PathBuf>, no_imports: bool) -> ExitCode {
    let mut sm = SourceMap::new();
    let m = match resolve_module(&file, no_imports, &mut sm) {
        Ok(m) => m,
        Err(code) => return code,
    };

    let program = match aether_bc::compile_module(&m) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("compile error: {e}");
            return ExitCode::from(1);
        }
    };

    let bytes = aether_bc::serialize_program(&program);

    let out_path = output.unwrap_or_else(|| file.with_extension("aebc"));
    if let Err(e) = fs::write(&out_path, &bytes) {
        eprintln!("error: could not write {}: {e}", out_path.display());
        return ExitCode::from(1);
    }

    println!(
        "compiled {} → {} ({} bytes)",
        file.display(),
        out_path.display(),
        bytes.len()
    );
    ExitCode::SUCCESS
}

fn cmd_exec(file: PathBuf) -> ExitCode {
    let bytes = match fs::read(&file) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("error: could not read {}: {e}", file.display());
            return ExitCode::from(2);
        }
    };

    let program = match aether_bc::deserialize_program(&bytes) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: invalid .aebc file {}: {e}", file.display());
            return ExitCode::from(1);
        }
    };

    match aether_bc::run_main(&program) {
        Ok(v) => {
            if !matches!(v, aether_bc::Value::Unit) {
                println!("{}", v.display());
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("runtime error: {e}");
            ExitCode::from(1)
        }
    }
}

// --- diagnostics -------------------------------------------------------------

pub(crate) fn print_diagnostic(sm: &SourceMap, d: &Diagnostic) {
    let (kind, color) = match d.severity {
        Severity::Error => (ReportKind::Error, Color::Red),
        Severity::Warning => (ReportKind::Warning, Color::Yellow),
        Severity::Note => (ReportKind::Advice, Color::Blue),
    };
    let span = d.span;
    let fname = sm.name(span.file).to_string();
    let src = sm.source(span.file).to_string();
    let mut r = Report::build(kind, fname.clone(), span.start as usize)
        .with_message(d.msg.clone())
        .with_label(
            Label::new((fname.clone(), span.range()))
                .with_message(&d.msg)
                .with_color(color),
        );
    if d.severity == Severity::Warning {
        r = r.with_help("the solver reported `Unknown`; consider strengthening the precondition or using `assume`");
    }
    let cache = (fname, ariadne::Source::from(src));
    let _ = r.finish().eprint(cache);
}

fn print_parse_error(sm: &SourceMap, fid: FileId, e: &aether_parser::ParseError) {
    let span = e.span().unwrap_or(aether_ast::Span::new(fid, 0..0));
    let fname = sm.name(span.file).to_string();
    let src = sm.source(span.file).to_string();
    let r = Report::build(ReportKind::Error, fname.clone(), span.start as usize)
        .with_message(format!("{e}"))
        .with_label(
            Label::new((fname.clone(), span.range()))
                .with_message(format!("{e}"))
                .with_color(Color::Red),
        );
    let cache = (fname, ariadne::Source::from(src));
    let _ = r.finish().eprint(cache);
}

#[cfg(test)]
mod tests {
    /// Verify the CLI accepts --network without error on --help.
    #[test]
    fn cli_accepts_network_flag_in_help() {
        // We can't easily call the binary in unit tests, but we can verify
        // that the Cli struct parses --network via clap's try_parse.
        // This ensures the flag is registered and global.
        use clap::Parser as ClapParser;
        let result = super::Cli::try_parse_from(["aether", "--network", "run", "file.ae"]);
        assert!(
            result.is_ok(),
            "CLI should accept --network before subcommand"
        );

        let result2 = super::Cli::try_parse_from(["aether", "run", "--network", "file.ae"]);
        assert!(
            result2.is_ok(),
            "CLI should accept --network after subcommand (global flag)"
        );
    }

    #[test]
    fn cli_accepts_bc_flag() {
        use clap::Parser as ClapParser;
        let result = super::Cli::try_parse_from(["aether", "run", "--bc", "file.ae"]);
        assert!(result.is_ok(), "CLI should accept --bc on run subcommand");
    }

    #[test]
    fn cli_accepts_compile_subcommand() {
        use clap::Parser as ClapParser;
        let result = super::Cli::try_parse_from(["aether", "compile", "file.ae"]);
        assert!(result.is_ok(), "CLI should accept compile subcommand");
    }

    #[test]
    fn cli_accepts_exec_subcommand() {
        use clap::Parser as ClapParser;
        let result = super::Cli::try_parse_from(["aether", "exec", "file.aebc"]);
        assert!(result.is_ok(), "CLI should accept exec subcommand");
    }
}

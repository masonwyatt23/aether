//! `aether verify` — strict correctness gate.
//!
//! Unlike `aether check` (which exits 0 when warnings are present), `verify`
//! exits 0 **only** when there are zero errors AND zero warnings.  A `Warning`
//! from the refinement checker means the solver could not decide the
//! postcondition — "could not prove" is unacceptable for a strict gate.
//!
//! Intended for CI pipelines and AI-agent coding loops where you need a proof,
//! not a best-effort.

use std::fmt::Write as FmtWrite;
use std::path::PathBuf;
use std::process::ExitCode;

use aether_ast::SourceMap;
use aether_types::{check_module, Diagnostic, Severity};

use crate::{print_diagnostic, resolve_module};

/// Run strict verification on `file`.
///
/// Returns `ExitCode::SUCCESS` only when `check_module` produces **no**
/// `Error` diagnostics and **no** `Warning` diagnostics.  Any warning (which
/// the refinement checker emits when it cannot prove a postcondition) is
/// treated as a hard failure.
pub fn run_verify(file: PathBuf, no_imports: bool, json: bool) -> ExitCode {
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
    // Warnings from the refinement checker mean "solver returned Unknown" —
    // unproven contracts.  Treat them as failures in verify mode.
    let n_warnings = diags
        .iter()
        .filter(|d| d.severity == Severity::Warning)
        .count();

    let verified = n_errors == 0 && n_warnings == 0;

    if json {
        print_json_verdict(&sm, &diags, verified, n_errors, n_warnings);
    } else {
        for d in &diags {
            print_diagnostic(&sm, d);
        }
        if verified {
            println!("\u{2713} verified \u{2014} all contracts proved, all effects sound");
        } else {
            println!(
                "\n\u{2717} verification failed \u{2014} {} error(s), {} unproven contract(s)",
                n_errors, n_warnings
            );
        }
    }

    if verified {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn print_json_verdict(
    sm: &SourceMap,
    diags: &[Diagnostic],
    verified: bool,
    n_errors: usize,
    n_warnings: usize,
) {
    let mut out = String::new();
    let _ = write!(
        out,
        "{{\n  \"verified\": {},\n  \"errors\": {},\n  \"warnings\": {},\n  \"diagnostics\": [\n",
        verified, n_errors, n_warnings
    );
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
    out.push_str("\n  ]\n}\n");
    print!("{out}");
}

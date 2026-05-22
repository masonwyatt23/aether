//! Persistent check server (`aether serve`).
//!
//! Reads newline-delimited JSON requests on stdin and writes one JSON
//! response line per request, looping until stdin closes. This amortizes
//! process startup (Rust runtime init, binary page-in, the z3-availability
//! probe) for tools that invoke the checker many times -- notably an RL
//! training loop, where `aether check` is called millions of times.
//!
//! Protocol. Request (one JSON object per line):
//!     {"source": "<aether source>", "id": <optional, any JSON value>}
//! Response (one JSON object per line):
//!     {"id": <echoed>, "diagnostics": [...], "errors": N, "warnings": N}
//! or, for a malformed request:
//!     {"id": <echoed-or-null>, "error": "<message>"}
//!
//! Each request is checked statelessly in-process (a fresh `SourceMap`), so
//! the loop is safe and there is no cross-request state to corrupt.

use std::io::{self, BufRead, Write};
use std::process::ExitCode;

use crate::check_source_to_json;

pub fn run_serve() -> ExitCode {
    let stdin = io::stdin();
    let mut out = io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break, // stdin closed or unreadable
        };
        if line.trim().is_empty() {
            continue;
        }
        let response = handle(&line);
        if writeln!(out, "{response}").is_err() {
            break; // downstream closed the pipe
        }
        if out.flush().is_err() {
            break;
        }
    }
    ExitCode::SUCCESS
}

/// Handle one request line; always returns a single-line JSON response.
fn handle(line: &str) -> String {
    let value: serde_json::Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => return error_response("null", &e.to_string()),
    };
    // Echo the request `id` (any JSON value) verbatim; absent -> null.
    let id = match value.get("id") {
        Some(x) => serde_json::to_string(x).unwrap_or_else(|_| "null".into()),
        None => "null".into(),
    };
    let source = match value.get("source").and_then(|s| s.as_str()) {
        Some(s) => s,
        None => return error_response(&id, "request missing string field `source`"),
    };
    // `check_source_to_json` returns `{\n  "diagnostics": ...}`; splice the
    // echoed `id` in after the opening brace and collapse to one line.
    let body = check_source_to_json("<serve>", source);
    body.replacen('{', &format!("{{\"id\":{id},"), 1)
        .replace('\n', "")
}

fn error_response(id: &str, msg: &str) -> String {
    let escaped = msg.replace('\\', "\\\\").replace('"', "\\\"");
    format!("{{\"id\":{id},\"error\":\"{escaped}\"}}")
}

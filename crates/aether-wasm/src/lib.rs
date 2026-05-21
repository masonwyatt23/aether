//! Aether → WebAssembly bindings.
//!
//! Exposes `aether_check`, `aether_run`, `aether_format`, and `aether_introspect`
//! to JavaScript via wasm-bindgen. Each function takes a source string and
//! returns either a structured JsValue or a plain String.
//!
//! Network tools (`http_get`, `llm_complete`) remain as deterministic stubs —
//! no real I/O is attempted from the browser sandbox.

use wasm_bindgen::prelude::*;

use aether_ast::SourceMap;
use aether_eval::{Runtime, Value};
use aether_parser::parse_module;
use aether_parser::pretty::{module as pp_module, Form};
use aether_types::{check_module, Severity};
use serde::Serialize;

// ── serialisable types returned to JS ────────────────────────────────────────

#[derive(Serialize, Debug)]
pub struct DiagnosticJs {
    pub severity: &'static str,
    pub message: String,
    pub line: usize,
    pub col: usize,
    pub start: u32,
    pub end: u32,
}

#[derive(Serialize)]
pub struct CheckResult {
    pub ok: bool,
    pub errors: Vec<DiagnosticJs>,
    pub warnings: Vec<DiagnosticJs>,
    pub notes: Vec<DiagnosticJs>,
}

#[derive(Serialize)]
pub struct RunResult {
    pub ok: bool,
    pub stdout: String,
    pub error: Option<String>,
    pub value: Option<String>,
}

// ── internal helpers ──────────────────────────────────────────────────────────

fn build_diag_js(sm: &SourceMap, d: &aether_types::Diagnostic) -> DiagnosticJs {
    let (line, col) = sm.line_col(d.span);
    DiagnosticJs {
        severity: match d.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Note => "note",
        },
        message: d.msg.clone(),
        line,
        col,
        start: d.span.start,
        end: d.span.end,
    }
}

fn parse_source(source: &str) -> Result<(SourceMap, aether_ast::Module), String> {
    let mut sm = SourceMap::new();
    let fid = sm.add("<playground>", source);
    match parse_module(fid, source) {
        Ok(m) => Ok((sm, m)),
        Err(e) => {
            // Format a plain-text parse error with location.
            let span = e.span().unwrap_or(aether_ast::Span::new(fid, 0..0));
            let (line, col) = sm.line_col(span);
            Err(format!("parse error at {}:{}: {}", line, col, e))
        }
    }
}

// ── wasm-bindgen exports ──────────────────────────────────────────────────────

/// Type-check (parse + type/effect/refinement verify) a source string.
/// Returns `{ ok, errors[], warnings[], notes[] }`.
/// Each diagnostic has `{ severity, message, line, col, start, end }`.
#[wasm_bindgen]
pub fn aether_check(source: &str) -> JsValue {
    let (sm, m) = match parse_source(source) {
        Ok(pair) => pair,
        Err(msg) => {
            let result = CheckResult {
                ok: false,
                errors: vec![DiagnosticJs {
                    severity: "error",
                    message: msg,
                    line: 1,
                    col: 1,
                    start: 0,
                    end: 0,
                }],
                warnings: vec![],
                notes: vec![],
            };
            return serde_wasm_bindgen::to_value(&result).unwrap_or(JsValue::NULL);
        }
    };

    let (_, diags) = check_module(&m);
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let mut notes = Vec::new();

    for d in &diags {
        let dj = build_diag_js(&sm, d);
        match d.severity {
            Severity::Error => errors.push(dj),
            Severity::Warning => warnings.push(dj),
            Severity::Note => notes.push(dj),
        }
    }

    let ok = errors.is_empty();
    let result = CheckResult { ok, errors, warnings, notes };
    serde_wasm_bindgen::to_value(&result).unwrap_or(JsValue::NULL)
}

/// Parse, type-check (errors only), then run `main()`.
/// Returns `{ ok, stdout, error?, value? }`.
/// `stdout` contains everything printed via `print(...)`.
/// `error` is set if there were type errors or a runtime error.
#[wasm_bindgen]
pub fn aether_run(source: &str) -> JsValue {
    let (sm, m) = match parse_source(source) {
        Ok(pair) => pair,
        Err(msg) => {
            let result = RunResult { ok: false, stdout: String::new(), error: Some(msg), value: None };
            return serde_wasm_bindgen::to_value(&result).unwrap_or(JsValue::NULL);
        }
    };

    // Type-check: reject on errors (warnings are fine).
    let (_, diags) = check_module(&m);
    let errors: Vec<_> = diags.iter().filter(|d| d.severity == Severity::Error).collect();
    if !errors.is_empty() {
        let msgs: Vec<String> = errors
            .iter()
            .map(|d| {
                let (line, col) = sm.line_col(d.span);
                format!("{}:{}: {}", line, col, d.msg)
            })
            .collect();
        let result = RunResult {
            ok: false,
            stdout: String::new(),
            error: Some(msgs.join("\n")),
            value: None,
        };
        return serde_wasm_bindgen::to_value(&result).unwrap_or(JsValue::NULL);
    }

    // Run — always capture-only (no host stdout in wasm).
    let mut rt = Runtime::new(m);
    rt.capture_only = true;

    // Replace mem_get/mem_set with no-op wasm stubs (filesystem unavailable).
    install_wasm_tool_stubs(&mut rt);

    match rt.run_main() {
        Ok(v) => {
            let value_str = if matches!(v, Value::Unit(_)) {
                None
            } else {
                Some(v.display().to_string())
            };
            let result = RunResult {
                ok: true,
                stdout: rt.stdout.clone(),
                error: None,
                value: value_str,
            };
            serde_wasm_bindgen::to_value(&result).unwrap_or(JsValue::NULL)
        }
        Err(e) => {
            let result = RunResult {
                ok: false,
                stdout: rt.stdout.clone(),
                error: Some(e.to_string()),
                value: None,
            };
            serde_wasm_bindgen::to_value(&result).unwrap_or(JsValue::NULL)
        }
    }
}

/// Format (pretty-print) a source string.
/// `verbose = true` uses the human-readable verbose form; `false` uses compact.
/// Returns the formatted source, or an error message prefixed with "error: ".
#[wasm_bindgen]
pub fn aether_format(source: &str, verbose: bool) -> String {
    let (_, m) = match parse_source(source) {
        Ok(pair) => pair,
        Err(msg) => return format!("error: {}", msg),
    };
    let form = if verbose { Form::Verbose } else { Form::Compact };
    pp_module(&m, form)
}

/// Return the module's introspection surface as a string.
/// Equivalent to calling `introspect("current")` in-language.
#[wasm_bindgen]
pub fn aether_introspect(source: &str) -> String {
    let (_, m) = match parse_source(source) {
        Ok(pair) => pair,
        Err(msg) => return format!("error: {}", msg),
    };
    let mut rt = Runtime::new(m);
    rt.capture_only = true;
    install_wasm_tool_stubs(&mut rt);

    // Synthesise `introspect("current")`.
    use aether_ast::{Arg, Expr, Lit, Span};
    let call = Expr::Call {
        callee: Box::new(Expr::Var("introspect".into(), Span::DUMMY)),
        args: vec![Arg {
            name: None,
            value: Expr::Lit(Lit::Str("current".into()), Span::DUMMY),
            span: Span::DUMMY,
        }],
        span: Span::DUMMY,
    };
    match rt.eval_root(&call) {
        Ok(v) => v.display().to_string(),
        Err(e) => format!("error: {}", e),
    }
}

// ── wasm tool stubs ───────────────────────────────────────────────────────────

/// Replace file-system-backed tools with browser-safe no-ops.
fn install_wasm_tool_stubs(rt: &mut Runtime) {
    use aether_ast::{ProvArena, ProvChain, ProvOp, Span};

    // mem_get: always returns empty string (no persistent store in wasm).
    rt.tools.register("mem_get", |_args| {
        let arena = ProvArena::new();
        let prov = ProvChain::singleton(arena, ProvOp::Tool("mem_get".into()), Span::DUMMY);
        Ok(Value::Str(String::new(), prov))
    });

    // mem_set: silently succeeds (no-op).
    rt.tools.register("mem_set", |_args| {
        let arena = ProvArena::new();
        let prov = ProvChain::singleton(arena, ProvOp::Tool("mem_set".into()), Span::DUMMY);
        Ok(Value::Unit(prov))
    });

    // llm_complete: deterministic stub (already the default, but make explicit).
    // The default installed by Runtime::new is already a stub; no override needed.
}

// ── native (non-wasm) API surface — callable from Rust tests without wasm-pack ─

/// Non-wasm-bindgen version of check: returns a plain Rust struct.
/// Used by native integration tests and for programmatic embedding.
pub fn check_source(source: &str) -> CheckResult {
    let (sm, m) = match parse_source(source) {
        Ok(pair) => pair,
        Err(msg) => {
            return CheckResult {
                ok: false,
                errors: vec![DiagnosticJs {
                    severity: "error",
                    message: msg,
                    line: 1,
                    col: 1,
                    start: 0,
                    end: 0,
                }],
                warnings: vec![],
                notes: vec![],
            };
        }
    };
    let (_, diags) = check_module(&m);
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let mut notes = Vec::new();
    for d in &diags {
        let dj = build_diag_js(&sm, d);
        match d.severity {
            Severity::Error => errors.push(dj),
            Severity::Warning => warnings.push(dj),
            Severity::Note => notes.push(dj),
        }
    }
    let ok = errors.is_empty();
    CheckResult { ok, errors, warnings, notes }
}

/// Non-wasm-bindgen version of run: returns a plain Rust struct.
pub fn run_source(source: &str) -> RunResult {
    let (sm, m) = match parse_source(source) {
        Ok(pair) => pair,
        Err(msg) => {
            return RunResult { ok: false, stdout: String::new(), error: Some(msg), value: None };
        }
    };
    let (_, diags) = check_module(&m);
    let errors: Vec<_> = diags.iter().filter(|d| d.severity == Severity::Error).collect();
    if !errors.is_empty() {
        let msgs: Vec<String> = errors
            .iter()
            .map(|d| {
                let (line, col) = sm.line_col(d.span);
                format!("{}:{}: {}", line, col, d.msg)
            })
            .collect();
        return RunResult {
            ok: false,
            stdout: String::new(),
            error: Some(msgs.join("\n")),
            value: None,
        };
    }
    let mut rt = Runtime::new(m);
    rt.capture_only = true;
    install_wasm_tool_stubs(&mut rt);
    match rt.run_main() {
        Ok(v) => {
            let value_str = if matches!(v, Value::Unit(_)) { None } else { Some(v.display().to_string()) };
            RunResult { ok: true, stdout: rt.stdout.clone(), error: None, value: value_str }
        }
        Err(e) => RunResult { ok: false, stdout: rt.stdout.clone(), error: Some(e.to_string()), value: None },
    }
}

//! Bytecode runner helper — isolates BC compile+run from `main.rs`.

use aether_ast::Module;
use aether_bc::{compile_module, run_main, CompileError};

/// Try to compile and run `m` via the bytecode VM.
///
/// Returns:
/// - `Ok(Some(n))` — main returned an `Int` value `n`.
/// - `Ok(None)`    — main returned `Unit`.
/// - `Err(msg)`    — compile/runtime failure (caller decides whether to fall back).
///
/// The caller is responsible for printing the fallback note when
/// `Err` wraps a `CompileError::Unsupported` message.
pub fn run_via_bc(m: &Module) -> Result<Option<i64>, String> {
    let program = compile_module(m).map_err(|e| match &e {
        CompileError::Unsupported(s) => format!("unsupported:{s}"),
        other => format!("{other}"),
    })?;

    let value = run_main(&program).map_err(|e| e.to_string())?;
    Ok(value.as_int())
}

/// Returns true if the error message indicates an unsupported-feature compile
/// error (so the CLI can fall back gracefully).
pub fn is_unsupported(msg: &str) -> Option<&str> {
    msg.strip_prefix("unsupported:")
}

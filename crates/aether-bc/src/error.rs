//! Error types for the bytecode compiler and VM.

use thiserror::Error;

/// Error produced during compilation of an AST module to bytecode.
#[derive(Debug, Error)]
pub enum CompileError {
    #[error("unbound variable `{0}`")]
    UnboundVar(String),
    #[error("undefined function `{0}`")]
    UndefinedFn(String),
    #[error("unsupported construct in bytecode path: {0}")]
    Unsupported(String),
}

/// Error produced at VM runtime.
#[derive(Debug, Error)]
pub enum VmError {
    #[error("stack underflow")]
    StackUnderflow,
    #[error("instruction pointer out of bounds")]
    IpOutOfBounds,
    #[error("bad constant index {0}")]
    BadConstantIndex(u32),
    #[error("bad local index {0}")]
    BadLocalIndex(u16),
    #[error("unknown builtin id {0}")]
    UnknownBuiltin(u16),
    #[error("arity mismatch for `{name}`: expected {expected}, got {got}")]
    ArityMismatch {
        name: String,
        expected: usize,
        got: usize,
    },
    #[error("type error: {0}")]
    TypeError(String),
    #[error("division by zero")]
    DivByZero,
    #[error("user error: {0}")]
    User(String),
}

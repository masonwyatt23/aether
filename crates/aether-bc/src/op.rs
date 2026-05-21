//! Bytecode instruction set for the Aether stack-based VM.

use serde::{Deserialize, Serialize};

/// A single bytecode instruction.
///
/// The VM is stack-based: most ops pop their operands from and push results
/// onto an operand stack.  Strings are interned in a constant pool accessed
/// by index.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Op {
    // ── literals ─────────────────────────────────────────────────────────────
    /// Push a 64-bit signed integer.
    PushInt(i64),
    /// Push a boolean.
    PushBool(bool),
    /// Push a string constant by index into `Program::constants`.
    PushStr(u32),
    /// Push a 64-bit IEEE 754 float.
    PushFloat(f64),
    /// Push the unit value `()`.
    PushUnit,

    // ── locals ───────────────────────────────────────────────────────────────
    /// Load local variable at slot index.
    LoadLocal(u16),
    /// Store the top-of-stack into local slot (pops one value).
    StoreLocal(u16),

    // ── arithmetic ───────────────────────────────────────────────────────────
    Add,
    Sub,
    Mul,
    Div,
    Mod,

    // ── comparison ───────────────────────────────────────────────────────────
    Eq,
    Neq,
    Lt,
    Le,
    Gt,
    Ge,

    // ── logic ────────────────────────────────────────────────────────────────
    And,
    Or,
    Not,
    Implies,

    // ── unary ────────────────────────────────────────────────────────────────
    Neg,

    // ── strings / lists ──────────────────────────────────────────────────────
    /// Concatenate two strings (or two lists) on the stack.
    Concat,

    /// Pop the top-of-stack, convert to a display string, push `Value::Str`.
    ///
    /// Used by string-interpolation codegen: each `StrPart::Expr` is followed
    /// by `ToStr` so the subsequent `Concat` always sees two `Str` values.
    /// Matches the `Value::display()` formatting used by the tree-walker.
    ToStr,

    // ── control flow ─────────────────────────────────────────────────────────
    /// Unconditional relative jump (offset in instructions, signed).
    Jump(i32),
    /// Pop a bool; jump if it is `false`.
    JumpIfFalse(i32),

    // ── calls ────────────────────────────────────────────────────────────────
    /// Call a user-defined function from `Program::fns` with `argc` args.
    Call {
        fn_idx: u32,
        argc: u8,
    },
    /// Call a built-in function by ID with `argc` args.
    CallBuiltin {
        id: u16,
        argc: u8,
    },
    /// Call a builtin by name (dynamic dispatch via `BuiltinDispatcher`).
    ///
    /// Used as a fallback for builtins not natively implemented in the BC VM.
    /// `name_idx` indexes `Program::constants` for the function name string.
    /// The VM delegates to the registered `BuiltinDispatcher` at runtime.
    CallBuiltinDyn {
        name_idx: u32,
        argc: u8,
    },
    /// Call a `Value::Closure` sitting below `argc` args on the stack.
    ///
    /// Stack layout before: `[... | closure | arg0 | ... | argN-1]`
    /// Pops closure + args; pushes result.
    CallClosure {
        argc: u8,
    },
    /// Return the top-of-stack from the current call frame.
    Ret,

    // ── closures ─────────────────────────────────────────────────────────────
    /// Create a `Value::Closure` from a bytecode function and captured locals.
    ///
    /// `captured` lists the local-slot indices (of the **enclosing** frame) to
    /// capture by value at the point of closure creation.
    MakeClosure {
        fn_idx: u32,
        captured: Vec<u16>,
    },

    // ── constructors (ADTs) ──────────────────────────────────────────────────
    /// Build a `Value::Ctor { name, args }` from the top `argc` stack values.
    ///
    /// `name_idx` indexes `Program::constants` (a `Constant::Str`).
    /// Args are popped left-to-right (first pushed is `args[0]`).
    Ctor {
        name_idx: u32,
        argc: u8,
    },

    /// Test whether the top-of-stack is a `Value::Ctor` with the given name
    /// and arity — **without popping** the scrutinee.
    ///
    /// On match: pushes `true`.
    /// On miss:  pushes `false` and additionally jumps by `jump_if_miss`
    ///           (relative to the instruction after this one) so the VM skips
    ///           the arm body sequence.
    MatchCtor {
        name_idx: u32,
        expect_arity: u8,
        jump_if_miss: i32,
    },

    /// Extract field at `field_idx` from the `Value::Ctor` on top of stack.
    ///
    /// Does **not** pop the ctor; callers must `Pop` it afterward if desired.
    /// Pushes `ctor.args[field_idx]`.
    CtorField(u8),

    // ── tuples ───────────────────────────────────────────────────────────────
    /// Pop `n` values (first pushed = element 0), build a `Value::Tuple`.
    MakeTuple(u16),

    /// Pop a `Value::Tuple`; push element at position `index`.
    TupleGet(u16),

    // ── records ──────────────────────────────────────────────────────────────
    /// Pop `n` values from the stack (first pushed = field 0's value) and
    /// assemble them with the parallel `field_names` vector into a
    /// `Value::Record`.
    MakeRecord {
        field_names: Vec<u32>,
    },

    /// Pop a `Value::Record`; push the value of the field named by `name_idx`.
    /// Uses a linear scan (records are small).
    FieldGet(u32),

    // ── list construction ────────────────────────────────────────────────────
    /// Pop `n` values (last pushed = last element), build a List, push it.
    MakeList(u16),

    /// Pop a `Value::List` or `Value::Tuple`; pop an `Int` index; push element.
    Index,

    // ── misc ─────────────────────────────────────────────────────────────────
    /// Pop and discard the top-of-stack.
    Pop,

    /// Duplicate the top-of-stack (push a clone without consuming the original).
    Dup,

    // ── confidence ───────────────────────────────────────────────────────────
    /// Pop `p: Float` then `inner: Value` from the stack (value was pushed first,
    /// then p on top) and push `Value::Confident { inner, p }`.
    ///
    /// Stack before: `[... | inner | p_float]`
    /// Stack after:  `[... | Confident(inner, p)]`
    ///
    /// Matches the tree-walker's `"{value} ~confidence({p})"` display format.
    MakeConfident,
}

/// IDs for built-in functions; kept small so `CallBuiltin` stays compact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum BuiltinId {
    Print = 0,
    Str = 1,
    Int = 2,
    Len = 3,
    Abs = 4,
    Max = 5,
    Min = 6,
}

impl BuiltinId {
    /// Map a function name to its builtin id, if it is one.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "print" | "println" => Some(Self::Print),
            "str" => Some(Self::Str),
            "int" => Some(Self::Int),
            "len" => Some(Self::Len),
            "abs" => Some(Self::Abs),
            "max" => Some(Self::Max),
            "min" => Some(Self::Min),
            _ => None,
        }
    }
}

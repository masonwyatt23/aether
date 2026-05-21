//! Stack-based bytecode VM for Aether.
//!
//! Each call frame owns a fixed-size locals array (allocated to `n_locals`).
//! The operand stack is shared across all frames (callee appends on top of
//! caller's stack slice, then pops when returning).

use serde::{Deserialize, Serialize};

use crate::error::VmError;
use crate::op::{BuiltinId, Op};

// ─── public data types ───────────────────────────────────────────────────────

/// Compiled representation of a single Aether function.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BytecodeFn {
    pub name: String,
    pub arity: u8,
    pub n_locals: u16,
    pub code: Vec<Op>,
}

/// A constant pool entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Constant {
    Str(String),
}

/// A fully compiled Aether module ready for execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Program {
    pub fns: Vec<BytecodeFn>,
    pub constants: Vec<Constant>,
    /// Index of the entry-point function (usually `main`).
    pub entry: u32,
}

// ─── runtime value ────────────────────────────────────────────────────────────

/// Lightweight runtime value — no provenance overhead.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i64),
    Bool(bool),
    Str(String),
    Float(f64),
    Unit,
    List(Vec<Value>),
    Tuple(Vec<Value>),
    Record(Vec<(String, Value)>),
    /// A tagged ADT value, e.g. `Circle(5.0)` → `Ctor { name: "Circle", args: [Float(5.0)] }`.
    Ctor { name: String, args: Vec<Value> },
    /// A closure capturing locals by value from its definition site.
    Closure { fn_idx: u32, captured: Vec<Value> },
    /// A confidence-annotated value: `confident(v, p)` produces this.
    /// Display mirrors `aether_eval::Value::Confident`: `"{v} ~confidence({p})"`.
    Confident { inner: Box<Value>, p: f64 },
}

impl Value {
    pub fn as_int(&self) -> Option<i64> {
        if let Value::Int(n) = self { Some(*n) } else { None }
    }
    pub fn as_bool(&self) -> Option<bool> {
        if let Value::Bool(b) = self { Some(*b) } else { None }
    }
    pub fn as_str(&self) -> Option<&str> {
        if let Value::Str(s) = self { Some(s.as_str()) } else { None }
    }
    pub fn as_float(&self) -> Option<f64> {
        if let Value::Float(f) = self { Some(*f) } else { None }
    }

    /// Human-readable display (mirrors `aether_eval::Value::display`).
    pub fn display(&self) -> String {
        match self {
            Value::Int(n)    => n.to_string(),
            Value::Bool(b)   => b.to_string(),
            Value::Str(s)    => s.clone(),
            Value::Float(f)  => f.to_string(),
            Value::Unit      => "()".to_string(),
            Value::List(vs)  => {
                let inner: Vec<String> = vs.iter().map(Value::display).collect();
                format!("[{}]", inner.join(", "))
            }
            Value::Tuple(vs) => {
                let inner: Vec<String> = vs.iter().map(Value::display).collect();
                format!("({})", inner.join(", "))
            }
            Value::Record(fs) => {
                let inner: Vec<String> = fs.iter()
                    .map(|(k, v)| format!("{}: {}", k, v.display()))
                    .collect();
                format!("{{{}}}", inner.join(", "))
            }
            Value::Ctor { name, args } => {
                if args.is_empty() {
                    name.clone()
                } else {
                    let inner: Vec<String> = args.iter().map(Value::display).collect();
                    format!("{}({})", name, inner.join(", "))
                }
            }
            Value::Closure { fn_idx, .. } => format!("<closure fn{}>", fn_idx),
            Value::Confident { inner, p } => format!("{} ~confidence({p})", inner.display()),
        }
    }
}

// ─── builtin dispatcher trait ────────────────────────────────────────────────

/// Trait for dispatching dynamic builtin calls from the bytecode VM.
///
/// Implement this to provide a fallback for builtins not natively handled by
/// the BC VM (e.g. `assert_eq`, `llm_complete`, stdlib natives).  The trait is
/// `Send + Sync`-friendly so future async use is open.
pub trait BuiltinDispatcher: Send + Sync {
    /// Call the named builtin with `args`, returning the result value.
    ///
    /// Return `Err(String)` to signal a runtime error from the builtin.
    fn call(&mut self, name: &str, args: &[Value]) -> Result<Value, String>;
}

/// A no-op dispatcher that errors on every call.  Used by `run_main`.
pub struct NoopDispatcher;

impl BuiltinDispatcher for NoopDispatcher {
    fn call(&mut self, name: &str, _args: &[Value]) -> Result<Value, String> {
        Err(format!("no dispatcher registered; cannot call builtin `{name}`"))
    }
}

// ─── VM ──────────────────────────────────────────────────────────────────────

/// Bytecode VM.  Create one per program execution; do not reuse across runs.
pub struct Vm<'p> {
    program: &'p Program,
    stack: Vec<Value>,
    /// Captured stdout for testing.
    pub stdout: String,
    /// When true, don't write to real stdout.
    pub capture_only: bool,
    /// Dynamic builtin dispatcher (boxed for lifetime flexibility).
    dispatcher: Box<dyn BuiltinDispatcher>,
}

impl<'p> Vm<'p> {
    pub fn new(program: &'p Program) -> Self {
        Self {
            program,
            stack: Vec::with_capacity(256),
            stdout: String::new(),
            capture_only: false,
            dispatcher: Box::new(NoopDispatcher),
        }
    }

    /// Create a VM with a custom `BuiltinDispatcher` for dynamic builtin calls.
    pub fn with_dispatcher(program: &'p Program, dispatcher: Box<dyn BuiltinDispatcher>) -> Self {
        Self {
            program,
            stack: Vec::with_capacity(256),
            stdout: String::new(),
            capture_only: false,
            dispatcher,
        }
    }

    /// Run the entry-point function and return its result.
    pub fn run(&mut self) -> Result<Value, VmError> {
        let entry = self.program.entry as usize;
        self.call_fn(entry, 0)
    }

    /// Execute a function by index, consuming `argc` values already on the
    /// operand stack as its arguments (innermost = last pushed).
    fn call_fn(&mut self, fn_idx: usize, argc: usize) -> Result<Value, VmError> {
        let bf = &self.program.fns[fn_idx];
        if argc != bf.arity as usize {
            return Err(VmError::ArityMismatch {
                name: bf.name.clone(),
                expected: bf.arity as usize,
                got: argc,
            });
        }

        // Pop arguments from the stack (they were pushed left-to-right).
        let arg_start = self.stack.len().checked_sub(argc)
            .ok_or(VmError::StackUnderflow)?;
        let args: Vec<Value> = self.stack.drain(arg_start..).collect();

        // Set up locals: args occupy slots 0..arity; rest are Unit.
        let mut locals: Vec<Value> = args;
        locals.resize(bf.n_locals as usize, Value::Unit);

        self.exec_code(bf.code.clone(), locals)
    }

    /// Execute a closure, injecting captured values into the locals array.
    fn call_closure(&mut self, fn_idx: u32, captured: Vec<Value>, argc: usize) -> Result<Value, VmError> {
        let bf = &self.program.fns[fn_idx as usize];
        if argc != bf.arity as usize {
            return Err(VmError::ArityMismatch {
                name: bf.name.clone(),
                expected: bf.arity as usize,
                got: argc,
            });
        }

        // Pop arguments (last pushed = last param).
        let arg_start = self.stack.len().checked_sub(argc)
            .ok_or(VmError::StackUnderflow)?;
        let args: Vec<Value> = self.stack.drain(arg_start..).collect();

        // Locals layout: params first, then captured slots, padded to n_locals.
        let mut locals: Vec<Value> = args;
        locals.extend(captured);
        locals.resize(bf.n_locals as usize, Value::Unit);

        self.exec_code(bf.code.clone(), locals)
    }

    /// Core interpreter loop.
    fn exec_code(&mut self, code: Vec<Op>, mut locals: Vec<Value>) -> Result<Value, VmError> {
        let mut ip: usize = 0;
        loop {
            let op = code.get(ip).ok_or(VmError::IpOutOfBounds)?;
            match op {
                Op::PushInt(n)   => self.stack.push(Value::Int(*n)),
                Op::PushBool(b)  => self.stack.push(Value::Bool(*b)),
                Op::PushFloat(f) => self.stack.push(Value::Float(*f)),
                Op::PushStr(i)   => {
                    let s = match self.program.constants.get(*i as usize) {
                        Some(crate::vm::Constant::Str(s)) => s.clone(),
                        None => return Err(VmError::BadConstantIndex(*i)),
                    };
                    self.stack.push(Value::Str(s));
                }
                Op::PushUnit    => self.stack.push(Value::Unit),

                Op::LoadLocal(idx) => {
                    let v = locals.get(*idx as usize)
                        .cloned()
                        .ok_or(VmError::BadLocalIndex(*idx))?;
                    self.stack.push(v);
                }
                Op::StoreLocal(idx) => {
                    let v = self.pop()?;
                    let slot = locals.get_mut(*idx as usize)
                        .ok_or(VmError::BadLocalIndex(*idx))?;
                    *slot = v;
                }

                Op::Add  => { let (a, b) = self.pop2()?; self.stack.push(add(a, b)?); }
                Op::Sub  => { let (a, b) = self.pop2()?; self.stack.push(sub(a, b)?); }
                Op::Mul  => { let (a, b) = self.pop2()?; self.stack.push(mul(a, b)?); }
                Op::Div  => { let (a, b) = self.pop2()?; self.stack.push(div(a, b)?); }
                Op::Mod  => { let (a, b) = self.pop2()?; self.stack.push(rem(a, b)?); }

                Op::Eq   => { let (a, b) = self.pop2()?; self.stack.push(Value::Bool(val_eq(&a, &b))); }
                Op::Neq  => { let (a, b) = self.pop2()?; self.stack.push(Value::Bool(!val_eq(&a, &b))); }
                Op::Lt   => { let (a, b) = self.pop2()?; self.stack.push(Value::Bool(cmp_lt(&a, &b)?)); }
                Op::Le   => { let (a, b) = self.pop2()?; self.stack.push(Value::Bool(cmp_le(&a, &b)?)); }
                Op::Gt   => { let (a, b) = self.pop2()?; self.stack.push(Value::Bool(cmp_gt(&a, &b)?)); }
                Op::Ge   => { let (a, b) = self.pop2()?; self.stack.push(Value::Bool(cmp_ge(&a, &b)?)); }

                Op::And  => { let (a, b) = self.pop2_bool()?; self.stack.push(Value::Bool(a && b)); }
                Op::Or   => { let (a, b) = self.pop2_bool()?; self.stack.push(Value::Bool(a || b)); }
                Op::Implies => {
                    let (a, b) = self.pop2_bool()?;
                    self.stack.push(Value::Bool(!a || b));
                }
                Op::Not  => {
                    let v = self.pop()?;
                    match v {
                        Value::Bool(b) => self.stack.push(Value::Bool(!b)),
                        _ => return Err(VmError::TypeError("Not: expected Bool".into())),
                    }
                }
                Op::Neg  => {
                    let v = self.pop()?;
                    match v {
                        Value::Int(n)   => self.stack.push(Value::Int(-n)),
                        Value::Float(f) => self.stack.push(Value::Float(-f)),
                        _ => return Err(VmError::TypeError("Neg: expected Int or Float".into())),
                    }
                }
                Op::Concat => {
                    let (a, b) = self.pop2()?;
                    match (a, b) {
                        (Value::Str(s1), Value::Str(s2)) => self.stack.push(Value::Str(format!("{s1}{s2}"))),
                        (Value::List(mut l1), Value::List(l2)) => {
                            l1.extend(l2);
                            self.stack.push(Value::List(l1));
                        }
                        _ => return Err(VmError::TypeError("Concat: expected Str/List pair".into())),
                    }
                }

                Op::ToStr => {
                    let v = self.pop()?;
                    self.stack.push(Value::Str(v.display()));
                }

                Op::Jump(offset) => {
                    let new_ip = (ip as i32 + 1 + *offset) as usize;
                    ip = new_ip;
                    continue;
                }
                Op::JumpIfFalse(offset) => {
                    let v = self.pop()?;
                    let b = match v {
                        Value::Bool(b) => b,
                        _ => return Err(VmError::TypeError("JumpIfFalse: expected Bool".into())),
                    };
                    if !b {
                        let new_ip = (ip as i32 + 1 + *offset) as usize;
                        ip = new_ip;
                        continue;
                    }
                }

                Op::Call { fn_idx: fi, argc: ac } => {
                    let result = self.call_fn(*fi as usize, *ac as usize)?;
                    self.stack.push(result);
                }

                Op::CallBuiltin { id, argc: ac } => {
                    let result = self.call_builtin(*id, *ac as usize)?;
                    self.stack.push(result);
                }

                Op::CallBuiltinDyn { name_idx, argc: ac } => {
                    let name = self.get_constant_str(*name_idx)?;
                    let argc = *ac as usize;
                    let start = self.stack.len().checked_sub(argc)
                        .ok_or(VmError::StackUnderflow)?;
                    let args: Vec<Value> = self.stack.drain(start..).collect();
                    let result = self.dispatcher.call(&name, &args)
                        .map_err(VmError::User)?;
                    self.stack.push(result);
                }

                Op::CallClosure { argc: ac } => {
                    let argc = *ac as usize;
                    // Stack: [... | closure | arg0 | ... | argN-1]
                    // closure is at stack[len - argc - 1]
                    let closure_idx = self.stack.len()
                        .checked_sub(argc + 1)
                        .ok_or(VmError::StackUnderflow)?;
                    let closure = self.stack.remove(closure_idx);
                    match closure {
                        Value::Closure { fn_idx, captured } => {
                            let result = self.call_closure(fn_idx, captured, argc)?;
                            self.stack.push(result);
                        }
                        other => return Err(VmError::TypeError(
                            format!("CallClosure: expected Closure, got {:?}", other.display())
                        )),
                    }
                }

                Op::Ret => {
                    let v = self.pop()?;
                    return Ok(v);
                }

                Op::Pop => {
                    self.pop()?;
                }

                Op::Dup => {
                    let v = self.stack.last()
                        .cloned()
                        .ok_or(VmError::StackUnderflow)?;
                    self.stack.push(v);
                }

                Op::MakeClosure { fn_idx, captured: cap_slots } => {
                    let captured: Vec<Value> = cap_slots.iter()
                        .map(|&slot| {
                            locals.get(slot as usize)
                                .cloned()
                                .ok_or(VmError::BadLocalIndex(slot))
                        })
                        .collect::<Result<_, _>>()?;
                    self.stack.push(Value::Closure { fn_idx: *fn_idx, captured });
                }

                Op::Ctor { name_idx, argc: ac } => {
                    let count = *ac as usize;
                    let name = self.get_constant_str(*name_idx)?;
                    if self.stack.len() < count {
                        return Err(VmError::StackUnderflow);
                    }
                    let start = self.stack.len() - count;
                    let args: Vec<Value> = self.stack.drain(start..).collect();
                    self.stack.push(Value::Ctor { name, args });
                }

                Op::MatchCtor { name_idx, expect_arity, jump_if_miss: _ } => {
                    // MatchCtor peeks at TOS (does not pop), tests name+arity,
                    // and pushes a Bool result.  The jump is handled externally
                    // by a JumpIfFalse that follows.  This op does NOT jump itself —
                    // the `jump_if_miss` field is reserved for future optimisation.
                    let expected_name = self.get_constant_str(*name_idx)?;
                    let top = self.stack.last()
                        .ok_or(VmError::StackUnderflow)?;
                    let matches = match top {
                        Value::Ctor { name, args } => {
                            name == &expected_name && args.len() == *expect_arity as usize
                        }
                        _ => false,
                    };
                    self.stack.push(Value::Bool(matches));
                }

                Op::CtorField(field_idx) => {
                    let top = self.stack.last()
                        .ok_or(VmError::StackUnderflow)?;
                    match top {
                        Value::Ctor { args, .. } => {
                            let v = args.get(*field_idx as usize)
                                .cloned()
                                .ok_or_else(|| VmError::TypeError(
                                    format!("CtorField: index {} out of bounds", field_idx)
                                ))?;
                            self.stack.push(v);
                        }
                        _ => return Err(VmError::TypeError("CtorField: expected Ctor".into())),
                    }
                }

                Op::MakeTuple(n) => {
                    let count = *n as usize;
                    if self.stack.len() < count {
                        return Err(VmError::StackUnderflow);
                    }
                    let start = self.stack.len() - count;
                    let elts: Vec<Value> = self.stack.drain(start..).collect();
                    self.stack.push(Value::Tuple(elts));
                }

                Op::TupleGet(idx) => {
                    let tup = self.pop()?;
                    match tup {
                        Value::Tuple(vs) => {
                            let v = vs.get(*idx as usize)
                                .cloned()
                                .ok_or_else(|| VmError::TypeError(
                                    format!("TupleGet: index {} out of bounds", idx)
                                ))?;
                            self.stack.push(v);
                        }
                        _ => return Err(VmError::TypeError("TupleGet: expected Tuple".into())),
                    }
                }

                Op::MakeRecord { field_names } => {
                    let count = field_names.len();
                    if self.stack.len() < count {
                        return Err(VmError::StackUnderflow);
                    }
                    let start = self.stack.len() - count;
                    let vals: Vec<Value> = self.stack.drain(start..).collect();
                    let mut fields: Vec<(String, Value)> = Vec::with_capacity(count);
                    for (name_idx, val) in field_names.iter().zip(vals) {
                        let name = self.get_constant_str(*name_idx)?;
                        fields.push((name, val));
                    }
                    self.stack.push(Value::Record(fields));
                }

                Op::FieldGet(name_idx) => {
                    let field_name = self.get_constant_str(*name_idx)?;
                    let rec = self.pop()?;
                    match rec {
                        Value::Record(fields) => {
                            let v = fields.into_iter()
                                .find(|(k, _)| k == &field_name)
                                .map(|(_, v)| v)
                                .ok_or_else(|| VmError::TypeError(
                                    format!("FieldGet: field `{field_name}` not found")
                                ))?;
                            self.stack.push(v);
                        }
                        _ => return Err(VmError::TypeError(
                            format!("FieldGet: expected Record, got {}", rec.display())
                        )),
                    }
                }

                Op::MakeList(n) => {
                    let count = *n as usize;
                    if self.stack.len() < count {
                        return Err(VmError::StackUnderflow);
                    }
                    let start = self.stack.len() - count;
                    let elts: Vec<Value> = self.stack.drain(start..).collect();
                    self.stack.push(Value::List(elts));
                }

                Op::MakeConfident => {
                    // Stack layout: [..., inner, p_float] — pop p first (top), then inner.
                    let p_val = self.pop()?;
                    let p = match p_val {
                        Value::Float(f) => f,
                        Value::Int(n) => n as f64,
                        _ => return Err(VmError::TypeError(
                            "MakeConfident: p must be Float or Int".into()
                        )),
                    };
                    let inner = self.pop()?;
                    self.stack.push(Value::Confident { inner: Box::new(inner), p });
                }

                Op::Index => {
                    let idx = self.pop()?;
                    let container = self.pop()?;
                    let i = idx.as_int()
                        .ok_or_else(|| VmError::TypeError("Index: index must be Int".into()))?;
                    match container {
                        Value::List(vs) => {
                            let v = vs.get(i as usize)
                                .cloned()
                                .ok_or_else(|| VmError::TypeError(
                                    format!("Index: index {i} out of bounds (len {})", vs.len())
                                ))?;
                            self.stack.push(v);
                        }
                        Value::Tuple(vs) => {
                            let v = vs.get(i as usize)
                                .cloned()
                                .ok_or_else(|| VmError::TypeError(
                                    format!("Index: index {i} out of bounds (len {})", vs.len())
                                ))?;
                            self.stack.push(v);
                        }
                        _ => return Err(VmError::TypeError(
                            "Index: expected List or Tuple".into()
                        )),
                    }
                }
            }
            ip += 1;
        }
    }

    fn get_constant_str(&self, idx: u32) -> Result<String, VmError> {
        match self.program.constants.get(idx as usize) {
            Some(Constant::Str(s)) => Ok(s.clone()),
            None => Err(VmError::BadConstantIndex(idx)),
        }
    }

    fn call_builtin(&mut self, id: u16, argc: usize) -> Result<Value, VmError> {
        let start = self.stack.len().checked_sub(argc)
            .ok_or(VmError::StackUnderflow)?;
        let args: Vec<Value> = self.stack.drain(start..).collect();

        let bid = match id {
            0 => BuiltinId::Print,
            1 => BuiltinId::Str,
            2 => BuiltinId::Int,
            3 => BuiltinId::Len,
            4 => BuiltinId::Abs,
            5 => BuiltinId::Max,
            6 => BuiltinId::Min,
            _ => return Err(VmError::UnknownBuiltin(id)),
        };

        crate::builtins::call(bid, &args, self)
    }

    #[inline]
    fn pop(&mut self) -> Result<Value, VmError> {
        self.stack.pop().ok_or(VmError::StackUnderflow)
    }

    /// Pop two values: returns (first_pushed, second_pushed) — i.e. pops rhs
    /// first (top), then lhs.
    #[inline]
    fn pop2(&mut self) -> Result<(Value, Value), VmError> {
        let rhs = self.stack.pop().ok_or(VmError::StackUnderflow)?;
        let lhs = self.stack.pop().ok_or(VmError::StackUnderflow)?;
        Ok((lhs, rhs))
    }

    #[inline]
    fn pop2_bool(&mut self) -> Result<(bool, bool), VmError> {
        let (a, b) = self.pop2()?;
        let av = a.as_bool().ok_or_else(|| VmError::TypeError("expected Bool (lhs)".into()))?;
        let bv = b.as_bool().ok_or_else(|| VmError::TypeError("expected Bool (rhs)".into()))?;
        Ok((av, bv))
    }
}

// ─── arithmetic helpers ───────────────────────────────────────────────────────

fn add(a: Value, b: Value) -> Result<Value, VmError> {
    match (a, b) {
        (Value::Int(x), Value::Int(y))     => Ok(Value::Int(x + y)),
        (Value::Float(x), Value::Float(y)) => Ok(Value::Float(x + y)),
        _ => Err(VmError::TypeError("Add: expected Int or Float".into())),
    }
}
fn sub(a: Value, b: Value) -> Result<Value, VmError> {
    match (a, b) {
        (Value::Int(x), Value::Int(y))     => Ok(Value::Int(x - y)),
        (Value::Float(x), Value::Float(y)) => Ok(Value::Float(x - y)),
        _ => Err(VmError::TypeError("Sub: expected Int or Float".into())),
    }
}
fn mul(a: Value, b: Value) -> Result<Value, VmError> {
    match (a, b) {
        (Value::Int(x), Value::Int(y))     => Ok(Value::Int(x * y)),
        (Value::Float(x), Value::Float(y)) => Ok(Value::Float(x * y)),
        _ => Err(VmError::TypeError("Mul: expected Int or Float".into())),
    }
}
fn div(a: Value, b: Value) -> Result<Value, VmError> {
    match (a, b) {
        (Value::Int(_), Value::Int(0))     => Err(VmError::DivByZero),
        (Value::Int(x), Value::Int(y))     => Ok(Value::Int(x / y)),
        (Value::Float(x), Value::Float(y)) => Ok(Value::Float(x / y)),
        _ => Err(VmError::TypeError("Div: expected Int or Float".into())),
    }
}
fn rem(a: Value, b: Value) -> Result<Value, VmError> {
    match (a, b) {
        (Value::Int(_), Value::Int(0)) => Err(VmError::DivByZero),
        (Value::Int(x), Value::Int(y)) => Ok(Value::Int(x % y)),
        _ => Err(VmError::TypeError("Mod: expected Int".into())),
    }
}

/// Equality that handles Float NaN correctly (NaN != NaN).
fn val_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Float(x), Value::Float(y)) => x == y,
        _ => a == b,
    }
}

fn cmp_lt(a: &Value, b: &Value) -> Result<bool, VmError> {
    match (a, b) {
        (Value::Int(x), Value::Int(y))     => Ok(x < y),
        (Value::Float(x), Value::Float(y)) => Ok(x < y),
        _ => Err(VmError::TypeError("Lt: expected Int or Float".into())),
    }
}
fn cmp_le(a: &Value, b: &Value) -> Result<bool, VmError> {
    match (a, b) {
        (Value::Int(x), Value::Int(y))     => Ok(x <= y),
        (Value::Float(x), Value::Float(y)) => Ok(x <= y),
        _ => Err(VmError::TypeError("Le: expected Int or Float".into())),
    }
}
fn cmp_gt(a: &Value, b: &Value) -> Result<bool, VmError> {
    match (a, b) {
        (Value::Int(x), Value::Int(y))     => Ok(x > y),
        (Value::Float(x), Value::Float(y)) => Ok(x > y),
        _ => Err(VmError::TypeError("Gt: expected Int or Float".into())),
    }
}
fn cmp_ge(a: &Value, b: &Value) -> Result<bool, VmError> {
    match (a, b) {
        (Value::Int(x), Value::Int(y))     => Ok(x >= y),
        (Value::Float(x), Value::Float(y)) => Ok(x >= y),
        _ => Err(VmError::TypeError("Ge: expected Int or Float".into())),
    }
}

//! Built-in function implementations for the bytecode VM.
//!
//! Mirrors the semantics of `aether_eval::builtins` but operates on
//! `crate::vm::Value` (no provenance overhead).

use crate::error::VmError;
use crate::op::BuiltinId;
use crate::vm::{Value, Vm};

/// Dispatch a built-in call.  `args` have already been popped from the stack
/// by the caller; the return value will be pushed back.
pub fn call(id: BuiltinId, args: &[Value], vm: &mut Vm<'_>) -> Result<Value, VmError> {
    match id {
        BuiltinId::Print => {
            let s = args.first().map(Value::display).unwrap_or_default();
            if !vm.capture_only {
                println!("{s}");
            }
            vm.stdout.push_str(&s);
            vm.stdout.push('\n');
            Ok(Value::Unit)
        }

        BuiltinId::Str => {
            // display() handles all value types including Float.
            let s = args.first().map(Value::display).unwrap_or_default();
            Ok(Value::Str(s))
        }

        BuiltinId::Int => {
            let s = args
                .first()
                .and_then(Value::as_str)
                .ok_or_else(|| VmError::TypeError("int(): expected Str".into()))?;
            let n: i64 = s
                .trim()
                .parse()
                .map_err(|_| VmError::User(format!("int(): not a number: {s:?}")))?;
            Ok(Value::Int(n))
        }

        BuiltinId::Len => {
            match args.first() {
                Some(Value::Str(s))   => Ok(Value::Int(s.chars().count() as i64)),
                Some(Value::List(vs)) => Ok(Value::Int(vs.len() as i64)),
                _ => Err(VmError::TypeError("len(): expected Str or List".into())),
            }
        }

        BuiltinId::Abs => {
            match args.first() {
                Some(Value::Int(n))   => Ok(Value::Int(n.abs())),
                Some(Value::Float(f)) => Ok(Value::Float(f.abs())),
                _ => Err(VmError::TypeError("abs(): expected Int or Float".into())),
            }
        }

        BuiltinId::Max => {
            match (args.first(), args.get(1)) {
                (Some(Value::Int(a)), Some(Value::Int(b)))     => Ok(Value::Int((*a).max(*b))),
                (Some(Value::Float(a)), Some(Value::Float(b))) => Ok(Value::Float(a.max(*b))),
                _ => Err(VmError::TypeError("max(): expected two Int or two Float args".into())),
            }
        }

        BuiltinId::Min => {
            match (args.first(), args.get(1)) {
                (Some(Value::Int(a)), Some(Value::Int(b)))     => Ok(Value::Int((*a).min(*b))),
                (Some(Value::Float(a)), Some(Value::Float(b))) => Ok(Value::Float(a.min(*b))),
                _ => Err(VmError::TypeError("min(): expected two Int or two Float args".into())),
            }
        }
    }
}

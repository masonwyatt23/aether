//! Aether bytecode compiler and stack-based VM.
//!
//! # Quick start
//!
//! ```rust,no_run
//! use aether_bc::{compile_module, run_main};
//! use aether_parser::parse_module;
//! use aether_ast::FileId;
//!
//! let src = "fn main() -> Int effects {} { 1 + 2 }";
//! let module = parse_module(FileId(0), src).unwrap();
//! let program = compile_module(&module).unwrap();
//! let value = run_main(&program).unwrap();
//! assert_eq!(value.as_int(), Some(3));
//! ```

#![allow(clippy::module_inception)]

pub mod builtins;
pub mod compile;
pub mod error;
pub mod op;
pub mod vm;

pub use compile::compile_module;
pub use error::{CompileError, VmError};
pub use vm::{BuiltinDispatcher, BytecodeFn, Constant, NoopDispatcher, Program, Value, Vm};

// ─── magic header ─────────────────────────────────────────────────────────────

/// Magic bytes that prefix every `.aebc` file (8 bytes).
/// Format: b"AEBC\0\0\0\x01" — ASCII tag + 3 reserved zero bytes + version 1.
pub const AEBC_MAGIC: &[u8; 8] = b"AEBC\0\0\0\x01";

// ─── (de)serialization ───────────────────────────────────────────────────────

/// Serialize a `Program` to bytes.
///
/// Format: `AEBC_MAGIC` (8 bytes) || bincode-encoded `Program`.
pub fn serialize_program(p: &Program) -> Vec<u8> {
    let payload = bincode::serialize(p).expect("bincode serialization is infallible for Program");
    let mut out = Vec::with_capacity(8 + payload.len());
    out.extend_from_slice(AEBC_MAGIC);
    out.extend_from_slice(&payload);
    out
}

/// Deserialize a `Program` from bytes produced by [`serialize_program`].
///
/// Returns `Err` if the magic header is missing/wrong or the bincode payload is
/// malformed.
pub fn deserialize_program(bytes: &[u8]) -> Result<Program, bincode::Error> {
    if bytes.len() < 8 || &bytes[..8] != AEBC_MAGIC {
        return Err(Box::new(bincode::ErrorKind::Custom(
            "missing or invalid AEBC magic header".into(),
        )));
    }
    bincode::deserialize(&bytes[8..])
}

/// Run the entry-point (`main`) function of a compiled `Program`.
///
/// Stdout is written to the host terminal.  For tests that need captured
/// output, construct a [`Vm`] directly and set `capture_only = true`.
/// Unknown dynamic builtins will error at runtime — use
/// [`run_main_with_dispatcher`] to wire in a trampoline.
pub fn run_main(program: &Program) -> Result<Value, VmError> {
    let mut vm = Vm::new(program);
    vm.run()
}

/// Run the entry-point function with a custom [`BuiltinDispatcher`] that
/// handles `CallBuiltinDyn` ops.  Use this when the program calls builtins
/// not natively implemented in the BC VM (e.g. stdlib natives, `assert_eq`,
/// `http_get`).
pub fn run_main_with_dispatcher(
    program: &Program,
    dispatcher: Box<dyn BuiltinDispatcher>,
) -> Result<Value, VmError> {
    let mut vm = Vm::with_dispatcher(program, dispatcher);
    vm.run()
}

// ─── unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use aether_ast::FileId;
    use aether_parser::parse_module;

    fn compile_run(src: &str) -> Value {
        let m = parse_module(FileId(0), src).expect("parse failed");
        let prog = compile_module(&m).expect("compile failed");
        run_main(&prog).expect("runtime error")
    }

    fn compile_run_capture(src: &str) -> (Value, String) {
        let m = parse_module(FileId(0), src).expect("parse failed");
        let prog = compile_module(&m).expect("compile failed");
        let mut vm = Vm::new(&prog);
        vm.capture_only = true;
        let v = vm.run().expect("runtime error");
        (v, vm.stdout)
    }

    // ── arithmetic ───────────────────────────────────────────────────────────

    #[test]
    fn pure_arith() {
        // 1 + 2 * 3 should respect precedence → 7
        let v = compile_run("fn main() -> Int effects {} { 1 + 2 * 3 }");
        assert_eq!(v.as_int(), Some(7));
    }

    #[test]
    fn arith_sub_div_mod() {
        let v = compile_run("fn main() -> Int effects {} { 10 - 3 }");
        assert_eq!(v.as_int(), Some(7));
        let v = compile_run("fn main() -> Int effects {} { 10 / 2 }");
        assert_eq!(v.as_int(), Some(5));
        let v = compile_run("fn main() -> Int effects {} { 10 % 3 }");
        assert_eq!(v.as_int(), Some(1));
    }

    // ── float arithmetic ─────────────────────────────────────────────────────

    #[test]
    fn float_arithmetic() {
        // Basic float ops match expected values.
        let v = compile_run("fn main() -> Float effects {} { 1.0 + 2.5 }");
        let f = v.as_float().expect("expected Float");
        assert!((f - 3.5).abs() < 1e-10, "expected 3.5, got {f}");

        let v = compile_run("fn main() -> Float effects {} { 3.0 * 2.0 }");
        let f = v.as_float().expect("expected Float");
        assert!((f - 6.0).abs() < 1e-10, "expected 6.0, got {f}");

        // Float display matches tree-walker: f.to_string()
        assert_eq!(Value::Float(1.0_f64).display(), "1");
        assert_eq!(Value::Float(0.5_f64).display(), "0.5");
        assert_eq!(Value::Float(-2.5_f64).display(), "-2.5");
    }

    // ── string interpolation ─────────────────────────────────────────────────

    #[test]
    fn string_interpolation_result() {
        let src = r#"
fn greet(name: Str) -> Str effects {} {
    "hello ${name}!"
}
fn main() -> Str effects {} { greet("world") }
"#;
        let v = compile_run(src);
        assert_eq!(v.as_str(), Some("hello world!"), "got {:?}", v);
    }

    #[test]
    fn string_interpolation_with_int_expr() {
        let src = r#"
fn main() -> Str effects {} {
    let n = 42
    "the answer is ${str(n)}"
}
"#;
        let v = compile_run(src);
        assert_eq!(v.as_str(), Some("the answer is 42"), "got {:?}", v);
    }

    // ── fallback dispatcher ───────────────────────────────────────────────────

    #[test]
    fn fallback_dispatcher_hit() {
        // A "fake" builtin not in BuiltinId: assert_eq-like.
        // We wire a dispatcher that returns "dispatched:<name>" as a Str.
        struct TestDispatcher;
        impl BuiltinDispatcher for TestDispatcher {
            fn call(&mut self, name: &str, _args: &[Value]) -> Result<Value, String> {
                Ok(Value::Str(format!("dispatched:{name}")))
            }
        }

        let src = r#"fn main() -> Str effects {} { assert_eq(1, 1) }"#;
        let m = parse_module(FileId(0), src).expect("parse");
        let prog = compile_module(&m).expect("compile");
        let result = run_main_with_dispatcher(&prog, Box::new(TestDispatcher));
        let v = result.expect("runtime error");
        match v.as_str() {
            Some(s) => assert_eq!(s, "dispatched:assert_eq"),
            _ => panic!("expected Str, got {:?}", v),
        }
    }

    // ── Float in ADT ─────────────────────────────────────────────────────────

    #[test]
    fn float_in_adt() {
        let src = r#"
type Shape = Circle(Float) | Square(Float)

fn area(s: Shape) -> Float effects {} {
    match s with {
        Circle(r)    => r * r,
        Square(side) => side * side
    }
}
fn main() -> Float effects {} { area(Circle(3.0)) }
"#;
        let v = compile_run(src);
        let f = v.as_float().expect("expected Float");
        assert!((f - 9.0).abs() < 1e-10, "expected 9.0, got {f}");
    }

    // ── fib via recursion ────────────────────────────────────────────────────

    const FIB_SRC: &str = r#"
fn fib(n: Int) -> Int effects {} {
    if n < 2 then n else fib(n - 1) + fib(n - 2)
}
fn main() -> Int effects {} { fib(10) }
"#;

    #[test]
    fn fib_10() {
        let v = compile_run(FIB_SRC);
        assert_eq!(v.as_int(), Some(55));
    }

    /// Verify fib(10) matches aether-eval's result exactly.
    #[test]
    fn fib_10_matches_eval() {
        use aether_eval::Runtime;
        let m = parse_module(FileId(0), FIB_SRC).unwrap();
        let mut rt = Runtime::new(m.clone());
        rt.capture_only = true;
        let eval_val = rt.run_main().unwrap();
        let eval_n = eval_val.as_int().unwrap();

        let prog = compile_module(&m).unwrap();
        let bc_n = run_main(&prog).unwrap().as_int().unwrap();

        assert_eq!(bc_n, eval_n, "bc={bc_n} eval={eval_n}");
    }

    // ── mutual recursion ─────────────────────────────────────────────────────

    #[test]
    fn mutual_recursion() {
        // is_even and is_odd call each other; is_even(4) == true.
        let src = r#"
fn is_even(n: Int) -> Bool effects {} {
    if n == 0 then true else is_odd(n - 1)
}
fn is_odd(n: Int) -> Bool effects {} {
    if n == 0 then false else is_even(n - 1)
}
fn main() -> Bool effects {} { is_even(4) }
"#;
        let v = compile_run(src);
        assert_eq!(v.as_bool(), Some(true));
    }

    // ── print / stdout capture ───────────────────────────────────────────────

    #[test]
    fn print_captures_stdout() {
        let src = r#"fn main() -> Unit effects {IO} { print("hello bytecode") }"#;
        let (v, out) = compile_run_capture(src);
        assert!(matches!(v, Value::Unit), "expected Unit, got {v:?}");
        assert!(out.contains("hello bytecode"), "stdout was: {out:?}");
    }

    // ── builtins: min ────────────────────────────────────────────────────────

    #[test]
    fn builtin_min_end_to_end() {
        // min(3, 7) == 3  — tests builtin through full compile+run pipeline
        let src = r#"
fn main() -> Bool effects {} { min(3, 7) == 3 }
"#;
        let v = compile_run(src);
        assert_eq!(v.as_bool(), Some(true));
    }

    #[test]
    fn builtin_max() {
        let src = r#"fn main() -> Int effects {} { max(5, 12) }"#;
        assert_eq!(compile_run(src).as_int(), Some(12));
    }

    #[test]
    fn builtin_abs() {
        let src = r#"fn main() -> Int effects {} { abs(-42) }"#;
        assert_eq!(compile_run(src).as_int(), Some(42));
    }

    #[test]
    fn builtin_str_conversion() {
        let src = r#"fn main() -> Str effects {} { str(99) }"#;
        assert_eq!(compile_run(src).as_str(), Some("99"));
    }

    // ── let bindings ─────────────────────────────────────────────────────────

    #[test]
    fn let_binding() {
        let src = r#"fn main() -> Int effects {} {
            let x = 10;
            let y = x + 5;
            y
        }"#;
        assert_eq!(compile_run(src).as_int(), Some(15));
    }

    // ── if/else ──────────────────────────────────────────────────────────────

    #[test]
    fn if_else_true_branch() {
        let src = r#"fn main() -> Int effects {} { if true then 1 else 2 }"#;
        assert_eq!(compile_run(src).as_int(), Some(1));
    }

    #[test]
    fn if_else_false_branch() {
        let src = r#"fn main() -> Int effects {} { if false then 1 else 2 }"#;
        assert_eq!(compile_run(src).as_int(), Some(2));
    }

    // ── user function calls ───────────────────────────────────────────────────

    #[test]
    fn user_fn_call() {
        let src = r#"
fn inc(x: Int) -> Int effects {} { x + 1 }
fn main() -> Int effects {} { inc(41) }
"#;
        assert_eq!(compile_run(src).as_int(), Some(42));
    }

    // ── closures ─────────────────────────────────────────────────────────────

    #[test]
    fn closure_captures_local() {
        // Lambda captures `k` from the enclosing scope; calling it with 35 should
        // return 42.  Compared to tree-walker output.
        use aether_eval::Runtime;
        let src = r#"
fn main() -> Int effects {} {
    let k = 7
    let add_k = fn(x: Int) -> Int => x + k
    add_k(35)
}
"#;
        let m = parse_module(FileId(0), src).unwrap();

        // Tree-walker result.
        let mut rt = Runtime::new(m.clone());
        rt.capture_only = true;
        let eval_val = rt.run_main().unwrap();
        let eval_n = eval_val.as_int().unwrap();

        // Bytecode result.
        let prog = compile_module(&m).unwrap();
        let bc_val = run_main(&prog).unwrap();
        let bc_n = bc_val.as_int().unwrap();

        assert_eq!(bc_n, eval_n, "closure capture: bc={bc_n} eval={eval_n}");
        assert_eq!(bc_n, 42);
    }

    // ── match expressions ─────────────────────────────────────────────────────

    #[test]
    fn match_int_arms() {
        // match n { 0 => "zero", _ => "other" }
        use aether_eval::Runtime;
        let src_zero = r#"
fn classify(n: Int) -> Str effects {} {
    match n with { 0 => "zero", _ => "other" }
}
fn main() -> Str effects {} { classify(0) }
"#;
        let src_other = r#"
fn classify(n: Int) -> Str effects {} {
    match n with { 0 => "zero", _ => "other" }
}
fn main() -> Str effects {} { classify(5) }
"#;

        // Compare to tree-walker.
        for src in &[src_zero, src_other] {
            let m = parse_module(FileId(0), src).unwrap();
            let mut rt = Runtime::new(m.clone());
            rt.capture_only = true;
            let eval_s = rt.run_main().unwrap().as_str().unwrap().to_string();

            let prog = compile_module(&m).unwrap();
            let bc_s = run_main(&prog).unwrap().as_str().unwrap().to_string();

            assert_eq!(bc_s, eval_s, "match_int: bc={bc_s} eval={eval_s}");
        }
    }

    #[test]
    fn match_ctor_pattern() {
        // Pattern matching on Circle(r) should extract r.
        let src = r#"
type Shape = Circle(Int) | Square(Int)

fn area(s: Shape) -> Int effects {} {
    match s with {
        Circle(r) => r * r,
        Square(side) => side * side
    }
}
fn main() -> Int effects {} { area(Circle(5)) }
"#;
        let v = compile_run(src);
        assert_eq!(
            v.as_int(),
            Some(25),
            "Circle(5) area should be 25, got {:?}",
            v
        );
    }

    // ── tuples ────────────────────────────────────────────────────────────────

    #[test]
    fn tuple_field_access() {
        // Tuple construction and extraction via match.
        let src = r#"
fn fst(t: (Int, Int)) -> Int effects {} {
    match t with {
        (a, _) => a
    }
}
fn snd(t: (Int, Int)) -> Int effects {} {
    match t with {
        (_, b) => b
    }
}
fn main() -> Int effects {} {
    let t = (10, 20)
    fst(t) + snd(t)
}
"#;
        let v = compile_run(src);
        assert_eq!(
            v.as_int(),
            Some(30),
            "tuple fst+snd should be 30, got {:?}",
            v
        );
    }

    // ── example files ────────────────────────────────────────────────────────

    #[test]
    fn example_07_closures_and_match_compiles_and_runs() {
        let src = r#"
fn classify(n: Int) -> Str effects {} {
    match n with {
        0       => "zero",
        x if x > 0 => "positive",
        _       => "negative"
    }
}

fn main() -> Unit effects {IO} {
    let k = 7
    let add_k = fn(x: Int) -> Int => x + k
    let y = add_k(35)
    print(str(y))
    print(classify(y))
    print(classify(0))
    print(classify(-3))
}
"#;
        let m = parse_module(FileId(0), src).expect("parse failed");
        let prog = compile_module(&m).expect("compile failed");
        let mut vm = Vm::new(&prog);
        vm.capture_only = true;
        vm.run().expect("runtime error");
        assert!(
            vm.stdout.contains("42"),
            "stdout should contain 42: {}",
            vm.stdout
        );
        assert!(vm.stdout.contains("positive"), "stdout: {}", vm.stdout);
        assert!(vm.stdout.contains("zero"), "stdout: {}", vm.stdout);
        assert!(vm.stdout.contains("negative"), "stdout: {}", vm.stdout);
    }

    #[test]
    fn example_10_adt_constructors_and_match() {
        // Int-arithmetic version (no floating point) of the area example.
        let src = r#"
type Shape = Circle(Int) | Square(Int) | Triangle(Int, Int)

fn area(s: Shape) -> Int effects {} {
    match s with {
        Circle(r)      => r * r,
        Square(side)   => side * side,
        Triangle(a, b) => a * b / 2
    }
}
fn main() -> Unit effects {IO} {
    print(str(area(Circle(5))))
    print(str(area(Square(4))))
    print(str(area(Triangle(6, 4))))
}
"#;
        let m = parse_module(FileId(0), src).expect("parse failed");
        let prog = compile_module(&m).expect("compile failed");
        let mut vm = Vm::new(&prog);
        vm.capture_only = true;
        vm.run().expect("runtime error");
        assert!(
            vm.stdout.contains("25"),
            "Circle(5) area=25, stdout: {}",
            vm.stdout
        );
        assert!(
            vm.stdout.contains("16"),
            "Square(4) area=16, stdout: {}",
            vm.stdout
        );
        assert!(
            vm.stdout.contains("12"),
            "Triangle(6,4) area=12, stdout: {}",
            vm.stdout
        );
    }

    // ── records ───────────────────────────────────────────────────────────────

    // ── opaque value tests ──────────────────────────────────────────────────────

    /// An `Opaque` value round-trips through a let-binding: storing it in a
    /// local and loading it back produces an equal value.
    #[test]
    fn opaque_roundtrips_through_let() {
        use std::sync::Arc;
        let inner: Arc<dyn std::any::Any + Send + Sync> = Arc::new(42_i64);
        let v = Value::Opaque {
            inner: inner.clone(),
            display: "test-opaque".to_string(),
        };
        // Put it in an array (simulating a locals slot), then read back.
        let locals = [v.clone()];
        let retrieved = locals[0].clone();
        assert_eq!(retrieved, v, "opaque value should equal itself after clone");
        assert_eq!(retrieved.display(), "test-opaque");
    }

    /// `Value::Opaque { display }` renders via `display()` just like the eval
    /// `Value::display()` would for the same underlying type.
    #[test]
    fn opaque_display_matches_stored_string() {
        use std::sync::Arc;
        // Simulate a ProvHandle whose display the eval side would produce.
        let display_str = "<prov head=0>".to_string();
        let v = Value::Opaque {
            inner: Arc::new(0_i64) as Arc<dyn std::any::Any + Send + Sync>,
            display: display_str.clone(),
        };
        assert_eq!(
            v.display(),
            display_str,
            "Opaque::display() must return the cached display string"
        );
    }

    /// Serialization of a program that contains *no* opaque values (the normal
    /// case for AOT artifacts) must still work correctly after the `Value::Opaque`
    /// variant was added.  Opaque values are runtime-only and never present in
    /// compiled programs.
    #[test]
    fn serialize_normal_program_still_works() {
        let src = r#"
fn add(a: Int, b: Int) -> Int effects {} { a + b }
fn main() -> Int effects {} { add(3, 4) }
"#;
        let m = aether_parser::parse_module(aether_ast::FileId(0), src).expect("parse");
        let prog = compile_module(&m).expect("compile");
        let bytes = serialize_program(&prog);
        let prog2 = deserialize_program(&bytes).expect("deserialize");
        let val = run_main(&prog2).expect("run");
        assert_eq!(
            val.as_int(),
            Some(7),
            "serialized/deserialized program must run correctly"
        );
    }

    #[test]
    fn record_field_get() {
        // Record literal + field access.
        use aether_eval::Runtime;
        let src = r#"
fn main() -> Int effects {} {
    let p = { x: 3, y: 4 }
    p.x + p.y
}
"#;
        let m = parse_module(FileId(0), src).unwrap();

        let mut rt = Runtime::new(m.clone());
        rt.capture_only = true;
        let eval_n = rt.run_main().unwrap().as_int().unwrap();

        let prog = compile_module(&m).unwrap();
        let bc_n = run_main(&prog).unwrap().as_int().unwrap();

        assert_eq!(bc_n, eval_n, "record p.x+p.y: bc={bc_n} eval={eval_n}");
        assert_eq!(bc_n, 7);
    }
}

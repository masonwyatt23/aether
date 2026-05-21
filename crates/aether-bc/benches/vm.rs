//! Criterion benchmark of the bytecode VM (`aether-bc`).
//!
//! The same workloads as `aether-eval/benches/interpreter.rs`, so the two
//! files together measure the tree-walker-vs-VM speedup. Each program is
//! compiled once, outside the timed closure; only `run_main` is measured.
//! Run with `cargo bench -p aether-bc`.

use criterion::{criterion_group, criterion_main, Criterion};

use aether_ast::FileId;
use aether_parser::parse_module;

/// Recursion-heavy: naive Fibonacci.
const FIB: &str = r#"
fn fib(n: Int) -> Int effects {} {
    if n < 2 then n else fib(n - 1) + fib(n - 2)
}
fn main() -> Int effects {} { fib(20) }
"#;

/// Call-overhead workload: a 500-deep tail recursion. (The bytecode VM does
/// not tail-call-optimize, so the depth is kept modest; the tree-walker TCOs
/// the same program.)
const COUNTDOWN: &str = r#"
fn cd(i: Int, n: Int) -> Int effects {} {
    if i >= n then i else cd(i + 1, n)
}
fn main() -> Int effects {} { cd(0, 500) }
"#;

/// Non-tail recursion with arithmetic at every frame: sum 1..=100.
/// (Kept shallow: `n + s(n-1)` is not a tail call, so deep non-tail
/// recursion overflows the stack on both runtimes.)
const SUMTO: &str = r#"
fn s(n: Int) -> Int effects {} {
    if n <= 0 then 0 else n + s(n - 1)
}
fn main() -> Int effects {} { s(100) }
"#;

fn bench_workload(c: &mut Criterion, name: &str, src: &str, expect: i64) {
    // Parse + compile to bytecode once; the timed closure only runs the VM.
    let module = parse_module(FileId(0), src).expect("benchmark source must parse");
    let program = aether_bc::compile_module(&module).expect("benchmark program must compile");
    c.bench_function(name, |b| {
        b.iter(|| {
            let v = aether_bc::run_main(&program).expect("benchmark program must run");
            assert_eq!(v.as_int(), Some(expect));
        });
    });
}

fn benches(c: &mut Criterion) {
    bench_workload(c, "vm/fib20", FIB, 6765);
    bench_workload(c, "vm/countdown500", COUNTDOWN, 500);
    bench_workload(c, "vm/sumto100", SUMTO, 5_050);
}

criterion_group!(vm, benches);
criterion_main!(vm);

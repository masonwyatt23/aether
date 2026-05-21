//! Criterion benchmark of the tree-walking interpreter (`aether-eval`).
//!
//! Each workload is parsed once, outside the timed closure; only execution
//! is measured. Run with `cargo bench -p aether-eval`. The bytecode VM is
//! benchmarked on the same workloads in `aether-bc/benches/vm.rs`.

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

/// Call-overhead workload: a 500-deep tail recursion. (Kept at 500 so the
/// bytecode VM — which does not yet tail-call-optimize — survives the same
/// workload; the tree-walker TCOs it and would handle any depth.)
const COUNTDOWN: &str = r#"
fn cd(i: Int, n: Int) -> Int effects {} {
    if i >= n then i else cd(i + 1, n)
}
fn main() -> Int effects {} { cd(0, 500) }
"#;

/// Non-tail recursion with arithmetic at every frame: sum 1..=100.
/// (Kept shallow: `n + s(n-1)` is not a tail call, so it is not TCO'd and
/// each frame costs real stack — deep non-tail recursion overflows.)
const SUMTO: &str = r#"
fn s(n: Int) -> Int effects {} {
    if n <= 0 then 0 else n + s(n - 1)
}
fn main() -> Int effects {} { s(100) }
"#;

fn bench_workload(c: &mut Criterion, name: &str, src: &str, expect: i64) {
    // Parse once; the timed closure only runs the program.
    let module = parse_module(FileId(0), src).expect("benchmark source must parse");
    c.bench_function(name, |b| {
        b.iter(|| {
            let mut rt = aether_eval::Runtime::new(module.clone());
            rt.capture_only = true;
            let v = rt.run_main().expect("benchmark program must run");
            assert_eq!(v.as_int(), Some(expect));
        });
    });
}

fn benches(c: &mut Criterion) {
    bench_workload(c, "eval/fib20", FIB, 6765);
    bench_workload(c, "eval/countdown500", COUNTDOWN, 500);
    bench_workload(c, "eval/sumto100", SUMTO, 5_050);
}

criterion_group!(interpreter, benches);
criterion_main!(interpreter);

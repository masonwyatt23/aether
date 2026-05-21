//! Criterion benchmark: fib(20) via bytecode VM vs tree-walking interpreter.

use criterion::{criterion_group, criterion_main, Criterion};

use aether_ast::FileId;
use aether_parser::parse_module;

const FIB20_SRC: &str = r#"
fn fib(n: Int) -> Int effects {} {
    if n < 2 then n else fib(n - 1) + fib(n - 2)
}
fn main() -> Int effects {} { fib(20) }
"#;

fn bench_bc(c: &mut Criterion) {
    // Pre-compile once outside the benchmark loop.
    let module = parse_module(FileId(0), FIB20_SRC).unwrap();
    let program = aether_bc::compile_module(&module).unwrap();

    c.bench_function("bc_fib20", |b| {
        b.iter(|| {
            let v = aether_bc::run_main(&program).unwrap();
            assert_eq!(v.as_int(), Some(6765));
        });
    });
}

fn bench_eval(c: &mut Criterion) {
    c.bench_function("eval_fib20", |b| {
        b.iter(|| {
            let module = parse_module(FileId(0), FIB20_SRC).unwrap();
            let mut rt = aether_eval::Runtime::new(module);
            rt.capture_only = true;
            let v = rt.run_main().unwrap();
            assert_eq!(v.as_int(), Some(6765));
        });
    });
}

criterion_group!(benches, bench_bc, bench_eval);
criterion_main!(benches);

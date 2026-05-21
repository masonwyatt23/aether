# Aether Performance

Aether ships two runtimes — a tree-walking interpreter (`aether-eval`, full
language semantics, automatic provenance, tail-call optimization) and a
bytecode VM (`aether-bc`). This document reports measured execution speed.

## Methodology

Benchmarks use [`criterion`](https://github.com/bheisler/criterion.rs).
Each workload is parsed and (for the VM) compiled to bytecode **once**,
outside the timed closure — only execution time is measured.

Sources: `crates/aether-eval/benches/interpreter.rs`,
`crates/aether-bc/benches/vm.rs`.

Reproduce:

```bash
cargo bench -p aether-eval --bench interpreter
cargo bench -p aether-bc   --bench vm
```

## Results

Measured on an Apple Silicon laptop (`aarch64-apple-darwin`, release build).
Absolute numbers are machine-dependent; the **ratio** is the portable figure.

| Workload | Tree-walker | Bytecode VM | Speedup |
|---|---:|---:|---:|
| `fib(20)` — recursion-heavy (~13.5k calls) | 45.0 ms | 1.92 ms | **23.5×** |
| `countdown(500)` — call overhead | 343.6 µs | 51.3 µs | **6.7×** |
| `sumto(100)` — arithmetic per frame | 458.9 µs | 9.83 µs | **46.7×** |

The bytecode VM is **roughly 7×–47× faster** depending on the workload;
the recursion-heavy `fib(20)` case lands at ~23×. The headline "~18×"
figure quoted elsewhere is a conservative round number — the true speedup
is workload-dependent and, for compute-bound code, higher.

## Why the VM is faster

The tree-walker re-traverses the AST on every evaluation and wraps every
intermediate value in an automatic provenance chain. The bytecode VM
compiles the AST once to a flat instruction stream and executes a stack
machine with no provenance bookkeeping. Provenance is the tree-walker's
defining feature *and* its main cost — which is the point of having both:
use the tree-walker when you want to query *why* a value exists, the VM
when you want raw speed.

## Honest caveats

- **The VM does not tail-call-optimize.** The tree-walker trampolines tail
  calls and runs 200k-deep recursion in constant stack; the VM recurses in
  the host and overflows on very deep recursion. The `countdown` workload
  is deliberately kept shallow (500) so both runtimes survive it.
- **The VM routes some builtins through the tree-walker** (eval-only values
  like `ProvChain`). Programs dominated by such calls see a smaller speedup.
- These are micro-benchmarks. Real programs mix parsing, checking, and I/O,
  none of which this measures.
- Numbers will differ on other hardware. Run the benchmarks yourself.

# A tour of Aether

> A one-page walkthrough that takes you from "never seen the language" to
> "I could ship an agent script in this." Reads in about 10 minutes.

## 1. Hello

```aether
fn main() -> Unit effects {IO} {
  print("hello, agent")
}
```

Everything is explicit: the return type is `Unit`, the side effect is `IO`.
The compiler rejects this program if `main` calls something that secretly
does network I/O — you have to write `effects {IO, Net}` for that.

## 2. Two surfaces for one AST

The same function in compact form:

```aether
main():U!{IO} = print("hello, agent")
```

`aether fmt --verbose` and `aether fmt --compact` project between them
deterministically. Agents emit compact; humans read verbose; reviews diff
either.

## 3. Refinements that get proved

```aether
fn min(a: Int, b: Int) -> Int
  where result <= a && result <= b
  effects {} {
  if a <= b then a else b
}
```

Run `aether check examples/02_refinement.ae` → `✓ refinements verified`.
The solver case-splits on the `if` and discharges each branch by linear
arithmetic. No tests, no fuzzing — just a proof done at compile time.

## 4. Effects you can't dodge

```aether
fn fetch(u: Str) -> Str effects {Net, Throw} { http_get(u) }

fn main() -> Unit effects {IO} {    # missing {Net, Throw}!
  let body = fetch("https://x")     # type error: missing effect
  print(body)
}
```

The checker tracks the *transitive* effect set: if anything you call uses
`Net`, you have to declare `Net`.

## 5. Agent primitives

```aether
fn double(x: Int) -> Int effects {} { x * 2 }
fn add(a: Int, b: Int) -> Int effects {} { a + b }

fn main() -> Unit effects {IO} {
  let surface = introspect("current")     # query the module without grep
  print_module_surface(surface)
}
```

`introspect`, `summarize` (budgeted, cached), and `provenance` are
language keywords. They let an agent inspect a module structurally instead
of re-reading source.

## 6. Provenance, automatic

```aether
fn main() -> Unit effects {IO} {
  let x = 5
  let y = x + 3
  let z = y * 2
  print_prov(provenance(z))      # full DAG of how z was computed
}
```

Every runtime value carries a `ProvChain`. An agent debugging its own
output can ask "where did this value come from?" without re-running.

## 7. Pattern matching, ADTs

```aether
type Shape = Circle(Float) | Square(Float) | Triangle(Float, Float, Float)

fn area(s: Shape) -> Float effects {} {
  match s with {
    Circle(r)         => r * r * 314 / 100,
    Square(side)      => side * side,
    Triangle(a, b, _) => a * b / 2
  }
}
```

Missing arms get a `Severity::Warning` from the exhaustiveness checker.
`Pattern::Ctor` binds nested vars; guards (`if cond`) work too.

## 8. Closures

```aether
fn main() -> Int effects {} {
  let k = 10
  let add_k = fn(x: Int) -> Int => x + k
  add_k(5)                                 # 15 — k is captured by value
}
```

## 9. In-language testing and benchmarks

```aether
test "min returns the smaller" {
  assert_eq(min(3, 7), 3)
  assert_eq(min(9, 2), 2)
}

bench "hot loop" {
  let _ = sum_to(50)
}
```

```
$ aether test examples/09_test_blocks.ae
  ✓ add is commutative
  ✓ double matches add
✓ 3 test(s) passed

$ aether bench examples/14_benchmarks.ae --iters 500
  fib(10)        500 iters   961.13 µs/iter
  sum_to(50)     500 iters   529.62 µs/iter
```

## 10. Imports across files

```aether
import std::iter

fn main() -> Unit effects {IO} {
  print(str(clamp(150, 0, 100)))      # 100, refinement-verified
}
```

`std::iter`, `std::list`, `std::string`, `std::map`, `std::plan`,
`std::mem`, `std::proof`, `std::json`, `std::path`, `std::time`, `std::env`,
`std::fmt`, `std::result` are shipped as `.ae` source files.

## 11. The tool registry

A Rust host can swap any agent primitive at runtime:

```rust
rt.tools.register("llm_complete", |args| { /* real Anthropic call */ });
```

The same code that runs with the deterministic LLM stub during tests can
hit a real model with `aether run --network`.

## 12. Bytecode VM

For tight loops, `aether-bc` compiles the same AST to bytecode and runs it
24× faster than the tree-walker (`fib(20)`: 1.5 ms vs 37 ms). Same
semantics, no closures-yet on the bytecode path (tree-walker still required
for full programs).

## 13. Editor

- LSP server: `cargo run -p aether-lsp`
- VS Code extension: `vscode-aether/` (install via `vsce package`)
- Syntax + diagnostics + hover + completion + goto-def out of the box.

## 14. Where to go next

- `spec/LANGUAGE.md` — full reference.
- `spec/REFINEMENTS.md` — solver tutorial with worked examples.
- `spec/RATIONALE.md` — why these design choices, not others.
- `examples/01–14` — every feature has a runnable example.
- `Justfile` — `just demo`, `just ae-test`, `just ae-bench`.

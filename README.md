# Aether

[![CI](https://github.com/masonwyatt23/aether/actions/workflows/ci.yml/badge.svg)](https://github.com/masonwyatt23/aether/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)
[![386 tests](https://img.shields.io/badge/tests-386%20passing-brightgreen.svg)](#)
[![Rust](https://img.shields.io/badge/rust-stable-orange.svg)](#install)

> **A programming language designed for AI agents to read, write, and verify code.**

Aether is a statically typed, effect-tracked language where every function declares its side effects, every postcondition is proved at compile time, and every runtime value carries automatic provenance. Where Python optimizes for human keystrokes and TypeScript for human refactoring, **Aether optimizes for agent reasoning per token**.

The result: code an agent can emit, verify without running, and query structurally — without grepping source.

---

## See it

```aether
fn min(a: Int, b: Int) -> Int
  where result <= a && result <= b
  effects {} {
  if a <= b then a else b
}
```

```
$ aether check examples/02_refinement.ae
✓ types, effects, and refinements verified
```

The `where` clause is a postcondition. The compiler case-splits on the `if` and discharges each branch via linear arithmetic — no tests, no fuzzing, just a proof at compile time.

---

## Why it exists

- **Token efficiency.** Agents waste tokens on Python/TS boilerplate. Aether has a dense compact syntax (`min(a:Int,b:Int):Int!{}`) and a verbose projection for humans — one AST, two surfaces, projected deterministically by `aether fmt`.
- **Provable correctness.** A built-in Fourier-Motzkin solver checks refinement postconditions statically. Ship code you can prove, not just code that passes a test suite.
- **Queryable structure.** `introspect("current")` returns the module's semantic surface as data. An agent inspects a module without re-reading source. `provenance(v)` returns a full computation DAG for any value at runtime.
- **Explicit effects.** Every function signature declares what side effects it may trigger. The type checker tracks transitively — if you call `Net`, you must declare `Net`. No hidden I/O.

---

## Feature highlights

| Feature | Detail |
|---|---|
| Refinement types | Hand-rolled FM/LIA solver; path-sensitive; `forall_in`, `mod`/`div` by constant |
| Effect tracking | Row-polymorphic unordered effect sets on every signature; transitive checking |
| Algebraic data types | `type Shape = Circle(Float) | Square(Float)` + constructor patterns + exhaustiveness |
| Closures | First-class, captured by value |
| Dual syntax | Compact (agent-emitted) + verbose (human-readable); `aether fmt` projects either way |
| Two runtimes | Tree-walker (full features) + bytecode VM (**18x speedup** on `fib(20)`) with differential testing |
| LSP + editor | LSP server, VS Code extension (syntax, snippets, diagnostics, hover, goto-def), browser playground |
| Stdlib | 21 modules shipped as `.ae` source: `iter`, `list`, `map`, `string`, `json`, `result`, `math`, `regex`, `mem`, `plan`, `proof`, `fmt`, `path`, `time`, `env`, `sys`, `base64`, `hash`, `uuid`, `random`, `date` |
| In-language testing | `test "..." { ... }`, `bench "..." { ... }`, `snap "..." { ... }` blocks; `aether test/bench/snap` |
| Agent primitives | `introspect`, `summarize`, `provenance`, `confident`, `assume`, `spec` as language keywords |
| Network | `--network` flag enables real `http_get` + `llm_complete` via `ANTHROPIC_API_KEY` |
| Persistent memory | `std::mem` — JSON store at `~/.aether/mem.json` out of the box |

---

## Install

```bash
git clone https://github.com/masonwyatt23/aether && cd aether
cargo install --path crates/aether-cli   # puts `aether` on $PATH
aether --version                         # aether 0.3.0
```

Or use Docker:

```bash
docker build -t aether:0.3 .
docker run --rm -v "$PWD:/work" aether:0.3 check /work/main.ae
```

**Requirements:** Rust stable (1.75+). No external dependencies for the solver or type checker.

---

## Quickstart

```bash
# scaffold a new project
aether init hello && cd hello

# type-check, effect-check, and verify refinements
aether check main.ae
# ✓ types, effects, and refinements verified

# run it
aether run main.ae
# Hello, world!

# run the bytecode VM (18x faster for tight loops)
aether run --bc main.ae

# run in-language tests
aether test main.ae

# watch mode — re-checks on every save (150ms debounce)
aether watch main.ae
```

---

## 60-second tour

### 1. Explicit effects — no hidden I/O

```aether
fn fetch(u: Str) -> Str effects {Net, Throw} { http_get(u) }

fn main() -> Unit effects {IO} {   # missing Net, Throw
  let body = fetch("https://example.com")
  print(body)
}
```

```
error: effect `Net` not declared in `main`
```

If you call it, you declare it. The checker is transitive.

---

### 2. Refinement proofs at compile time

```aether
fn min(a: Int, b: Int) -> Int
  where result <= a && result <= b
  effects {} {
  if a <= b then a else b
}
```

```
$ aether check examples/02_refinement.ae
✓ types, effects, and refinements verified
```

---

### 3. Algebraic data types + pattern matching

```aether
type Task = Pending(Str) | Done(Str) | Failed(Str, Str)

fn describe(t: Task) -> Str effects {} {
  match t with {
    Pending(label)  => "pending: ${label}",
    Done(label)     => "done: ${label}",
    Failed(l, why)  => "failed: ${l} (${why})"
  }
}
```

---

### 4. Agent primitives

```aether
fn main() -> Unit effects {IO} {
  let surface = introspect("current")   # query module structure as data
  print_module_surface(surface)
}
```

```aether
fn main() -> Unit effects {IO} {
  let x = 5
  let y = x + 3
  let z = y * 2
  print_prov(provenance(z))   # full computation DAG: z <- y*2 <- (x+3)*2 <- 5
}
```

`introspect`, `summarize`, and `provenance` are language keywords — not library calls. An agent can inspect a module structurally without re-reading source.

---

### 5. In-language tests, live output

```aether
test "min returns the smaller" {
  assert_eq(min(3, 7), 3)
  assert_eq(min(9, 2), 2)
}
```

```
$ aether test examples/09_test_blocks.ae
  ✓ add is commutative
  ✓ double matches add
  ✓ comparison

✓ 3 test(s) passed
```

---

## Repository layout

```
crates/
  aether-ast/          # AST + spans + provenance arena (serde for JSON)
  aether-lexer/        # logos-based tokenizer (compact + verbose)
  aether-parser/       # Pratt parser -> typed AST + dual-form pretty-printer
  aether-types/        # HM inference + effects + LIA refinement solver (proptest)
  aether-eval/         # tree-walker: closures, ADTs, match, provenance, tool registry
  aether-bc/           # bytecode VM (18x speedup over tree-walker on fib(20))
  aether-stdlib/       # 21 stdlib modules shipped as .ae sources
  aether-tools-net/    # real HTTP + Anthropic LLM via reqwest, behind --network flag
  aether-lsp/          # LSP server: diagnostics, hover, goto-def, completion
  aether-wasm/         # browser playground (wasm-pack)
  aether-cli/          # `aether` binary: check / run / test / bench / fmt / repl / watch / doc / init
spec/
  LANGUAGE.md          # full language reference
  RATIONALE.md         # design decisions and what was deliberately rejected
  TOUR.md              # 10-minute walkthrough
  REFINEMENTS.md       # solver tutorial with worked examples
  grammar.ebnf         # formal grammar
  AST_JSON_SCHEMA.md   # serialized AST schema for tooling
vscode-aether/         # VS Code extension: syntax + snippets + LSP client
playground/            # browser REPL
examples/              # 28 runnable programs covering every feature
CHANGELOG.md           # release notes per version
ROADMAP.md             # planned work
CONTRIBUTING.md        # contribution guide
SECURITY.md            # security policy
```

---

## Project status

**v0.3 — a complete, well-tested language implementation and research artifact.**

Aether implements a full language pipeline: lexer → parser → type checker (HM + effects + refinements) → two runtimes (tree-walker + bytecode VM) → LSP → 21 stdlib modules → browser playground. 386 tests pass across 13 crates; differential testing between the runtimes is active.

**Not production-hardened.** Error messages are functional but terse. The bytecode VM does not yet support closures (the tree-walker fallback is automatic). The refinement solver covers linear arithmetic; nonlinear constraints yield a warning, not an error. This is a research artifact and exploration vehicle, not a production runtime.

See [ROADMAP.md](ROADMAP.md) for planned work. See [spec/RATIONALE.md](spec/RATIONALE.md) for what was built and why.

| Spec doc | Contents |
|---|---|
| [spec/LANGUAGE.md](spec/LANGUAGE.md) | Full language reference |
| [spec/TOUR.md](spec/TOUR.md) | 10-minute walkthrough |
| [spec/REFINEMENTS.md](spec/REFINEMENTS.md) | Solver tutorial with worked examples |
| [spec/RATIONALE.md](spec/RATIONALE.md) | Design decisions and tradeoffs |

---

## License

Aether is dual-licensed under **MIT OR Apache-2.0**. You may use it under either license at your option.

See [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE).

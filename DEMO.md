# Aether — From Zero to "Wow" in 12 Commands

Aether is a programming language designed for AI agents: it tracks effects at compile time, proves value invariants via a linear-arithmetic solver, records the provenance of every computed value, and ships a bytecode VM that is 18× faster than the tree-walker for compute-heavy code. Every one of these demos runs offline with no API key; pass `--network` to wire up real HTTP and LLM calls.

> **Prerequisites:** `cargo install aether-cli` (or clone and `cargo build --release -p aether-cli`).  
> All commands below assume `aether` is on your `PATH`.

---

## 1. Prove things at compile time

Refinement types let you state value invariants — `result >= 0`, `x >= lo && x <= hi` — and have the solver prove them statically before a single line runs.

```
$ aether check examples/12_refinement_proven.ae
✓ types, effects, and refinements verified
```

The file declares four functions with non-trivial postconditions (`pos_abs` always ≥ 0, `clamp_to` always inside `[lo, hi]`, etc.). No proof obligation is deferred to runtime.

---

## 2. Two runtimes — same semantics

Run the recursive Fibonacci benchmark through the **tree-walker** and then the **bytecode VM**:

```
$ aether run examples/18_bytecode.ae
6765
```

```
$ aether run --bc examples/18_bytecode.ae
6765
```

Both return `fib(20) = 6765`. The bytecode VM is ~18× faster for compute-intensive programs (verified by the differential test suite in `crates/aether-difftest/`).

Ahead-of-time compilation to a portable `.aebc` file also works:

```
$ aether compile examples/18_bytecode.ae -o /tmp/fib.aebc
compiled examples/18_bytecode.ae → /tmp/fib.aebc (343 bytes)

$ aether exec /tmp/fib.aebc
6765
```

---

## 3. Agent-native primitives

### Module introspection

`introspect("current")` returns the full typed surface of the running module — exports, effects, signatures — without grepping source:

```
$ aether run examples/04_introspect.ae
module <anonymous> — Agent primitive: `introspect("current")` returns a structured surface of the
current module — exports, effects, signatures — in a single primitive call.

This is how Aether keeps agents fast: instead of grepping source, the agent
queries a budgeted projection of the module.
Doubles a number.
  fn double : (x: Int) -> Int   # Agent primitive: `introspect("current")` returns a structured surface of the
  fn add : (x: Int, y: Int) -> Int   # Adds two numbers.
  fn main : () -> Unit !{IO}
```

### Provenance tracing

Every computed value carries a DAG of the operations that produced it. `provenance(v)` surfaces that chain — useful when an agent needs to explain a decision:

```
$ aether run examples/11_provenance_query.ae
15
provenance:
  #0: Lit
  #5: Call("process")   <- [0]
```

---

## 4. Test · bench · snapshot — in-language

Aether embeds three testing primitives directly in the source language; no external framework required.

### Tests

```
$ aether test examples/20_full_showcase.ae
  ✓ describe pending
  ✓ describe done
  ✓ task_of result
  ✓ ranked produces longer string
  ✓ clamp from std::iter is verified

✓ 5 test(s) passed
```

### Benchmarks

```
$ aether bench examples/14_benchmarks.ae
  fib(10)        100 iters       278 µs/iter   (27.80 ms total)
  sum_to(50)     100 iters       139 µs/iter   (13.91 ms total)
  string concat  100 iters         0.40 µs/iter   (0.04 ms total)

✓ 3 bench(es) completed
```

### Snapshots

```
$ aether snap examples/17_snapshots.ae
  ✓ clamp behavior — ok

✓ 1 snapshot(s) passed (1 ok)
```

Use `aether snap examples/17_snapshots.ae --update` to regenerate the golden `.snap` file.

---

## 5. Compile and execute a standalone binary

You can compile any Aether file to a self-contained `.aebc` bytecode artifact and run it anywhere without the source:

```
$ aether compile examples/20_full_showcase.ae -o /tmp/showcase.aebc
compiled examples/20_full_showcase.ae → /tmp/showcase.aebc (...)

$ aether exec /tmp/showcase.aebc
v0.3 agent — running for Aether
⏳ research
✅ design
❌ implement (build broke)
[p=7] critical task
```

---

## 6. The browser playground

A zero-install, offline playground ships in `playground/`. The parser, type checker, and tree-walking interpreter run as a WebAssembly module — no server required.

**Build it once:**

```bash
rustup target add wasm32-unknown-unknown   # one-time
cd playground
./build.sh                                 # installs wasm-pack if needed, builds pkg/
```

**Serve it:**

```bash
python3 -m http.server 8080
# open http://localhost:8080/playground/
```

Everything in `playground/` is static HTML + CSS + JS. The wasm bundle in `playground/pkg/` is pre-built and committed so you can open `index.html` directly in most browsers without a build step.

> **Playground limits:** `import` statements are disabled (programs must be self-contained); `http_get` and `llm_complete` use deterministic stubs.

---

## What to explore next

| Command | What it does |
|---------|-------------|
| `aether repl` | Interactive REPL — type expressions, see results instantly |
| `aether lint examples/20_full_showcase.ae` | Static analysis: over-broad effects, unused lets, missing docs |
| `aether doc examples/20_full_showcase.ae` | Generate Markdown docs from docstring comments |
| `aether explain examples/04_introspect.ae` | Pretty-print the module surface (same as `introspect("current")`) |
| `aether watch examples/02_refinement.ae` | Re-check on every save (150 ms debounce) |
| `aether fmt examples/20_full_showcase.ae` | Auto-format source |

---

## 7. Verification gate

`aether verify` is the strict sibling of `aether check`.  Use it in CI or an AI-agent loop when you need a guarantee — not a best-effort.

| Command | Exits 0 when... |
|---------|----------------|
| `aether check` | no type/effect **errors** (warnings — unproven contracts — are tolerated) |
| `aether verify` | no errors **and** no warnings (every contract must be proved) |

### Passing: all contracts proved

```
$ aether verify examples/31_verify_gate.ae
✓ verified — all contracts proved, all effects sound
```

`31_verify_gate.ae` declares three functions with linear-arithmetic postconditions (`result == n + n`, `result > n`, `result >= a && result >= b`). The built-in solver discharges all of them at compile time.

### Failing: unproven contract

When the linear solver cannot decide a postcondition it emits a `Warning` ("could not verify"). `check` exits 0; `verify` exits 1:

```
$ aether check   unproven.ae   # warning, but exits 0
✓ types/effects ok (1 warning(s), refinements partially verified)

$ aether verify  unproven.ae   # same warning, exits 1
✗ verification failed — 0 error(s), 1 unproven contract(s)
```

### Machine-readable output for pipelines

```
$ aether verify --json examples/31_verify_gate.ae
{
  "verified": true,
  "errors": 0,
  "warnings": 0,
  "diagnostics": []
}
```

A failing run emits `"verified": false` with `warnings > 0` and the full diagnostic list — ready for an agent to parse and act on.

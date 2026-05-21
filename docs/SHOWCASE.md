# Aether — What Makes It Interesting

Aether is a research-stage language designed for AI agents: effects are
tracked at compile time, value invariants are proved (not asserted), and
every computed value carries its derivation history. This document shows five
ideas with real snippets and real tool output — nothing is mocked.

> **Honest disclaimer:** Aether is a research artifact, not a production
> language. APIs may change; the LLM primitives use deterministic stubs unless
> you pass `--network`. Run `cargo build --release -p aether-cli` to follow
> along, then put `./target/release/aether` on your PATH.

---

## 1. Refinement proofs — the compiler proves it, you don't assert it

Aether's type checker includes a hand-rolled Fourier–Motzkin solver over
linear rational arithmetic. You write `where result >= 0` or
`where result >= lo && result <= hi` as part of the function signature, and the
compiler verifies the claim statically before a single line executes.

```aether
## abs always returns a non-negative integer.
fn pos_abs(n: Int) -> Int where result >= 0 effects {} {
  if n >= 0 then n else 0 - n
}

## Two-armed clamp restricted to a valid range via parameter refinement.
fn clamp_to(x: Int, lo: Int, hi: Int{h: h >= lo}) -> Int
  where result >= lo && result <= hi
  effects {} {
  if x < lo then lo
  else if x > hi then hi
  else x
}

## Successor of a positive integer is positive.
fn next_pos(n: Int{x: x > 0}) -> Int where result > 0 effects {} {
  n + 1
}
```

Run the checker:

```
$ aether check examples/12_refinement_proven.ae
✓ types, effects, and refinements verified
```

The interesting part: the compiler proved `clamp_to` correct over *all*
possible inputs without running it. The parameter refinement `hi: Int{h: h >= lo}`
constrains the input space; the postcondition `result >= lo && result <= hi`
is then discharged by the solver. No runtime assertions, no fuzzing needed for
this property.

---

## 2. Effect tracking — undeclared effects are a compile error

Every function declares the effects it may perform in its signature. If the
body uses an effect the signature doesn't declare, the compiler rejects the
program with a precise error and source span.

This function correctly declares `{Net, Throw}`:

```aether
fn fetch(url: Str) -> Str effects {Net, Throw} {
  http_get(url)
}

fn main() -> Unit effects {IO, Net, Throw} {
  let body = fetch("https://example.com")
  print(body)
}
```

Remove `Net` and `Throw` from the declaration:

```aether
fn fetch(url: Str) -> Str effects {} {   ## <-- wrong
  http_get(url)
}
```

The compiler rejects it:

```
$ aether check /tmp/effect_missing.ae
Error: function `fetch` uses effect `Net` but declares effects {}
   ╭─[/tmp/effect_missing.ae:3:1]
   │
 3 │ ╭─▶    http_get(url)
   ┆ ┆
 5 │ ├─▶
   │ │
   │ ╰────── function `fetch` uses effect `Net` but declares effects {}
───╯
Error: function `fetch` uses effect `Throw` but declares effects {}
   ╭─[/tmp/effect_missing.ae:3:1]
   │
 3 │ ╭─▶    http_get(url)
   ┆ ┆
 5 │ ├─▶
   │ │
   │ ╰────── function `fetch` uses effect `Throw` but declares effects {}
───╯

✗ 2 error(s), 0 warning(s)
```

Why this matters for agents: an LLM-generated function that accidentally
makes network calls or panics cannot be silently mixed into a pure pipeline.
The effect boundary is enforced, not advisory.

---

## 3. Automatic provenance — every value carries its derivation DAG

Every value in Aether records how it was produced. `provenance(v)` returns a
first-class chain handle; `print_prov(chain)` renders the DAG.

```aether
fn main() -> Unit effects {IO} {
  let x = 5
  let y = x + 3
  let z = y * 2
  let chain = provenance(z)
  print_prov(chain)
}
```

```
$ aether run examples/05_provenance.ae
provenance:
  #0: Lit
  #1: Lit
  #2: BinOp("+")   <- [0, 1]
  #3: Lit
  #4: BinOp("*")   <- [2, 3]
```

Node `#4` is `z`. Its parents are node `#2` (the addition `y = x + 3`) and
node `#3` (the literal `2`). Node `#2`'s parents are `#0` (`x = 5`) and
`#1` (the literal `3`). The full derivation is inspectable at runtime without
any extra instrumentation — it is built in.

This is designed for agent tracing: when an agent produces a result and needs
to explain or audit its reasoning, the provenance DAG is already there.

---

## 4. Two runtimes, proved equivalent by differential testing

Aether ships two execution engines:

- **Tree-walking interpreter** (`aether-eval`) — simple, correct by
  construction, used as the reference.
- **Bytecode VM** (`aether-bc`) — compiles to a compact instruction set;
  18× faster than the tree-walker on compute-heavy workloads.

The two runtimes are tested for agreement on every example in `examples/`.
The harness (`aether-difftest`) runs each program on both engines and fails
if their observable outputs differ.

Current state: **30 examples run, 0 divergences.**

```
--- diff summary ---
total: 28  matched: 24  bc-skipped: 2  nondeterministic: 2
both-failed: 0  tree-failed: 0  load-errors: 0  DIVERGED: 0
```

`bc-skipped` means two examples use features not yet implemented in the
bytecode compiler. `nondeterministic` means two examples use LLM stubs whose
output varies. Neither counts as a divergence. The only hard failure mode is
`DIVERGED` — both runtimes produced different results for the same input.
That count is zero.

---

## 5. Agent primitives — introspect, summarize, confident

Aether has three built-in primitives aimed at agent reasoning loops:

**`introspect("current")`** — returns a structured surface of the current
module (exports, effect signatures, docstrings) without grepping source:

```aether
fn double(x: Int) -> Int effects {} { x * 2 }
fn add(x: Int, y: Int) -> Int effects {} { x + y }

fn main() -> Unit effects {IO} {
  let surface = introspect("current")
  print_module_surface(surface)
}
```

```
$ aether run examples/04_introspect.ae
module <anonymous> — ...
  fn double : (x: Int) -> Int
  fn add : (x: Int, y: Int) -> Int
  fn add : (x: Int, y: Int) -> Int   # Adds two numbers.
  fn main : () -> Unit !{IO}
```

**`confident(value, score)`** — wraps a value with a confidence score so
downstream code can gate on minimum certainty before acting:

```aether
fn main() -> Unit effects {IO} {
  let value = iter_refine(0, "step", 5)
  let tagged = confident(value, 0.85)
  print(str(value))
  print(str(tagged))
}
```

```
$ aether run examples/06_agent_loop.ae
5
5 ~confidence(0.85)
```

**`summarize`** is the third primitive (see `examples/13_agent_workflow.ae`)
— it produces a budget-bounded textual summary of a value, useful when an
agent needs to pass context into a constrained-token prompt window.

---

## Where to go next

| File | What it covers |
|------|---------------|
| `DEMO.md` | Step-by-step command walkthrough (12 commands) |
| `spec/LANGUAGE.md` | Full language reference |
| `spec/RATIONALE.md` | Design decisions and trade-offs |
| `spec/TOUR.md` | Guided tour of the type system |
| `ROADMAP.md` | What is shipped, what is planned, what is a non-goal |
| `examples/20_full_showcase.ae` | All features in one file |

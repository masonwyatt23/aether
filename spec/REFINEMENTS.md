# Refinement types in Aether

> A practical tutorial. Read this top-to-bottom and you'll know how to write
> functions whose postconditions are checked at compile time, without ever
> running the code.

## The 30-second pitch

A **refinement type** narrows a base type to the values that satisfy a
predicate. `Int{n: n > 0}` is the type of strictly positive integers.
`Str{s: len(s) < 256}` is the type of short strings. The compiler's solver
proves that values flow only into types they actually inhabit.

Where a regular type says *what shape the data has*, a refinement says
*what's true about it*. The solver does the bookkeeping.

## Three places refinements appear

```
# 1. Parameter refinement — narrows the caller's side.
fn sqrt(n: Int{x: x >= 0}) -> Int effects {} { ... }

# 2. Return-value postcondition (the binder is `result`).
fn pos_abs(n: Int) -> Int where result >= 0 effects {} {
  if n >= 0 then n else 0 - n
}

# 3. Local refinement on a `let`-bound value.
let safe: Int{x: x > 0} = 5
```

## What the solver can prove

Aether ships a hand-rolled Fourier-Motzkin solver over linear rational
arithmetic with boolean combinations. It can decide:

- Linear comparisons: `a + b <= c`, `2 * x - 3 > 0`.
- Conjunctions, disjunctions, negations of those.
- Implications written as `!p || q`.
- Bounded universal quantifiers: `forall_in(x, 0, 10, p)` is unrolled.
- Mod and division by **constant** divisors (introduces fresh bounded vars).

What it cannot prove:

- Non-linear multiplication (`x * y` where both are variables). Reported as
  a warning, not an error — the code still compiles.
- Quantifiers over unbounded domains.
- Trigonometry, exponentials.

## Worked example: `min`

```
fn min(a: Int, b: Int) -> Int
  where result <= a && result <= b
  effects {} {
  if a <= b then a else b
}
```

The checker case-splits on `if`:

- Branch 1 (`a <= b`): `result = a`. Goals: `a <= a` ✓ and `a <= b` ✓ (from hypothesis).
- Branch 2 (`a > b`): `result = b`. Goals: `b <= a` ✓ (from hypothesis `a > b`) and `b <= b` ✓.

`aether check` prints `✓ types, effects, and refinements verified`.

## Worked example: making `clamp` provable

The naive version doesn't prove cleanly because the solver doesn't know
`lo <= hi`:

```
fn clamp(x: Int, lo: Int, hi: Int) -> Int
  where result >= lo && result <= hi
  effects {} { ... }
```

Encode the precondition as a refinement on `hi`:

```
fn clamp(x: Int, lo: Int, hi: Int{h: h >= lo}) -> Int
  where result >= lo && result <= hi
  effects {} {
  if x < lo then lo
  else if x > hi then hi
  else x
}
```

Now the solver has `hi >= lo` as a hypothesis at every branch. All three
arms prove.

## Worked example: `bounded_inc`

```
fn bounded_inc(x: Int, hi: Int{h: h >= x}) -> Int
  where result >= x && result <= hi + 1
  effects {} {
  x + 1
}
```

- `result = x + 1`. Goal 1: `x + 1 >= x` — trivial linear. Goal 2: `x + 1 <= hi + 1`
  — from `hi >= x` add 1 to both sides. Proved.

## When the solver returns `Unknown`

Anything outside the linear fragment becomes a `Warning`, not an `Error`.
You'll see:

```
warning: postcondition `result * result >= 0` could not be verified
         (outside linear-arithmetic fragment or beyond solver budget)
```

This is **by design**: an agent should be able to ship a useful program
with one unverified refinement rather than be rejected. Use `aether check
--strict` to promote warnings to errors when you want hard guarantees.

## When to use what

| Situation | Refinement |
|---|---|
| Argument must be in a range | `n: Int{x: x >= 0 && x < 100}` |
| Caller must supply a non-empty string | `s: Str{x: len(x) > 0}` |
| Function returns a positive | `-> Int where result > 0` |
| Two-arg precondition (`hi >= lo`) | Move to the second param: `hi: Int{x: x >= lo}` |
| Confidence-tagged value | `Str ~ confidence(p)` |

## Combining with `assume`

Sometimes the solver gives up but you know more than it does. The
`assume(predicate)` block introduces an axiom local to its scope, surfaced
in the value's provenance so reviewers can audit it:

```
fn nonlinear(n: Int{x: x > 0}) -> Int where result > 0 effects {} {
  assume(n * n > 0)           # the solver can't prove this on its own
  n * n
}
```

This is an **escape hatch**; it lets unsound proofs compile. Every
`assume` shows up in `provenance(value)` so an auditor (or another agent)
can find them.

## Performance

The solver is bounded. DNF expansion is capped at 64 disjuncts; FM
elimination is capped at 64 iterations per clause. Pathological inputs
fall back to `Unknown` rather than hanging. The `proptest` soundness
harness validates that "Proved" verdicts never have brute-force
counterexamples in `[-10, 10]⁴`.

## Where to learn more

- `crates/aether-types/src/refine.rs` — the solver source.
- `examples/02_refinement.ae`, `examples/12_refinement_proven.ae` — runnable.
- `crates/aether-stdlib/aether/std/iter.ae` — `clamp` lives here, refinement-proven.

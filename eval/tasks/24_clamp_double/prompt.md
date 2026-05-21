# Task 24: Clamp the Doubled Input

Write a function `clamp_double` that takes an integer `x`, **doubles it**, and clamps `2 * x` to the range `[lo, hi]`.

## Specification

- Compute `2 * x`.
- If `2 * x < lo`, return `lo`.
- If `2 * x > hi`, return `hi`.
- Otherwise, return `2 * x`.
- The function is pure (no side effects).

## Precondition

The parameter `hi` carries a refinement: `hi >= lo`. The signature encodes this as `hi: Int{h: h >= lo}`.

## Contract

The compiler must be able to prove at compile time that:
- The result is at least `lo`
- The result is at most `hi`

## Signature (provided — do not change)

```aether
fn clamp_double(x: Int, lo: Int, hi: Int{h: h >= lo}) -> Int
  where result >= lo && result <= hi
  effects {} {
  # your implementation here
}
```

Replace `# your implementation here` with a correct body.

## Why this is hard

Two common wrong implementations are:
1. Return `x` (without doubling) — the solver finds a counterexample where `x` is outside `[lo, hi]`.
2. Return `2 * x` without clamping — the solver finds a counterexample where `2 * x` is outside `[lo, hi]`.

The correct solution must double `x` **and** clamp the result, requiring three branches that branch on `2 * x` (not on `x` alone).

**Difficulty:** hard

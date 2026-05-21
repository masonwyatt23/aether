# Task 26: Sign Clamp

Write a function `sign_clamp` that returns `base + sign(x)`: it shifts the sign of `x` by a `base` offset. This produces one of three values: `base - 1`, `base`, or `base + 1`.

## Specification

- If `x > 0`, return `base + 1`.
- If `x < 0`, return `base - 1`.
- If `x == 0`, return `base`.
- The function is pure (no side effects).

## Contract

The compiler must be able to prove at compile time that:
- The result is at least `base - 1`
- The result is at most `base + 1`

## Signature (provided — do not change)

```aether
fn sign_clamp(x: Int, base: Int) -> Int
  where result >= base - 1 && result <= base + 1
  effects {} {
  # your implementation here
}
```

Replace `# your implementation here` with a correct body.

## Note on Aether syntax

Aether does not support unary minus. Use `0 - 1` to express `-1`, and `base - 1` to express the lower bound.

## Why this is hard

A naive implementation that returns the raw sign value (`1`, `-1`, or `0`) without adding `base` violates the contract in every branch — the solver finds counterexamples for all three cases. The correct solution must add `base` in each branch, and must also handle the zero case explicitly (a two-branch implementation misses either the positive or zero case).

**Difficulty:** hard

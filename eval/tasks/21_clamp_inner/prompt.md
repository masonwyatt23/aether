# Task 21: Inner-Range Clamp

Write a function `clamp_inner` that clamps an integer `x` to the **strict interior** of the range `[lo, hi]` — that is, to `[lo + 1, hi - 1]`.

## Specification

- If `x < lo + 1`, return `lo + 1`.
- If `x > hi - 1`, return `hi - 1`.
- Otherwise, return `x`.
- The function is pure (no side effects).

## Precondition

The parameter `hi` carries a refinement: `hi >= lo + 2`. This guarantees the inner range `[lo + 1, hi - 1]` is non-empty (it contains at least one integer). The signature encodes this as `hi: Int{h: h >= lo + 2}`.

## Contract

The compiler must be able to prove at compile time that:
- The result is at least `lo + 1`
- The result is at most `hi - 1`

## Signature (provided — do not change)

```aether
fn clamp_inner(x: Int, lo: Int, hi: Int{h: h >= lo + 2}) -> Int
  where result >= lo + 1 && result <= hi - 1
  effects {} {
  # your implementation here
}
```

Replace `# your implementation here` with a correct body.

## Why this is hard

A naive implementation that clamps to the **outer** range `[lo, hi]` instead of `[lo + 1, hi - 1]` violates the contract — the solver finds a counterexample where the result equals `lo` (which fails `result >= lo + 1`) or `hi` (which fails `result <= hi - 1`). Every branch must use the shifted bounds.

**Difficulty:** hard

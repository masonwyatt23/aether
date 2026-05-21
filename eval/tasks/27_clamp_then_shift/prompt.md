# Task 27: Clamp Then Shift

Write a function `clamp_then_shift` that first clamps `x` to `[lo, hi]`, then **adds `shift`** to the clamped value. The result lives in the shifted range `[lo + shift, hi + shift]`.

## Specification

- If `x < lo`, return `lo + shift`.
- If `x > hi`, return `hi + shift`.
- Otherwise, return `x + shift`.
- The function is pure (no side effects).

## Precondition

The parameter `hi` carries a refinement: `hi >= lo`. The signature encodes this as `hi: Int{h: h >= lo}`.

## Contract

The compiler must be able to prove at compile time that:
- The result is at least `lo + shift`
- The result is at most `hi + shift`

## Signature (provided — do not change)

```aether
fn clamp_then_shift(x: Int, lo: Int, hi: Int{h: h >= lo}, shift: Int) -> Int
  where result >= lo + shift && result <= hi + shift
  effects {} {
  # your implementation here
}
```

Replace `# your implementation here` with a correct body.

## Why this is hard

Two common wrong implementations are refuted:
1. A correct clamp without the shift (`if x < lo then lo else if x > hi then hi else x`) — the solver finds that `lo` fails `result >= lo + shift` when `shift != 0`.
2. Adding shift without clamping (`x + shift`) — the solver finds that `x + shift` can exceed `hi + shift` when `x > hi`.

The correct solution must apply **both** the clamp and the shift in every branch.

**Difficulty:** hard

# Task 65: Step Down to a Floor

Move `x` down by `step`, but never below the floor `lo`.

## Specification

- If `x - step` would fall below `lo`, return `lo`.
- Otherwise return `x - step`.
- The function is pure (no side effects).

## Preconditions

- `x` is in range: `lo <= x <= hi` (encoded in `x`'s type).
- `hi >= lo` (encoded in `hi`'s type).
- `step >= 0` (encoded in `step`'s type).

## Contract

The compiler must be able to prove at compile time that:

- `result >= lo`
- `result <= x`

## The trap

`x - step` alone breaks `result >= lo` once `step` is large enough to
overshoot the floor. The result must be floored at `lo`.

## Signature (provided — do not change)

```aether
## Move x down by step, flooring the result at lo.
## Preconditions: lo <= x <= hi (in x's type), hi >= lo, step >= 0.
## Contract: result >= lo AND result <= x (proved at compile time).
fn step_down(x: Int{v: v >= lo && v <= hi}, lo: Int, hi: Int{h: h >= lo}, step: Int{s: s >= 0}) -> Int
  where result >= lo && result <= x
  effects {} {
  0
}
```

Replace the stub body with a correct implementation.

**Difficulty:** expert

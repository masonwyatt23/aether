# Task 63: Window Clamp

Clamp `x` into the window `[lo, lo + width]`, where the window is
described by its lower bound and a non-negative width.

## Specification

- If `x` is below `lo`, return `lo`.
- If `x` is above `lo + width`, return `lo + width`.
- Otherwise return `x` unchanged.
- The function is pure (no side effects).

## Preconditions

- `width >= 0` (encoded in `width`'s refinement type).

## Contract

The compiler must be able to prove at compile time that:

- `result >= lo`
- `result <= lo + width`

## Signature (provided — do not change)

```aether
## Clamp x into the window [lo, lo + width].
## Precondition: width >= 0 (encoded in the type of width).
## Contract: result >= lo AND result <= lo + width (proved at compile time).
fn window_clamp(x: Int, lo: Int, width: Int{w: w >= 0}) -> Int
  where result >= lo && result <= lo + width
  effects {} {
  0
}
```

Replace the stub body with a correct implementation.

**Difficulty:** expert

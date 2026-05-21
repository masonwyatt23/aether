# Task 10: Clamp to Unit Interval {0, 1}

Write a function `clamp_unit` that clamps any integer to the discrete unit interval `{0, 1}`.

## Specification

- If `x < 0`, return `0`.
- If `x > 1`, return `1`.
- If `x` is already `0` or `1`, return `x`.
- The function is pure (no side effects).

## Contract

The compiler must be able to prove at compile time that:
- The result is at least `0`
- The result is at most `1`

## Signature (provided — do not change)

```aether
fn clamp_unit(x: Int) -> Int
  where result >= 0 && result <= 1
  effects {} {
  # your implementation here
}
```

This is a simpler variant of `clamp` where the bounds are constants, so no parameter refinements are needed. A two-branch `if/else if/else` is the natural solution.

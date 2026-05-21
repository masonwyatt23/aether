# Task 06: Safe Non-Negative Division

Write a function `safe_div` that divides a non-negative integer `a` by a strictly positive integer `b`.

## Specification

- Return `a / b` (integer division, rounding toward zero).
- The function is pure (no side effects).

## Preconditions

Both preconditions are encoded as refinement types in the parameters:
- `a` must be non-negative: `a: Int{x: x >= 0}`
- `b` must be strictly positive: `b: Int{x: x > 0}`

A caller that passes a negative `a` or a zero/negative `b` is a **type error** — the compiler rejects it before the function is ever called.

## Contract

The compiler must be able to prove at compile time that:
- The result is non-negative (>= 0)

This follows from `a >= 0` and `b > 0`: dividing a non-negative number by a positive number yields a non-negative result.

## Signature (provided — do not change)

```aether
fn safe_div(a: Int{x: x >= 0}, b: Int{x: x > 0}) -> Int
  where result >= 0
  effects {} {
  # your implementation here
}
```

Replace `# your implementation here` with the body. It is a single expression.

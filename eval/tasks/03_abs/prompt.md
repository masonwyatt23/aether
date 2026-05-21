# Task 03: Absolute Value

Write a function `abs` that takes an integer `n` and returns its absolute value.

## Specification

- If `n >= 0`, return `n`.
- If `n < 0`, return `-n` (i.e., `0 - n`).
- The function is pure (no side effects).

## Contract

The compiler must be able to prove at compile time that:
- The result is always non-negative (>= 0)

## Signature (provided — do not change)

```aether
fn abs(n: Int) -> Int
  where result >= 0
  effects {} {
  # your implementation here
}
```

Replace `# your implementation here` with a correct body.

## Note on Aether syntax

In Aether, negation of an integer is written `0 - n` (subtraction from zero), not unary minus.

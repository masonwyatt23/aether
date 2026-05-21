# Task 13: Absolute Difference

Write a function `abs_diff` that returns the absolute value of the difference between two integers: `|a - b|`.

## Specification

- If `a >= b`, return `a - b`.
- Otherwise, return `b - a`.
- The function is pure (no side effects).

## Contract

The compiler must prove at compile time that:
- The result is non-negative (`result >= 0`).
- The result is at least `a - b` (i.e., it is not less than the signed difference in either direction).
- The result is at least `b - a` (symmetric bound).

Together, the last two postconditions mean `result` is a valid upper bound on both `a - b` and `b - a`, which is the defining property of the absolute difference.

The difficulty here: a naive body like `a - b` satisfies `result >= a - b` but fails `result >= b - a` when `b > a`. The solver will find the counterexample.

## Signature (provided — do not change)

```aether
fn abs_diff(a: Int, b: Int) -> Int
  where result >= 0 && result >= a - b && result >= b - a
  effects {} {
  # your implementation here
}
```

Replace `# your implementation here` with a correct body. This requires a single `if/else`.

**Difficulty:** medium

# Task 11: Monotonic Step Up

Write a function `step_up` that adds a non-negative step to an integer, guaranteeing the result never decreases.

## Specification

- Return `n + step`.
- The function is pure (no side effects).

## Precondition

The parameter `step` is constrained to be non-negative: `step: Int{x: x >= 0}`. A caller that passes a negative step is a **type error**.

## Contract

The compiler must be able to prove at compile time that:
- The result is at least `n` (the function is monotonically non-decreasing)

## Signature (provided — do not change)

```aether
fn step_up(n: Int, step: Int{x: x >= 0}) -> Int
  where result >= n
  effects {} {
  # your implementation here
}
```

Replace `# your implementation here` with the body. It is a single expression.

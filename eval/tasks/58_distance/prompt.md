# Task 58: Non-Negative Distance

Write a function `distance` that computes the absolute difference between two integers `a` and `b`, always returning a non-negative result.

## Specification

- Return `|a - b|` (the absolute value of `a - b`).
- The function is pure (no side effects).
- There are no preconditions — `a` and `b` may be any integers, in any order.

## Contract

The compiler must be able to prove at compile time that:
- The result is non-negative (>= 0)

## The trap

The naive body `a - b` only returns a non-negative value when `a >= b`. The solver finds the counterexample `{a=0, b=0}` immediately (it cannot prove `a - b >= 0` without the hypothesis `a >= b`). The correct solution must branch on which argument is larger and subtract in the right order.

## Signature (provided — do not change)

```aether
fn distance(a: Int, b: Int) -> Int
  where result >= 0
  effects {} {
  # your implementation here
}
```

Replace `# your implementation here` with the body.

**Difficulty:** hard

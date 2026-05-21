# Task 09: Lower Midpoint

Write a function `lower_mid` that, given two integers `a` and `b` with `b >= a`, returns a value that lies in the closed interval `[a, b]`.

## Specification

Specifically, return a value `v` such that `a <= v <= b`. One natural choice is:
- If `a == b`, return `a`.
- If `b >= a + 1`, return `a + 1` (the value just above the lower bound).
- The function is pure (no side effects).

## Precondition

The parameter `b` is constrained to be at least `a`: `b: Int{x: x >= a}`.

## Contract

The compiler must be able to prove at compile time that:
- The result is at least `a`
- The result is at most `b`

## Signature (provided — do not change)

```aether
fn lower_mid(a: Int, b: Int{x: x >= a}) -> Int
  where result >= a && result <= b
  effects {} {
  # your implementation here
}
```

## Hint

A two-branch `if/else if/else` is needed. The key insight: in the branch where `a >= b`, only `a` satisfies the contract (since `b >= a` and `a >= b` implies `a == b`). In the branch where `a + 1 <= b`, returning `a + 1` satisfies both bounds.

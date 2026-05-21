# Task 48: Normalize Pair

Write a function `normalize_pair` that combines two bounded integers and returns a value in `[0, 100]`.

## Specification

- `a` is in `[0, 50]` and `b` is in `[0, 50]` (encoded as parameter refinements).
- Compute the sum `m = a + b` (which lies in `[0, 100]`).
- Return `m` clamped to `[0, 100]` as a safety guard.
- The function is pure (no side effects).

## Contract

The compiler must prove at compile time:
- `result >= 0`
- `result <= 100`

## Signature (provided — do not change)

```aether
fn normalize_pair(a: Int{x: x >= 0 && x <= 50}, b: Int{x: x >= 0 && x <= 50}) -> Int
  where result >= 0 && result <= 100
  effects {} {
  # your implementation here
}
```

## Requirement

The body must:
1. **`let`-bind** `m = a + b` as an intermediate value.
2. Use an `if/else if/else` to clamp `m` to `[0, 100]`.

The solver infers `m` is in `[0, 100]` from the parameter refinements on `a` and `b`, but the explicit clamp structure makes the proof visible. The `let m = a + b` equality must be present for the solver to propagate bounds across the subsequent branches.

**Difficulty:** medium-hard

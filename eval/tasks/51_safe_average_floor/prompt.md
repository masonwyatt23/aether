# Task 51: Safe Average Floor

Write a function `safe_average_floor` that computes an approximation of the floor average of two values in `[0, 100]`, returning a result guaranteed to be in `[0, 100]`.

## Specification

- `a` and `b` are both in `[0, 100]` (encoded as parameter refinements).
- Compute `lo = min(a, b)` by branching on `a <= b`.
- Within each branch, compute `step = (larger - lo)` and `half = step / 2`.
- Return `lo + half` clamped to `[0, 100]`.
- The function is pure (no side effects).

## Contract

The compiler must prove at compile time:
- `result >= 0`
- `result <= 100`

## Signature (provided — do not change)

```aether
fn safe_average_floor(a: Int{x: x >= 0 && x <= 100}, b: Int{x: x >= 0 && x <= 100}) -> Int
  where result >= 0 && result <= 100
  effects {} {
  # your implementation here
}
```

## Requirement

The body must use an outer `if a <= b / else` to identify which argument is smaller, then inside each arm use **multiple `let` bindings** (`lo`, `step`, `half`, `mid`) followed by a safety clamp `if mid > 100 then 100 else if mid < 0 then 0 else mid`. The solver proves the final result in range from the combination of path conditions, parameter refinements, and the chain of let-equality hypotheses.

**Difficulty:** medium-hard

# Task 46: Accumulate Step

Write a function `accumulate_step` that adds a bounded delta to a starting value.

## Specification

- `start` is any integer.
- `delta` is in `[0, 10]` (encoded as a parameter refinement).
- Return `start + delta`.
- The function is pure (no side effects).

## Contract

The compiler must prove at compile time:
- `result >= start` (delta is non-negative, so the sum is at least `start`)
- `result <= start + 10` (delta is at most 10)

## Signature (provided — do not change)

```aether
fn accumulate_step(start: Int, delta: Int{d: d >= 0 && d <= 10}) -> Int
  where result >= start && result <= start + 10
  effects {} {
  # your implementation here
}
```

## Requirement

The body must **let-bind the result** of `start + delta` before returning it. Name the intermediate `stepped`. The solver threads the equality `stepped = start + delta` into its hypotheses to prove both postcondition clauses from the parameter refinement on `delta`.

**Difficulty:** medium-hard

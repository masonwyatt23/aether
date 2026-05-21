# Task 19: Bounded Double

Write a function `bounded_double` that doubles `x` but caps the result at `hi`.

## Specification

- If `2 * x <= hi`, return `2 * x`.
- Otherwise, return `hi`.
- The function is pure (no side effects).

## Preconditions

Two parameter refinements constrain the inputs:
- `x: Int{v: v >= 0}` — `x` is non-negative.
- `hi: Int{v: v >= x}` — `hi` is at least `x`.

The second precondition is essential: when `2 * x > hi`, the function returns `hi`, and the proof that `result >= x` relies on `hi >= x`. Without this refinement, the solver cannot discharge the lower-bound postcondition in the capped branch.

## Contract

The compiler must prove at compile time that:
- The result is at least `x` (`result >= x`) — doubling cannot produce a value below the input.
- The result is at most `hi` (`result <= hi`) — the cap is respected.

Both arms must be verified: in the `then` branch, `result = 2 * x` and the solver uses `2 * x <= hi` (from the branch condition) plus `x >= 0` to prove `2 * x >= x`. In the `else` branch, `result = hi` and the solver uses `hi >= x` (from the precondition) to prove `result >= x`.

## Signature (provided — do not change)

```aether
fn bounded_double(x: Int{v: v >= 0}, hi: Int{v: v >= x}) -> Int
  where result >= x && result <= hi
  effects {} {
  # your implementation here
}
```

Replace `# your implementation here` with a correct body. This requires a single `if/else`.

**Difficulty:** medium

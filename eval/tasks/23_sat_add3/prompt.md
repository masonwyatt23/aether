# Task 23: Saturating Add with Floor and Cap

Write a function `sat_add3` that computes `x + y` and then **clamps the sum** to the range `[floor, cap]`. This is a saturating addition where both an upper cap and a lower floor are applied.

## Specification

- Compute `x + y`.
- If the sum is less than `floor`, return `floor`.
- If the sum is greater than `cap`, return `cap`.
- Otherwise, return `x + y`.
- The function is pure (no side effects).

## Preconditions

- `y >= 0` (encoded as `y: Int{v: v >= 0}`)
- `cap >= floor` (encoded as `cap: Int{c: c >= floor}`)

## Contract

The compiler must be able to prove at compile time that:
- The result is at least `floor`
- The result is at most `cap`

## Signature (provided — do not change)

```aether
fn sat_add3(x: Int, y: Int{v: v >= 0}, floor: Int, cap: Int{c: c >= floor}) -> Int
  where result >= floor && result <= cap
  effects {} {
  # your implementation here
}
```

Replace `# your implementation here` with a correct body.

## Why this is hard

A naive implementation that returns `x + y` directly violates the contract — the solver finds a counterexample where the unclamped sum exceeds `cap` or falls below `floor`. The function requires three branches: one for each of the under-floor, over-cap, and in-range cases.

**Difficulty:** hard

# Task 22: Offset Clamp

Write a function `offset_clamp` that clamps an integer `x` to the range `[base, base + span]`, where the range is given as a **base point** and a **span** rather than explicit lower and upper bounds.

## Specification

- If `x < base`, return `base`.
- If `x > base + span`, return `base + span`.
- Otherwise, return `x`.
- The function is pure (no side effects).

## Precondition

The parameter `span` carries a refinement: `span >= 0`. This guarantees the range `[base, base + span]` contains at least the point `base`. The signature encodes this as `span: Int{s: s >= 0}`.

## Contract

The compiler must be able to prove at compile time that:
- The result is at least `base`
- The result is at most `base + span`

## Signature (provided — do not change)

```aether
fn offset_clamp(x: Int, base: Int, span: Int{s: s >= 0}) -> Int
  where result >= base && result <= base + span
  effects {} {
  # your implementation here
}
```

Replace `# your implementation here` with a correct body.

## Why this is hard

A naive implementation that simply returns `x` violates the contract — the solver finds a counterexample where `x < base` (so `x` is below the lower bound) or `x > base + span` (so `x` exceeds the upper bound). The correct solution requires three branches that handle under, over, and within range separately.

**Difficulty:** hard

# Task 49: Weighted Pick

Write a function `weighted_pick` that combines a base value and a weight, both bounded, into a result with a provable range.

## Specification

- `a` is in `[0, 10]` (encoded as a parameter refinement).
- `weight` is in `[0, 10]` (encoded as a parameter refinement).
- Return `a + weight`.
- The function is pure (no side effects).

## Contract

The compiler must prove at compile time:
- `result >= 0`
- `result <= 20`

## Signature (provided — do not change)

```aether
fn weighted_pick(a: Int{x: x >= 0 && x <= 10}, weight: Int{w: w >= 0 && w <= 10}) -> Int
  where result >= 0 && result <= 20
  effects {} {
  # your implementation here
}
```

## Requirement

The body must use **three `let` bindings in sequence**:
1. `let base = a`
2. `let w = weight`
3. `let total = base + w`

Then return `total`. The solver threads the chain of equalities `base = a`, `w = weight`, `total = base + w` into hypotheses. Combined with the parameter refinements, this proves `total >= 0 && total <= 20`.

**Difficulty:** medium-hard

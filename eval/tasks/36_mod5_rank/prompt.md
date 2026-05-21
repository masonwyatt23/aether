# Task 36: Mod-5 Rank

Write a function `mod5_rank` that takes any integer `x` and returns
`(x % 5) + 1`.

## Specification

- Compute `x % 5`, which is in the range `[0, 4]` for any integer `x`.
- Add 1 to produce a 1-based rank in `[1, 5]`.
- The function is pure (no side effects).

## Contract

The compiler must prove at compile time that:
- `result >= 1` — the result is at least 1
- `result < 6` — the result is strictly less than 6

These follow from the modulo bound `0 <= x%5 < 5` plus the +1 shift.
This is useful for assigning items to one of 5 ranked tiers with 1-based
indexing (e.g., quintile assignment).

## Signature (provided — do not change)

```aether
fn mod5_rank(x: Int) -> Int
  where result >= 1 && result < 6
  effects {} {
  # your implementation here
}
```

Replace `# your implementation here` with a single expression.

**Difficulty:** hard

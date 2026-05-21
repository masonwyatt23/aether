# Task 32: Mod-4 Slot

Write a function `mod4_slot` that takes any integer `x` and returns
`(x % 4) + 1`.

## Specification

- Compute `x % 4`, which is in the range `[0, 3]` for any integer `x`.
- Add 1 to produce a 1-based slot index in `[1, 4]`.
- The function is pure (no side effects).

## Contract

The compiler must prove at compile time that:
- `result >= 1` — the result is at least 1
- `result < 5` — the result is strictly less than 5

These follow from the modulo bound `0 <= x%4 < 4` plus the +1 shift.
This is a useful primitive for distributing work across exactly 4 buckets
using 1-based indexing.

## Signature (provided — do not change)

```aether
fn mod4_slot(x: Int) -> Int
  where result >= 1 && result < 5
  effects {} {
  # your implementation here
}
```

Replace `# your implementation here` with a single expression.

**Difficulty:** hard

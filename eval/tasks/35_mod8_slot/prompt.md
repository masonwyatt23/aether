# Task 35: Mod-8 Slot

Write a function `mod8_slot` that takes any integer `x` and returns
`(x % 8) + 1`.

## Specification

- Compute `x % 8`, which is in the range `[0, 7]` for any integer `x`.
- Add 1 to produce a 1-based slot index in `[1, 8]`.
- The function is pure (no side effects).

## Contract

The compiler must prove at compile time that:
- `result >= 1` — the result is at least 1
- `result < 9` — the result is strictly less than 9

These follow from the modulo bound `0 <= x%8 < 8` plus the +1 shift.
This is useful for assigning items to one of 8 partitions with 1-based
indexing, and requires genuine modulo reasoning to prove.

## Signature (provided — do not change)

```aether
fn mod8_slot(x: Int) -> Int
  where result >= 1 && result < 9
  effects {} {
  # your implementation here
}
```

Replace `# your implementation here` with a single expression.

**Difficulty:** hard

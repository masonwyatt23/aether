# Task 29: Mod-3 Offset

Write a function `mod3_offset` that takes any integer `x` and returns
`(x % 3) + 1`.

## Specification

- Compute `x % 3`, which is in the range `[0, 2]` for any integer `x`.
- Add 1 to shift the result into the range `[1, 3]`.
- The function is pure (no side effects).

## Contract

The compiler must prove at compile time that:
- `result >= 1` — the result is at least 1
- `result < 4` — the result is strictly less than 4

These follow from the modulo bound `0 <= x%3 < 3` plus the +1 shift.
A model that does not understand modulo bounds cannot satisfy this contract
with a trivial body.

## Signature (provided — do not change)

```aether
fn mod3_offset(x: Int) -> Int
  where result >= 1 && result < 4
  effects {} {
  # your implementation here
}
```

Replace `# your implementation here` with a single expression.

**Difficulty:** hard

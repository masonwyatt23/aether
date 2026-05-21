# Task 31: Clock Tick

Write a function `clock_tick` that takes any integer `h` (representing an
arbitrary hour count) and returns `(h % 12) + 1`.

## Specification

- Compute `h % 12`, which is in the range `[0, 11]` for any integer `h`.
- Add 1 to produce a 1-based clock hour in `[1, 12]`.
- The function is pure (no side effects).

## Contract

The compiler must prove at compile time that:
- `result >= 1` — the result is at least 1 (minimum clock hour)
- `result < 13` — the result is strictly less than 13 (maximum clock hour is 12)

These follow from the modulo bound `0 <= h%12 < 12` plus the +1 shift.
A model that cannot reason about modulo remainders cannot satisfy the
`result >= 1` constraint with a trivial body.

## Signature (provided — do not change)

```aether
fn clock_tick(h: Int) -> Int
  where result >= 1 && result < 13
  effects {} {
  # your implementation here
}
```

Replace `# your implementation here` with a single expression.

**Difficulty:** hard

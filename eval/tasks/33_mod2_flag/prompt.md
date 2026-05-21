# Task 33: Mod-2 Flag

Write a function `mod2_flag` that takes any integer `x` and returns
`(x % 2) + 1`.

## Specification

- Compute `x % 2`, which is in the range `[0, 1]` for any integer `x`.
- Add 1 to produce a 1-based flag value in `[1, 2]`.
- The function is pure (no side effects).

## Contract

The compiler must prove at compile time that:
- `result >= 1` — the result is at least 1
- `result < 3` — the result is strictly less than 3

These follow from the modulo bound `0 <= x%2 < 2` plus the +1 shift.
This is the tightest possible modulo range — the solver must know that
`x % 2` is always 0 or 1 (never negative, never >= 2).

## Signature (provided — do not change)

```aether
fn mod2_flag(x: Int) -> Int
  where result >= 1 && result < 3
  effects {} {
  # your implementation here
}
```

Replace `# your implementation here` with a single expression.

**Difficulty:** hard

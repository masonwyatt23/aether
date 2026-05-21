# Task 30: Mod-10 Shift

Write a function `mod10_shift` that takes any integer `n` and returns
`(n % 10) + 1`.

## Specification

- Compute `n % 10`, which is in the range `[0, 9]` for any integer `n`.
- Add 1 to shift the result into the range `[1, 10]`.
- The function is pure (no side effects).

## Contract

The compiler must prove at compile time that:
- `result >= 1` — the result is at least 1
- `result < 11` — the result is strictly less than 11

These follow from the modulo bound `0 <= n%10 < 10` plus the +1 shift.
This contract requires the solver to know the modulo remainder bound;
returning a constant `0` is immediately refuted.

## Signature (provided — do not change)

```aether
fn mod10_shift(n: Int) -> Int
  where result >= 1 && result < 11
  effects {} {
  # your implementation here
}
```

Replace `# your implementation here` with a single expression.

**Difficulty:** hard

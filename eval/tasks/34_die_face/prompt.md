# Task 34: Die Face

Write a function `die_face` that maps any integer `x` to a valid die face
value by computing `(x % 6) + 1`.

## Specification

- Compute `x % 6`, which is in the range `[0, 5]` for any integer `x`.
- Add 1 to produce a standard die face in `[1, 6]`.
- The function is pure (no side effects).

## Contract

The compiler must prove at compile time that:
- `result >= 1` — the result is at least 1 (minimum die face)
- `result < 7` — the result is strictly less than 7 (maximum die face is 6)

These follow from the modulo bound `0 <= x%6 < 6` plus the +1 shift.
A model that guesses the body without understanding modulo arithmetic will
fail the `result >= 1` constraint if it returns a constant `0`.

## Signature (provided — do not change)

```aether
fn die_face(x: Int) -> Int
  where result >= 1 && result < 7
  effects {} {
  # your implementation here
}
```

Replace `# your implementation here` with a single expression.

**Difficulty:** hard

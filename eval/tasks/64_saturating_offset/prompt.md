# Task 64: Saturating Offset

Add `step` to `x`, but saturate at the ceiling `cap` so the result
never climbs above it.

## Specification

- If `x + step` would exceed `cap`, return `cap`.
- Otherwise return `x + step`.
- The function is pure (no side effects).

## Preconditions

- `x` is within the ceiling: `0 <= x <= cap` (encoded in `x`'s type).
- `cap >= 0` (encoded in `cap`'s type).
- `step >= 0` (encoded in `step`'s type).

## Contract

The compiler must be able to prove at compile time that:

- `result >= x`
- `result <= cap`

## The trap

Returning `x + step` directly satisfies `result >= x` but violates
`result <= cap` as soon as `step` pushes the sum past the ceiling. The sum
must be capped at `cap`.

## Signature (provided — do not change)

```aether
## Add step to x, saturating at the ceiling cap.
## Preconditions: 0 <= x <= cap (in x's type), cap >= 0, step >= 0.
## Contract: result >= x AND result <= cap (proved at compile time).
fn saturating_offset(x: Int{v: v >= 0 && v <= cap}, cap: Int{c: c >= 0}, step: Int{s: s >= 0}) -> Int
  where result >= x && result <= cap
  effects {} {
  0
}
```

Replace the stub body with a correct implementation.

**Difficulty:** expert

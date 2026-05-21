# Task 47: Clamp and Offset

Write a function `clamp_and_offset` that clamps `x` to `[lo, hi]` and then adds a fixed `offset`.

## Specification

- Clamp `x` to the range `[lo, hi]` (using the same logic as `clamp`).
- Add `offset` to the clamped value.
- Return the shifted result.
- The function is pure (no side effects).

## Precondition

`hi: Int{h: h >= lo}` — the caller must supply a valid range.

## Contract

The compiler must prove at compile time:
- `result >= lo + offset`
- `result <= hi + offset`

## Signature (provided — do not change)

```aether
fn clamp_and_offset(x: Int, lo: Int, hi: Int{h: h >= lo}, offset: Int) -> Int
  where result >= lo + offset && result <= hi + offset
  effects {} {
  # your implementation here
}
```

## Requirement

The body must use **two `let` bindings inside each `if`/`else` arm**: first `let c = <clamped_value>`, then `let r = c + offset`. Return `r`. This two-step let chain threads the path condition and the offset arithmetic together into a form the solver can prove.

**Difficulty:** medium-hard

# Task 50: Two-Stage Bound

Write a function `two_stage_bound` that enforces a range bound in two explicit stages: first handle the under-floor case, then handle the over-ceiling case, with an inner branch in the "in-range" arm.

## Specification

- If `x < lo`, the result is `lo` (floor clamp).
- If `x > hi`, the result is `hi` (ceiling clamp).
- Otherwise, apply an inner `if/else if/else` on the already-in-range `x`.
- Return a value in `[lo, hi]`.
- The function is pure (no side effects).

## Precondition

`hi: Int{h: h >= lo}` — the caller must supply a valid range.

## Contract

The compiler must prove at compile time:
- `result >= lo`
- `result <= hi`

## Signature (provided — do not change)

```aether
fn two_stage_bound(x: Int, lo: Int, hi: Int{h: h >= lo}) -> Int
  where result >= lo && result <= hi
  effects {} {
  # your implementation here
}
```

## Requirement

The body must use **nested `if/else` with a `let` binding in each outer arm** and a **secondary `if/else`** in the in-range arm. Specifically:
- Outer `if x < lo`: `let stage1 = lo`, then inner `if stage1 > hi then hi else stage1`.
- Outer `else if x > hi`: `let stage1 = hi`, then inner `if stage1 < lo then lo else stage1`.
- Outer `else`: `let stage1 = x`, then inner three-way clamp on `stage1`.

This structure requires the solver to combine outer path conditions with inner `let`-equality hypotheses to discharge the postcondition at each leaf.

**Difficulty:** medium-hard

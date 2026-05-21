# Task 45: Clamp via Let

Write a function `clamp_via_let` that takes an integer `x` and a range `[lo, hi]` and returns `x` clamped to that range.

## Specification

- If `x < lo`, return `lo`.
- If `x > hi`, return `hi`.
- Otherwise, return `x`.
- The function is pure (no side effects).

## Precondition

`hi: Int{h: h >= lo}` — the caller must supply a valid range.

## Contract

The compiler must prove at compile time:
- `result >= lo`
- `result <= hi`

## Signature (provided — do not change)

```aether
fn clamp_via_let(x: Int, lo: Int, hi: Int{h: h >= lo}) -> Int
  where result >= lo && result <= hi
  effects {} {
  # your implementation here
}
```

## Requirement

Unlike the one-liner clamp, this version must use **`let` bindings inside each `if`/`else` arm** to name the intermediate result before returning it. Each branch should bind `let bounded = <chosen_value>` and then return `bounded`. This exercises the solver's ability to thread `let`-equality hypotheses into refinement proofs.

**Difficulty:** medium-hard

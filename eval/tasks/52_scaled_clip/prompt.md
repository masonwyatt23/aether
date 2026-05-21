# Task 52: Scaled Clip (Helper + Main)

This task has **two functions**: a helper `clip_to_cap` and a main function `scaled_clip`. The model must implement both so that the main function's contract verifies.

## Helper: `clip_to_cap`

- Takes `x: Int` and `cap: Int{c: c >= 0}`.
- Returns `x` if `x <= cap`, else `cap`.
- No contract on the helper itself.

## Main function: `scaled_clip`

- Takes `x: Int`, `base: Int`, and `max_cap: Int{c: c >= 0}`.
- Clips `x` to the range `[0, max_cap]`.
- Returns `base + clipped_x`.

## Contract

The compiler must prove at compile time:
- `result >= base`
- `result <= base + max_cap`

## Signature (provided — do not change)

```aether
fn clip_to_cap(x: Int, cap: Int{c: c >= 0}) -> Int
  effects {} {
  # your implementation here
}

fn scaled_clip(x: Int, base: Int, max_cap: Int{c: c >= 0}) -> Int
  where result >= base && result <= base + max_cap
  effects {} {
  # your implementation here
}
```

## Requirement

The `scaled_clip` body must use an `if/else if/else` with **two `let` bindings in each arm** (`clipped` then `r = base + clipped`), returning `r`. This structure gives the solver enough let-equality and path-condition hypotheses to prove `r >= base && r <= base + max_cap` at every leaf. The helper `clip_to_cap` should be implemented as a simple two-branch function; note that the main function's proof does not rely on the helper's contract (since the solver does not inline across call boundaries), so the main function must contain its own explicit branches.

**Difficulty:** medium-hard

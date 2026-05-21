# Task 28: Safe Subtraction with Clamp

Write a function `safe_sub_clamp` that computes `a - b` and then **clamps the difference** to the range `[lo, hi]`. Because `b >= 0`, the subtraction can only decrease or maintain `a`, but the result might still fall outside `[lo, hi]` — hence the clamp.

## Specification

- Compute `a - b`.
- If the difference is less than `lo`, return `lo`.
- If the difference is greater than `hi`, return `hi`.
- Otherwise, return `a - b`.
- The function is pure (no side effects).

## Preconditions

- `b >= 0` (encoded as `b: Int{v: v >= 0}`)
- `hi >= lo` (encoded as `hi: Int{h: h >= lo}`)

## Contract

The compiler must be able to prove at compile time that:
- The result is at least `lo`
- The result is at most `hi`

## Signature (provided — do not change)

```aether
fn safe_sub_clamp(a: Int, b: Int{v: v >= 0}, lo: Int, hi: Int{h: h >= lo}) -> Int
  where result >= lo && result <= hi
  effects {} {
  # your implementation here
}
```

Replace `# your implementation here` with a correct body.

## Why this is hard

A naive implementation that returns `a - b` directly violates the contract — the solver finds a counterexample where the difference is outside `[lo, hi]`. The function requires three branches that branch on the computed difference `a - b` (not on `a` or `b` individually). Getting the branch conditions right (`< lo` and `> hi`) is critical: using `<= lo` or similar off-by-one variants produces a wrong implementation that the verifier catches.

**Difficulty:** hard

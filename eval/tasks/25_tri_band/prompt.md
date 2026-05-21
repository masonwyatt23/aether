# Task 25: Tri-Band Clamp

Write a function `tri_band` that clamps an integer `x` to the range `[b, c]`. The function receives an extra parameter `a` that acts as a strict lower guard (satisfying `a < b`), but the clamp target is `[b, c]`, **not** `[a, c]`.

## Specification

- If `x < b`, return `b`.
- If `x > c`, return `c`.
- Otherwise, return `x`.
- The function is pure (no side effects).

## Preconditions

- `b >= a + 1` (so `b > a`, encoded as `b: Int{q: q >= a + 1}`)
- `c >= b` (encoded as `c: Int{r: r >= b}`)

## Contract

The compiler must be able to prove at compile time that:
- The result is at least `b`
- The result is at most `c`

## Signature (provided — do not change)

```aether
fn tri_band(x: Int, a: Int, b: Int{q: q >= a + 1}, c: Int{r: r >= b}) -> Int
  where result >= b && result <= c
  effects {} {
  # your implementation here
}
```

Replace `# your implementation here` with a correct body.

## Why this is hard

The presence of `a` is a **distractor**: a model that uses `a` as the lower clamp bound (clamping to `[a, c]` instead of `[b, c]`) violates the contract. When the lower branch returns `a`, the solver finds a counterexample — `a < b`, so `a` fails `result >= b`. The correct implementation ignores `a` and clamps to `[b, c]` only.

**Difficulty:** hard

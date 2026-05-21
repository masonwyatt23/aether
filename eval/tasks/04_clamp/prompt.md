# Task 04: Clamp to a Range

Write a function `clamp` that takes an integer `x` and a range `[lo, hi]` and returns `x` clamped to that range.

## Specification

- If `x < lo`, return `lo`.
- If `x > hi`, return `hi`.
- Otherwise, return `x`.
- The function is pure (no side effects).

## Precondition

The parameter `hi` carries a refinement: `hi >= lo`. This means the caller is required to supply a valid range (upper bound at least the lower bound). The signature encodes this as `hi: Int{h: h >= lo}`.

## Contract

The compiler must be able to prove at compile time that:
- The result is at least `lo`
- The result is at most `hi`

## Signature (provided — do not change)

```aether
fn clamp(x: Int, lo: Int, hi: Int{h: h >= lo}) -> Int
  where result >= lo && result <= hi
  effects {} {
  # your implementation here
}
```

Replace `# your implementation here` with a correct body. This requires a two-branch `if/else if/else`.

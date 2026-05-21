# Task 60: Bounded Addition (Saturating Add)

Write a function `add_bounded` that adds two non-negative integers `a` and `b`, both bounded by `n`, and returns the sum clamped to `n`. This is a saturating addition: if the true sum would exceed `n`, return `n` instead.

## Specification

- If `a + b <= n`, return `a + b`.
- If `a + b > n`, return `n`.
- The function is pure (no side effects).

## Preconditions

- `n >= 1` (encoded in `n`'s refinement type).
- `a` is in `[0, n]` (encoded in `a`'s refinement type).
- `b` is in `[0, n]` (encoded in `b`'s refinement type).

## Contract

The compiler must be able to prove at compile time that:
- The result is non-negative (>= 0)
- The result is at most `n`

## The trap

The naive body `a + b` proves `result >= 0` (sum of non-negatives), but fails `result <= n` because the sum of two values in `[0, n]` can reach `2n`. For example, when `a = n` and `b = n`, `a + b = 2n > n`. The solver finds this immediately. The correct solution must clamp the sum with an explicit `if`.

## Signature (provided — do not change)

```aether
fn add_bounded(n: Int{v: v >= 1}, a: Int{v: v >= 0 && v <= n}, b: Int{v: v >= 0 && v <= n}) -> Int
  where result >= 0 && result <= n
  effects {} {
  # your implementation here
}
```

Replace `# your implementation here` with the body.

**Difficulty:** hard

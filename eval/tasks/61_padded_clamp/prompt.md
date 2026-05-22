# Task 61: Padded Clamp

Clamp `x` into a window that is inset by `pad` units on both sides of
the outer range `[lo, hi]`.

## Specification

- The effective window is `[lo + pad, hi - pad]`.
- If `x` is below `lo + pad`, return `lo + pad`.
- If `x` is above `hi - pad`, return `hi - pad`.
- Otherwise return `x` unchanged.
- The function is pure (no side effects).

## Preconditions

- `pad >= 0` (encoded in `pad`'s refinement type).
- `hi >= lo + 2 * pad` (encoded in `hi`'s refinement type). This guarantees
  `lo + pad <= hi - pad`, so the padded window is non-empty.

## Contract

The compiler must be able to prove at compile time that:

- `result >= lo + pad`
- `result <= hi - pad`

## Signature (provided — do not change)

```aether
## Clamp x into the padded window [lo + pad, hi - pad].
## Precondition: hi >= lo + 2*pad, so the padded window is non-empty.
## Contract: result >= lo + pad AND result <= hi - pad (proved at compile time).
fn padded_clamp(x: Int, lo: Int, pad: Int{p: p >= 0}, hi: Int{h: h >= lo + 2 * pad}) -> Int
  where result >= lo + pad && result <= hi - pad
  effects {} {
  0
}
```

Replace the stub body with a correct implementation.

**Difficulty:** expert

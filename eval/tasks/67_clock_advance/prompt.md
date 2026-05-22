# Task 67: Advance a Clock

Advance a clock reading by `d` ticks on a `k`-hour face, returning a
1-based hour in `1 .. k`.

## Specification

- Hours are numbered `1, 2, ..., k`.
- Return `((t + d) % k) + 1`.
- The function is pure (no side effects).

## Preconditions

- `k >= 1` (encoded in `k`'s refinement type).

## Contract

The compiler must be able to prove at compile time that:

- `result >= 1`
- `result < k + 1`

## The trap

Forgetting the modulo lets `t + d` run off the face and fails
`result < k + 1`. Forgetting the `+ 1` yields a 0-based hour and fails
`result >= 1`. Both are needed to land in `1 .. k`.

## Signature (provided — do not change)

```aether
## Advance a clock reading by d ticks on a k-hour face (1-based).
## Precondition: k >= 1 (encoded in the type of k).
## Contract: result >= 1 AND result < k + 1 (proved at compile time).
fn clock_advance(t: Int, d: Int, k: Int{v: v >= 1}) -> Int
  where result >= 1 && result < k + 1
  effects {} {
  0
}
```

Replace the stub body with a correct implementation.

**Difficulty:** expert

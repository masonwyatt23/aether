# Task 66: Modular Offset Ring

Map an arbitrary integer `x` into the `k`-element ring that starts at
`base`: the values `base, base + 1, ..., base + k - 1`.

## Specification

- Return `(x % k) + base`.
- The function is pure (no side effects).

## Preconditions

- `k >= 1` (encoded in `k`'s refinement type).

## Contract

The compiler must be able to prove at compile time that:

- `result >= base`
- `result < base + k`

## The trap

`x % k` alone lands in `[0, k)` and fails `result >= base` whenever
`base > 0`. `x + base` alone never wraps and fails `result < base + k`.
Both the modulo *and* the offset are required.

## Signature (provided — do not change)

```aether
## Map x into the k-element ring starting at base.
## Precondition: k >= 1 (encoded in the type of k).
## Contract: result >= base AND result < base + k (proved at compile time).
fn mod_offset(x: Int, k: Int{v: v >= 1}, base: Int) -> Int
  where result >= base && result < base + k
  effects {} {
  0
}
```

Replace the stub body with a correct implementation.

**Difficulty:** expert

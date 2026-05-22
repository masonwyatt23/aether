# Task 62: Mirror Index in a Range

Given a valid index `i` in the inclusive range `[a, b]`, return its
mirror — the index that sits the same distance from the *other* end.

## Specification

- The mirror of `i` in `[a, b]` is `a + b - i`.
- The function is pure (no side effects).

## Preconditions

- `i` is in range: `a <= i <= b` (encoded in `i`'s refinement type).
- `b >= a` (encoded in `b`'s refinement type).

## Contract

The compiler must be able to prove at compile time that:

- `result >= a`
- `result <= b`

## The trap

The single-range formula `b - i` only works when `a = 0`. Once `a` is
non-zero it is wrong: for `i = a` it returns `b - a`, which can fall outside
`[a, b]` entirely. The solver finds the counterexample `{a=0, b=0, i=0}`
against a naive variant. The mirror must fold the lower bound back in:
`a + b - i`.

## Signature (provided — do not change)

```aether
## Return the mirror of index i within the inclusive range [a, b].
## Preconditions: a <= i <= b (in i's type), b >= a (in b's type).
## Contract: result >= a AND result <= b (proved at compile time).
fn mirror_in_range(i: Int{v: v >= a && v <= b}, a: Int, b: Int{h: h >= a}) -> Int
  where result >= a && result <= b
  effects {} {
  0
}
```

Replace the stub body with a correct implementation.

**Difficulty:** expert

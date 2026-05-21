# Task 55: Mirror Index

Write a function `mirror_index` that, given a valid index `i` into an array of length `len`, returns the "mirror" index that is equidistant from the other end — i.e. the index of the element in the same position counted from the back.

## Specification

- Return the mirror index of `i` in an array of length `len`.
- For index `i`, the mirror is the index at distance `i` from the end.
- The function is pure (no side effects).

## Preconditions

- `i` is a valid index: `0 <= i <= len - 1` (encoded in `i`'s refinement type).
- `len` is strictly positive: `len >= 1` (encoded in `len`'s refinement type).

## Contract

The compiler must be able to prove at compile time that:
- The result is a valid index: `result >= 0` and `result <= len - 1`

## The trap

The naive formula `len - i` is off by one. When `i = 0` it returns `len`, which is one past the last valid index. The solver finds the counterexample `{i=0, len=1}` where `len - i = 1` but the valid range is `[0, 0]`. The correct formula subtracts an additional 1: `len - 1 - i`.

## Signature (provided — do not change)

```aether
fn mirror_index(i: Int{v: v >= 0 && v <= len - 1}, len: Int{v: v >= 1}) -> Int
  where result >= 0 && result <= len - 1
  effects {} {
  # your implementation here
}
```

Replace `# your implementation here` with the body. It is a single expression.

**Difficulty:** hard

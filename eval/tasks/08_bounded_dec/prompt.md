# Task 08: Bounded Decrement

Write a function `bounded_dec` that decrements an integer `x` by 1, given that a floor `lo` is at most `x`.

## Specification

- Return `x - 1`.
- The function is pure (no side effects).

## Precondition

The parameter `lo` is constrained to be at most `x`: `lo: Int{l: l <= x}`. This gives the solver the hypothesis `lo <= x` at every point in the body.

## Contract

The compiler must be able to prove at compile time that:
- The result is at most `x` (it decreased)
- The result is at least `lo - 1` (it did not go below the floor by more than 1)

## Signature (provided — do not change)

```aether
fn bounded_dec(x: Int, lo: Int{l: l <= x}) -> Int
  where result <= x && result >= lo - 1
  effects {} {
  # your implementation here
}
```

Replace `# your implementation here` with the body. It is a single expression.

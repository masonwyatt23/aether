# Task 07: Bounded Increment

Write a function `bounded_inc` that increments an integer `x` by 1, given that a ceiling `hi` is at least `x`.

## Specification

- Return `x + 1`.
- The function is pure (no side effects).

## Precondition

The parameter `hi` is constrained to be at least `x`: `hi: Int{h: h >= x}`. This gives the solver the hypothesis `hi >= x` at every point in the body.

## Contract

The compiler must be able to prove at compile time that:
- The result is at least `x` (it increased)
- The result is at most `hi + 1` (it did not exceed the ceiling by more than 1)

## Signature (provided — do not change)

```aether
fn bounded_inc(x: Int, hi: Int{h: h >= x}) -> Int
  where result >= x && result <= hi + 1
  effects {} {
  # your implementation here
}
```

Replace `# your implementation here` with the body. It is a single expression.

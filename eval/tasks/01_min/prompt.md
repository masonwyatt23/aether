# Task 01: Minimum of Two Integers

Write a function `min` that takes two integers `a` and `b` and returns the smaller of the two.

## Specification

- If `a <= b`, return `a`.
- If `b < a`, return `b`.
- The function is pure (no side effects).

## Contract

The compiler must be able to prove at compile time that:
- The result is less than or equal to `a`
- The result is less than or equal to `b`

## Signature (provided — do not change)

```aether
fn min(a: Int, b: Int) -> Int
  where result <= a && result <= b
  effects {} {
  # your implementation here
}
```

Replace `# your implementation here` with a correct body. The solution should be a single expression — an `if/else` is idiomatic.

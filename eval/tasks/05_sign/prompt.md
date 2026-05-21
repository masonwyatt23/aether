# Task 05: Sign Function

Write a function `sign` that returns the sign of an integer: `-1` for negatives, `0` for zero, `1` for positives.

## Specification

- If `n > 0`, return `1`.
- If `n < 0`, return `-1`.
- If `n == 0`, return `0`.
- The function is pure (no side effects).

## Contract

The compiler must be able to prove at compile time that:
- The result is at least -1
- The result is at most 1

## Signature (provided — do not change)

```aether
fn sign(n: Int) -> Int
  where result >= -1 && result <= 1
  effects {} {
  # your implementation here
}
```

Replace `# your implementation here` with a correct body.

## Note on Aether syntax

Aether does not support unary minus. Use `0 - 1` to express the literal `-1`.

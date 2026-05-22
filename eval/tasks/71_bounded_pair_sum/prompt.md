# Task 71: Bounded Pair Sum

Sum two values, each drawn from the range `[0, cap]`.

## Specification

- Return `x + y`.
- The function is pure (no side effects).

## Preconditions

- `0 <= x <= cap` (encoded in `x`'s type).
- `0 <= y <= cap` (encoded in `y`'s type).
- `cap >= 0` (encoded in `cap`'s type).

## Contract

The compiler must be able to prove at compile time that:

- `result >= x`
- `result <= 2 * cap`

## Signature (provided — do not change)

```aether
## Sum two values, each drawn from [0, cap].
## Preconditions: 0 <= x <= cap, 0 <= y <= cap, cap >= 0.
## Contract: result >= x AND result <= 2 * cap (proved at compile time).
fn bounded_pair_sum(x: Int{v: v >= 0 && v <= cap}, y: Int{v: v >= 0 && v <= cap}, cap: Int{c: c >= 0}) -> Int
  where result >= x && result <= 2 * cap
  effects {} {
  0
}
```

Replace the stub body with a correct implementation.

**Difficulty:** expert

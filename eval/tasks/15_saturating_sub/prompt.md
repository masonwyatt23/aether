# Task 15: Saturating Subtraction

Write a function `saturating_sub` that computes `a - b` but saturates at zero: if the result would be negative, return `0` instead.

## Specification

- If `a > b`, return `a - b`.
- Otherwise, return `0`.
- The function is pure (no side effects).

## Preconditions

Both `a` and `b` are non-negative integers, encoded in their refinement types:
- `a: Int{x: x >= 0}`
- `b: Int{x: x >= 0}`

## Contract

The compiler must prove at compile time that:
- The result is non-negative (`result >= 0`).
- The result does not exceed `a` (`result <= a`) — i.e., subtraction never increases the value.
- The result is at least `a - b` (`result >= a - b`) — the saturation is tight; the only deviation from `a - b` is clamping upward to `0`.

The three-part contract is the key difficulty. The third clause (`result >= a - b`) means the function cannot simply return `0` always — the body must actually compute `a - b` in the branch where `a > b`. The parameter refinements (`a >= 0`, `b >= 0`) are needed by the solver to discharge `result <= a` in the `else` branch.

## Signature (provided — do not change)

```aether
fn saturating_sub(a: Int{x: x >= 0}, b: Int{x: x >= 0}) -> Int
  where result >= 0 && result <= a && result >= a - b
  effects {} {
  # your implementation here
}
```

Replace `# your implementation here` with a correct body. This requires a single `if/else`.

**Difficulty:** medium

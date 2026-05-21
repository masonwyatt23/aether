# Task 54: Safe Predecessor (Decrement Floored at Zero)

Write a function `safe_pred` that decrements a non-negative integer by 1, returning 0 if the input is already 0.

## Specification

- If `x > 0`, return `x - 1`.
- If `x == 0`, return `0`.
- The function is pure (no side effects).

## Precondition

The parameter `x` carries a refinement: `x >= 0`. A caller that passes a negative value is a type error.

## Contract

The compiler must be able to prove at compile time that:
- The result is non-negative (>= 0)

## The trap

The naive body `x - 1` proves `result = x - 1`. From `x >= 0` the solver can check whether `x - 1 >= 0` — it cannot, because at `x = 0` the result is `-1`. The solver finds the counterexample `{x=0}`. The correct solution must guard the zero case explicitly.

## Signature (provided — do not change)

```aether
fn safe_pred(x: Int{v: v >= 0}) -> Int
  where result >= 0
  effects {} {
  # your implementation here
}
```

Replace `# your implementation here` with the body.

**Difficulty:** hard

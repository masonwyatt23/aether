# Task 12: Bounded Counter Tick

Write a function `counter_tick` that increments a bounded counter by 1. The counter is constrained to be in the range `[0, 99]`, and the result is proven to be in `[1, 100]`.

## Specification

- Return `c + 1`.
- The function is pure (no side effects).

## Precondition

The parameter `c` is constrained to `[0, 99]` by its refinement type: `c: Int{x: x >= 0 && x <= 99}`. A caller that passes a value outside this range is a **type error**.

## Contract

The compiler must be able to prove at compile time that:
- The result is at least `1` (strictly positive after increment)
- The result is at most `100` (did not overflow the bounded range)

## Signature (provided — do not change)

```aether
fn counter_tick(c: Int{x: x >= 0 && x <= 99}) -> Int
  where result >= 1 && result <= 100
  effects {} {
  # your implementation here
}
```

Replace `# your implementation here` with the body. It is a single expression.

## Note

The bounds `[0, 99]` were chosen deliberately (not `[0, 100]`) so the solver can discharge both sides of the postcondition using only linear arithmetic, without needing strict-to-non-strict integer conversion.

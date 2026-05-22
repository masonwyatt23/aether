# Task 70: Monotone Cube

Return the cube of a non-negative integer `x`.

## Specification

- Return `x * x * x`.
- The function is pure (no side effects).

## Preconditions

- `x >= 0` (encoded in `x`'s refinement type).

## Contract

The compiler must be able to prove at compile time that:

- `result >= 0`
- `result >= x`

## Note

Both goals are non-linear in `x`. Aether's core solver decides linear
arithmetic only and escalates a goal like this to the `z3` SMT solver, which
must be installed on `PATH`.

## Signature (provided — do not change)

```aether
## Return x cubed, for non-negative x.
## Precondition: x >= 0 (encoded in the type of x).
## Contract: result >= 0 AND result >= x (proved at compile time).
fn cube_monotone(x: Int{v: v >= 0}) -> Int
  where result >= 0 && result >= x
  effects {} {
  0
}
```

Replace the stub body with a correct implementation.

**Difficulty:** expert

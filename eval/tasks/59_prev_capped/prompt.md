# Task 59: Decrement Clamped to Floor

Write a function `prev_capped` that decrements `x` by 1, but clamps the result at a lower bound `lo`. The result must never go below `lo`.

## Specification

- If `x > lo`, return `x - 1`.
- If `x == lo`, return `lo` (already at the floor, do not go below).
- The function is pure (no side effects).

## Precondition

The parameter `lo` carries a refinement: `lo <= x`. This guarantees the floor is at most `x`, so there is always a valid non-decreasing lower bound.

## Contract

The compiler must be able to prove at compile time that:
- The result is at least `lo` (the floor was not breached)

## The trap

The naive body `x - 1` fails when `x == lo`: the result is `lo - 1`, which is below the floor. The solver finds the counterexample `{lo=0, x=0}`. The correct solution must check whether `x` is already at the floor before decrementing.

## Signature (provided — do not change)

```aether
fn prev_capped(x: Int, lo: Int{l: l <= x}) -> Int
  where result >= lo
  effects {} {
  # your implementation here
}
```

Replace `# your implementation here` with the body.

**Difficulty:** hard

# Task 53: Increment Capped at Ceiling

Write a function `inc_capped` that increments `x` by 1, but the result must not exceed `hi`.

## Specification

- If `x` is strictly below `hi`, return `x + 1`.
- If `x` equals `hi`, return `hi` (do not exceed the ceiling).
- The function is pure (no side effects).

## Precondition

The parameter `hi` carries a refinement: `hi >= x`. This guarantees the ceiling is at least `x`, so a valid (non-decreasing) result always exists.

## Contract

The compiler must be able to prove at compile time that:
- The result is at least `x` (the value did not decrease)
- The result is at most `hi` (the ceiling was not exceeded)

## The trap

The obvious body `x + 1` satisfies `result >= x`, but it breaks `result <= hi` when `x == hi`: the result becomes `hi + 1`, which is one above the ceiling. The solver finds the counterexample `{hi=0, x=0}` immediately.

## Signature (provided — do not change)

```aether
fn inc_capped(x: Int, hi: Int{h: h >= x}) -> Int
  where result >= x && result <= hi
  effects {} {
  # your implementation here
}
```

Replace `# your implementation here` with the body.

**Difficulty:** hard

# Task 57: Clamp to Half-Open Interval

Write a function `clamp_exclusive` that clamps `x` into the half-open interval `[lo, hi)` — that is, `lo` is included but `hi` is excluded.

## Specification

- If `x < lo`, return `lo`.
- If `x >= hi`, return `hi - 1` (the largest value still inside the interval).
- Otherwise, return `x`.
- The function is pure (no side effects).

## Precondition

The parameter `hi` carries a refinement: `hi >= lo + 1`. This guarantees the interval is non-empty (at least one valid value, `lo`, exists).

## Contract

The compiler must be able to prove at compile time that:
- The result is at least `lo`
- The result is at most `hi - 1` (strictly below `hi`)

## The trap

The naive body copies a standard closed-interval clamp: `if x < lo then lo else if x > hi then hi else x`. This has two flaws: (1) the second branch returns `hi`, which violates `result <= hi - 1`; (2) even `x` in the else branch may equal `hi - 1` but the boundary check uses `>` instead of `>=`, so `x == hi` slips through. The solver detects `hi` returned in the middle branch: `hi <= hi - 1` is always false.

## Signature (provided — do not change)

```aether
fn clamp_exclusive(x: Int, lo: Int, hi: Int{h: h >= lo + 1}) -> Int
  where result >= lo && result <= hi - 1
  effects {} {
  # your implementation here
}
```

Replace `# your implementation here` with the body.

**Difficulty:** hard

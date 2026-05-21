# Task 21: Three-Way Comparison with Base Offset

Write a function `compare3` that compares integers `a` and `b` and returns a value offset from a given `base`: `base` for less-than, `base + 1` for equal, `base + 2` for greater-than.

## Specification

- If `a < b`, return `base`.
- If `a > b`, return `base + 2`.
- If `a == b`, return `base + 1`.
- The function is pure (no side effects).

## Contract

The compiler must be able to prove at compile time that:
- The result is at least `base`
- The result is at most `base + 2`

## Signature (provided — do not change)

```aether
fn compare3(a: Int, b: Int, base: Int) -> Int
  where result >= base && result <= base + 2
  effects {} {
  # your implementation here
}
```

Replace `# your implementation here` with a correct body.

## Why this is hard

Because `base` is unconstrained, the stub `{ 0 }` is immediately refuted — the solver finds a counterexample `base = 1` where `0 < base`. The function requires **three explicit branches** covering `a < b`, `a > b`, and `a == b`. Each branch must return a value in `[base, base + 2]`; any branch that returns a constant like `0` or `-1` will be refuted when `base` is large.

**Difficulty:** hard

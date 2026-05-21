# Task 41: Direction Vertical Delta

A compass direction carries a magnitude (number of steps).  The ADT is
already declared:

```aether
type Dir = North(Int) | South(Int) | East(Int) | West(Int)
```

## Specification

Compute the **vertical** displacement:

- `North(n)` → `n`  (positive = upward)
- `South(n)` → `0 - n`  (negative = downward)
- `East(n)`  → `0`  (no vertical component)
- `West(n)`  → `0`  (no vertical component)

The function is pure (no side effects).

## Exhaustiveness

Your `match` must handle **all four** constructors of `Dir`.  A
non-exhaustive match is a warning, which counts as unverified.

## Signature (provided — do not change)

```aether
type Dir = North(Int) | South(Int) | East(Int) | West(Int)

fn vertical(d: Dir) -> Int
  effects {} {
  # your implementation here
}
```

Replace `# your implementation here` with an exhaustive match over all four
constructors.

**Difficulty:** medium-hard

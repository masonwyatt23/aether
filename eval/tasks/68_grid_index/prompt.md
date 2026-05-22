# Task 68: Row-Major Grid Index

Compute the flat, row-major index of cell `(r, c)` in a `rows x cols`
grid.

## Specification

- Return `r * cols + c`.
- The function is pure (no side effects).

## Preconditions

- `r` is a valid row: `0 <= r < rows` (encoded in `r`'s type).
- `rows >= 1` (encoded in `rows`'s type).
- `c` is a valid column: `0 <= c < cols` (encoded in `c`'s type).
- `cols >= 1` (encoded in `cols`'s type).

## Contract

The compiler must be able to prove at compile time that:

- `result >= c`
- `result < rows * cols`

## Note

Proving `result < rows * cols` involves the non-linear term `rows * cols`.
Aether's core solver decides linear integer arithmetic only; for a goal like
this it escalates to the `z3` SMT solver, which must be installed on `PATH`.

## Signature (provided — do not change)

```aether
## Row-major flat index of cell (r, c) in a rows x cols grid.
## Preconditions: 0 <= r < rows, 0 <= c < cols, rows >= 1, cols >= 1.
## Contract: result >= c AND result < rows * cols (proved at compile time).
fn grid_index(r: Int{v: v >= 0 && v < rows}, rows: Int{n: n >= 1}, c: Int{v: v >= 0 && v < cols}, cols: Int{n: n >= 1}) -> Int
  where result >= c && result < rows * cols
  effects {} {
  0
}
```

Replace the stub body with a correct implementation.

**Difficulty:** expert

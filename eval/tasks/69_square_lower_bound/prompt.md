# Task 69: Square Lower Bound

Return the square of `x`, with a contract that pins down two facts a
square always satisfies.

## Specification

- Return `x * x`.
- The function is pure (no side effects).

## Contract

The compiler must be able to prove at compile time that:

- `result >= 0`
- `result >= x`

(For every integer `x`, `x * x >= x` — the gap `x * x - x = x * (x - 1)` is a
product of consecutive integers and so is never negative.)

## Note

Both goals are non-linear in `x`. Aether's core solver decides linear
arithmetic only and escalates a goal like this to the `z3` SMT solver, which
must be installed on `PATH`.

## Signature (provided — do not change)

```aether
## Return x squared. Contract holds for every integer x.
## Contract: result >= 0 AND result >= x (proved at compile time).
fn square_lower_bound(x: Int) -> Int
  where result >= 0 && result >= x
  effects {} {
  0
}
```

Replace the stub body with a correct implementation.

**Difficulty:** expert

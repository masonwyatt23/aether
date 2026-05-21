# Task 16: Range Span

Write a function `range_span` that returns the width of the closed range `[lo, hi]`, i.e., the value `hi - lo`.

## Specification

- Return `hi - lo`.
- The function is pure (no side effects).

## Precondition

The parameter `hi` carries a refinement: `hi >= lo`. This encodes the requirement that the range is valid (upper bound at least the lower bound), written as `hi: Int{x: x >= lo}`.

## Contract

The compiler must prove at compile time that:
- The result is non-negative (`result >= 0`).
- The result is at least `hi - lo` (`result >= hi - lo`) — the span is not under-reported.

Both postconditions together mean the only correct answer is exactly `hi - lo`. The non-negativity follows from the precondition `hi >= lo`. The second clause prevents returning a trivially large constant like `999`.

A stub returning `0` fails: when `hi = lo + 5`, the solver finds `0 >= 5` is false. The precondition `hi >= lo` is what makes `result >= 0` provable for the correct body — without it, the solver cannot discharge the non-negativity goal.

## Signature (provided — do not change)

```aether
fn range_span(lo: Int, hi: Int{x: x >= lo}) -> Int
  where result >= 0 && result >= hi - lo
  effects {} {
  # your implementation here
}
```

Replace `# your implementation here` with the body. It is a single expression.

**Difficulty:** medium

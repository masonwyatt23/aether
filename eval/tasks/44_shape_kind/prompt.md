# Task 44: Shape Perimeter Approximation

A geometric shape is described by an ADT.  The ADT is already declared:

```aether
type Shape = Circle(Int) | Square(Int) | Triangle(Int, Int)
```

`Circle(r)` has integer radius `r`; `Square(side)` has integer side length;
`Triangle(a, b)` is an isoceles triangle with two equal sides of length `a`
and base `b`.

## Specification

Compute an integer approximation of the perimeter:

- `Circle(r)`      → `6 * r`         (approximation: 2πr ≈ 6r)
- `Square(side)`   → `4 * side`
- `Triangle(a, b)` → `a + b + a`     (two equal sides plus base)

The function is pure (no side effects).

## Exhaustiveness

Your `match` must handle **all three** constructors: `Circle`, `Square`, and
`Triangle`.  A non-exhaustive match is a warning, which counts as unverified.

## Signature (provided — do not change)

```aether
type Shape = Circle(Int) | Square(Int) | Triangle(Int, Int)

fn perimeter(s: Shape) -> Int
  effects {} {
  # your implementation here
}
```

Replace `# your implementation here` with an exhaustive match over all three
constructors.

**Difficulty:** medium-hard

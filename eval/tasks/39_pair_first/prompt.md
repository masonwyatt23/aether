# Task 39: Pair or Single — First Element

A container can hold either a pair of integers or a single integer.  The ADT
is already declared:

```aether
type Pair = Pair(Int, Int) | Single(Int)
```

`Pair(a, b)` holds two values; `Single(x)` holds one.

## Specification

- If the container is `Pair(a, b)`, return `a` (the first element).
- If the container is `Single(x)`, return `x` (the only element).
- The function is pure (no side effects).

## Exhaustiveness

Your `match` must handle **both** constructors of `Pair`.  A non-exhaustive
match is a warning, which counts as unverified.

## Signature (provided — do not change)

```aether
type Pair = Pair(Int, Int) | Single(Int)

fn first(p: Pair) -> Int
  effects {} {
  # your implementation here
}
```

Replace `# your implementation here` with an exhaustive match.

**Difficulty:** medium-hard

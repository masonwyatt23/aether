# Task 40: Flat Tree Sum

A flat, two-level tree can be a leaf (one value) or a branch (two children,
both integers).  The ADT is already declared:

```aether
type Tree = Leaf(Int) | Branch(Int, Int)
```

`Leaf(v)` holds a single integer; `Branch(l, r)` holds two children.

## Specification

- If the tree is `Leaf(v)`, return `v`.
- If the tree is `Branch(l, r)`, return `l + r` (the sum of the two children).
- The function is pure (no side effects).

## Exhaustiveness

Your `match` must handle **both** constructors of `Tree`.  A non-exhaustive
match is a warning, which counts as unverified.

## Signature (provided — do not change)

```aether
type Tree = Leaf(Int) | Branch(Int, Int)

fn tree_sum(t: Tree) -> Int
  effects {} {
  # your implementation here
}
```

Replace `# your implementation here` with an exhaustive match.

**Difficulty:** medium-hard

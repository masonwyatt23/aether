# Task 72: Nested ADT Classification

Classify a two-level tree into one of four integer codes by
exhaustively matching its nested constructors. The ADTs are already declared:

```aether
type Leaf = Red(Int) | Blue(Int)
type Node = Tip(Leaf) | Fork(Leaf, Leaf)
```

## Specification

- `Tip(Red(_))`  => `1`
- `Tip(Blue(_))` => `2`
- `Fork(Red(_),  _)` => `3`
- `Fork(Blue(_), _)` => `4`  (a `Fork` is classified by its **first** child)
- The function is pure (no side effects).

## Exhaustiveness

The outer `match` must cover both `Node` constructors, and every inner
`match` must cover both `Leaf` constructors. A non-exhaustive `match` is a
compiler warning, which counts as unverified — so a stub that handles only
`Tip` does not pass.

## Signature (provided — do not change)

```aether
type Leaf = Red(Int) | Blue(Int)
type Node = Tip(Leaf) | Fork(Leaf, Leaf)

## Classify a two-level Node tree into a code 1..4.
## Every constructor of Leaf and Node must be matched (non-exhaustive => warning).
fn count(n: Node) -> Int
  effects {} {
  match n with {
    Tip(l) => 0
  }
}
```

Replace the stub body with a correct implementation.

**Difficulty:** expert

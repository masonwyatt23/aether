# Task 42: Comparison to Integer

A comparison result can carry a signed distance.  The ADT is already
declared:

```aether
type Cmp = Lt(Int) | Eq | Gt(Int)
```

`Lt(n)` means "less than, by distance n"; `Eq` means "equal"; `Gt(n)` means
"greater than, by distance n".

## Specification

Convert to a signed integer representing the direction and magnitude:

- `Lt(n)` → `0 - n`  (negative: went left/below)
- `Gt(n)` → `n`  (positive: went right/above)
- `Eq`    → `0`

The function is pure (no side effects).

## Exhaustiveness

Your `match` must handle **all three** constructors: `Lt`, `Gt`, and `Eq`.
`Eq` carries no payload so it parses as a variable pattern (wildcard); it
**must** appear after `Lt` and `Gt` so that those payload arms are matched
first.  A non-exhaustive match is a warning.

## Signature (provided — do not change)

```aether
type Cmp = Lt(Int) | Eq | Gt(Int)

fn cmp_value(c: Cmp) -> Int
  effects {} {
  # your implementation here
}
```

Replace `# your implementation here` with an exhaustive match.

**Difficulty:** medium-hard

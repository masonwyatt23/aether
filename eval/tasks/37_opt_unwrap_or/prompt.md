# Task 37: Option Unwrap-Or

Implement `unwrap_or` for a simple option type.  The ADT is already declared:

```aether
type Opt = Some(Int) | None
```

`Some(v)` wraps an integer value; `None` represents absence.

## Specification

- If the option is `Some(v)`, return `v`.
- If the option is `None`, return `default`.
- The function is pure (no side effects).

## Exhaustiveness

Your `match` must cover **both** constructors.  A non-exhaustive match on
a known ADT emits a warning, which counts as unverified in this benchmark.

## Signature (provided — do not change)

```aether
type Opt = Some(Int) | None

fn unwrap_or(o: Opt, default: Int) -> Int
  effects {} {
  # your implementation here
}
```

Replace `# your implementation here` with a match expression that handles
every constructor of `Opt`.

**Difficulty:** medium-hard

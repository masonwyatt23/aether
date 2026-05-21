# Task 38: Result Ok-or-Zero

Implement `ok_or_zero` for a result type that can carry either a success
value or an error code.  The ADT is already declared:

```aether
type Res = Ok(Int) | Err(Int)
```

`Ok(v)` represents success with value `v`; `Err(e)` represents failure with
error code `e`.

## Specification

- If the result is `Ok(v)`, return `v`.
- If the result is `Err(_)`, return `0` (discard the error code).
- The function is pure (no side effects).

## Exhaustiveness

Your `match` must cover **both** constructors of `Res`.  A non-exhaustive
match is a warning, which counts as unverified in this benchmark.

## Signature (provided — do not change)

```aether
type Res = Ok(Int) | Err(Int)

fn ok_or_zero(r: Res) -> Int
  effects {} {
  # your implementation here
}
```

Replace `# your implementation here` with a match expression covering both
`Ok` and `Err`.

**Difficulty:** medium-hard

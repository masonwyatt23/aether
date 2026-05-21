# Task 43: State Machine Code

A simple state machine has three states.  The ADT is already declared:

```aether
type State = Active(Int) | Idle | Done(Int)
```

`Active(n)` means the machine is running with step counter `n`; `Idle` means
it is paused; `Done(n)` means it finished with exit code `n`.

## Specification

Return a signed integer that encodes the state:

- `Active(n)` → `n`         (positive: how many steps taken)
- `Done(n)`   → `0 - n`     (negative: exit code negated)
- `Idle`      → `0`          (zero: paused)

The function is pure (no side effects).

## Exhaustiveness

Your `match` must handle **all three** constructors.  `Idle` carries no
payload; place it last in the match so the two payload arms (`Active`,
`Done`) are checked first.  A non-exhaustive match is a warning.

## Signature (provided — do not change)

```aether
type State = Active(Int) | Idle | Done(Int)

fn state_code(s: State) -> Int
  effects {} {
  # your implementation here
}
```

Replace `# your implementation here` with an exhaustive match.

**Difficulty:** medium-hard

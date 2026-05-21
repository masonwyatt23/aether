# Task 56: Step Toward Target

Write a function `step_toward` that moves `x` one step (of size 1) toward a target `t`, without overshooting. Since `t >= x` is guaranteed by the precondition, "toward" means adding 1 — but only if `x` has not yet reached `t`.

## Specification

- If `x < t`, return `x + 1`.
- If `x == t` (already at the target), return `t` (do not overshoot).
- The function is pure (no side effects).

## Precondition

The parameter `t` carries a refinement: `t >= x`. This restricts the domain to cases where the target is at or above `x`.

## Contract

The compiler must be able to prove at compile time that:
- The result is at least `x` (did not move backward)
- The result is at most `t` (did not overshoot the target)

## The trap

The naive body `x + 1` proves `result >= x`, but fails `result <= t` when `x == t`: the result becomes `t + 1`. The solver finds the counterexample `{t=0, x=0}`. The correct solution must check whether `x` has reached `t` before stepping.

## Signature (provided — do not change)

```aether
fn step_toward(x: Int, t: Int{v: v >= x}) -> Int
  where result >= x && result <= t
  effects {} {
  # your implementation here
}
```

Replace `# your implementation here` with the body.

**Difficulty:** hard

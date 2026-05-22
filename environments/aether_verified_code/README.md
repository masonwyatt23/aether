# aether-verified-code

A **`verifiers` RL environment for verified-code generation.** The model
writes [Aether](../../README.md) code; the Aether compiler — an *exact*
refinement-type oracle — scores it; the compiler's diagnostics (including
refutation counterexamples) are fed back for a retry.

## Why this environment exists

Every code RL environment today rewards on a **test suite** or an **LLM
judge**. Both can be reward-hacked — pass the visible tests, satisfy the
grader, without being correct. This environment's reward is the compiler
**proving** a refinement contract holds *for all inputs*. A proof cannot be
hacked. That makes it a rare thing: a **dense, verifiable, non-hackable
reward** for code generation.

Two more properties matter for RL:

- **Contamination-free by construction.** Tasks are procedurally generated
  (`aether_core.new_task(seed)`) from parametric templates — a seed maps to a
  never-committed instance. Train and eval on disjoint seed ranges and the
  model has provably never seen the data.
- **Mutation-audited specs.** Each task's `where` contract provably rejects
  every wrong mutant of its reference solution, so a rewarded solution
  genuinely satisfies the specification — there is no loose-spec loophole.

## Task

The model is shown a natural-language problem and an Aether function
signature carrying a fixed `where` contract. It must write a body (or, for
compositional tasks, several functions) the compiler can **prove** satisfies
the contract. It is a multi-turn loop: after each submission the model sees
`aether check`'s diagnostics and may retry, up to `max_turns`.

## Reward

A dense, graded ladder — partial progress earns partial reward, only a
compiler-proved solution earns the full 1.0:

| Outcome | Reward |
|---|---|
| `PARSE-ERROR` | 0.00 |
| `TYPE-ERROR` | 0.04 |
| `EFFECT-ERROR` | 0.08 |
| `UNPROVEN` (type-clean, contract unproved) | 0.15 + 0.45 × (contract conjuncts proved), capped 0.60 |
| `VERIFIED` (compiler proved the contract) | **1.00** |

The reward is the **high-water mark** across the rollout's turns — monotone,
so it cannot be farmed by oscillating the diagnostics. For a multi-clause
contract `where A && B && C`, the fraction of conjuncts independently proved
gives genuine partial credit (each conjunct is mutation-audited, so this is
real partial correctness, not spec-gaming). Only zero-errors-**and**-zero-
warnings counts as `VERIFIED`; a solver timeout is treated as unverified.

## Usage

```bash
prime env install aether-verified-code
prime eval run aether-verified-code -m <model> -n 20 -r 3
```

`load_environment` parameters:

- `num_tasks` (default 200) — fresh procedurally-generated tasks
- `max_turns` (default 5) — verifier-feedback rounds per task
- `seed` (default 0) — starting seed; use disjoint ranges for train vs. eval

Requires the `aether` compiler on `PATH` or at `target/release/aether` (set
`AETHER_BIN` to override). `aether_core.py` is framework-free and can be
unit-tested directly: `python3 aether_core.py <seed>`.

## Status & roadmap

- **Working:** task generation, exact-oracle scoring, dense reward,
  multi-turn verifier-feedback loop.
- **Throughput:** the environment calls `aether check` per submission
  (off the event loop). For high-throughput RL, an `aether serve` persistent
  mode (amortizing process startup; target ~200–1000 linear checks/sec/core)
  is the planned next step.
- **Curriculum depth:** the task families currently span linear-arithmetic,
  modular, compositional (`compose`), and termination-checked recursive
  (`recursive`) verification. Deeper families (quantified list contracts,
  data-structure invariants) track the Aether verifier's expressiveness.

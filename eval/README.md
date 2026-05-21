# Aether Verified-Code Benchmark

A small benchmark that measures something most coding benchmarks do not:
**whether AI-generated code is *provably* correct.**

## What it measures

HumanEval, MBPP, and similar benchmarks score a solution by running a hidden
test suite — "did the tests pass?" That answers *correct on the examples we
tried*. It does not answer *correct for all inputs*.

Aether's compiler proves refinement contracts at compile time. Each task here
ships a function **signature carrying a `where` contract** — a postcondition
the solution must satisfy for *every* input. A solution counts as solved only
when `aether check` reports **zero errors and zero warnings**: the compiler
has *proved* the contract, not merely failed to find a counterexample.

```
test-pass-rate benchmark:   "the 8 hidden tests passed"
this benchmark:             "the compiler proved result >= lo && result <= hi"
```

## Layout

```
eval/
  tasks/NN_name/
    prompt.md      — the natural-language task an AI model is given
    signature.ae   — the function signature + `where` contract; body stubbed
    reference.ae   — a known-correct solution (proves the contract)
  baseline/        — the reference solutions, one `<task>.ae` per task
  harness/
    run.py         — scorer: classifies each candidate
    generate.py    — optional: ask an LLM to solve the tasks
```

## Running it

Build the CLI once, then score the reference baseline (the harness
self-test — it must score 12/12):

```bash
cargo build --release -p aether-cli
python3 eval/harness/run.py eval/baseline
```

To score AI-generated solutions, put one `<task>.ae` file per task in a
directory and point the harness at it:

```bash
python3 eval/harness/run.py path/to/candidate-solutions
```

Or generate solutions with a model (needs `ANTHROPIC_API_KEY`):

```bash
export ANTHROPIC_API_KEY=sk-...
python3 eval/harness/generate.py candidates/run1
python3 eval/harness/run.py candidates/run1
```

## Scoring

Each candidate is run through `aether check --json` and classified:

| Outcome | Meaning |
|---|---|
| `VERIFIED` | type-checks, effect-checks, **contract proved** — counts as solved |
| `UNPROVEN` | type-checks but a postcondition is refuted or unprovable |
| `TYPE-ERROR` | the body does not type-check |
| `EFFECT-ERROR` | an undeclared effect is used |
| `PARSE-ERROR` | the candidate does not parse |
| `MISSING` | no candidate file for this task |

The score is `verified / total`.

## The 12 tasks

`min`, `max`, `abs`, `clamp`, `sign`, `safe_div`, `bounded_inc`,
`bounded_dec`, `lower_mid`, `clamp_unit`, `step_up`, `counter_tick` — each a
small integer function whose contract is a non-trivial refinement the
linear-arithmetic solver must actually work to discharge.

## Honest limitations

- **It is small.** Twelve tasks, all integer arithmetic. This is a research
  demonstration of an *idea* — verified-correctness scoring — not an
  industry-scale benchmark.
- **Contract coverage is bounded by the solver.** Aether proves linear
  integer arithmetic (and escalates non-linear goals to `z3` when installed).
  Tasks are chosen to stay inside what the solver decides; a richer benchmark
  would need a richer solver.
- **It rewards a narrow notion of correctness.** "The compiler proved this
  contract" is stronger than "tests passed" but only as meaningful as the
  contract. A weak contract is easy to satisfy; the value is in the contract
  design.

What it demonstrates is the *direction*: an AI coding evaluation can score
provable correctness, not just test-pass rate — and a compiler with a real
verifier can be the ground-truth oracle for it.

# Benchmark Results

A first run of the Aether verified-code benchmark (see [README.md](README.md))
against current frontier models. Each model was given only `prompt.md` +
`signature.ae` for each task and asked to return a complete Aether function;
the result was scored by `aether check` — a task counts as solved only when
the compiler **proved** its refinement contract.

Run date: 2026-05-21. Harness: `eval/harness/run.py`.

> **Note — the benchmark has since been expanded from 12 to 57 tasks**
> across four difficulty tiers (easy / medium / medium-hard / hard). The
> scores below are the **original 12-task (easy-tier) run**. The 45 harder
> tasks have not yet been run against models — that re-run is the next step,
> and it is where a score *spread* is expected to appear.

## Scores — original 12-task run

| Model | Provider | Verified | Score |
|---|---|---:|---:|
| `gpt-5.2` | OpenAI | 12 / 12 | **100%** |
| `grok-4.3` | xAI | 12 / 12 | **100%** |
| `claude-opus-4-7` | Anthropic | 12 / 12 | **100%** |

Every model produced provably-correct solutions for all 12 easy-tier tasks.
The generated solutions are committed under `eval/candidates/<model>/` so the
result is fully inspectable and reproducible.

## What this shows — and what it does not

**It shows** that current frontier models can pick up an unfamiliar small
language's refinement-contract syntax from a single example signature and
write code that a compiler *proves* correct — not merely code that passes
tests. That is a real capability and the methodology (scoring provable
correctness) works end to end.

**It does not** discriminate between these models — they all scored 100%.
That is an honest limitation of this first benchmark, not a finding about
the models: the 12 tasks are deliberately small integer functions
(`min`, `clamp`, `abs`, …) whose contracts sit well inside what any capable
model can satisfy. A benchmark that *separates* frontier models needs
harder tasks — deeper case analysis, tighter contracts, multi-function
programs, contracts that require the non-linear SMT path.

## Methodology notes

- **`gpt-5.2` and `grok-4.3`** are clean API evaluations: the model saw
  only the task prompt and the stubbed signature, never the reference
  solution. Reproduce with `eval/harness/generate.py`.
- **`claude-opus-4-7`** was produced by the Claude agent that built this
  repository, which had full repository context (including the reference
  solutions). It is therefore **not a clean blind evaluation** — treat that
  column as a demonstration that the tasks are solvable in idiomatic
  Aether, not as a head-to-head data point. A clean Claude run needs an
  `ANTHROPIC_API_KEY` and `generate.py` with `provider=anthropic`.
- `gpt-5.5` was requested but is not offered by the OpenAI API; `gpt-5.2`
  (released 2025-12-11) was the newest model available at run time.

## Next step

Grow the task set toward contracts that genuinely stress a model: tasks
where the obvious implementation violates the contract, where the contract
needs the SMT escalation path, and where the solution spans several
mutually-constrained functions. Only then will the score spread.

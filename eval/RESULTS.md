# Benchmark Results

Runs of the Aether verified-code benchmark (see [README.md](README.md))
against current frontier models. Each model is given only `prompt.md` +
`signature.ae` for each task and must return a complete Aether function;
the result is scored by `aether check` — a task counts as solved only when
the compiler **proves** its refinement contract (zero errors, zero
warnings), not merely when it type-checks.

Run date: 2026-05-21. Harness: `eval/harness/run.py`.

## 57-task run (current)

| Model | Provider | Score | easy | medium | medium-hard | hard |
|---|---|---:|---:|---:|---:|---:|
| `grok-4.3` | xAI | **50 / 57 — 88%** | 12/12 | 4/4 | 14/16 | 20/25 |
| `gpt-5.2` | OpenAI | **46 / 57 — 81%** | 11/12 | 4/4 | 11/16 | 20/25 |

**The expanded benchmark discriminates.** On the original 12 easy tasks
both models scored a flat 100%; across 57 tasks spanning four difficulty
tiers a real gap appears:

- **Grok-4.3 (88%) outscores GPT-5.2 (81%)** by 4 tasks.
- The gap is concentrated in the **medium-hard** tier — ADTs with
  exhaustive pattern matching and structured multi-statement code — where
  Grok-4.3 solved 14/16 to GPT-5.2's 11/16.
- The **hard** tier — multi-branch case analysis, modular arithmetic, and
  "trap" tasks where the naive implementation violates the contract — cost
  *both* models 5 tasks each (20/25). Traps bite frontier models equally.
- GPT-5.2 also missed one **easy** task, which a 100%-on-easy benchmark
  would never have surfaced.

Generated solutions are committed under `eval/candidates/<model>/` so every
result is inspectable and reproducible. Regenerate with
`eval/harness/generate.py <dir> <provider> <model>` and re-score with
`run.py`.

## What this shows

The methodology works: scoring **provable correctness** rather than
test-pass rate produces a benchmark that — once the tasks are hard enough —
separates frontier models, and locates *where* they differ. Both models are
excellent at simple refinement-typed code; they diverge on structured code
and lose ground on adversarial "trap" tasks. That is a more informative
signal than "the hidden tests passed".

## Methodology notes

- **`grok-4.3` and `gpt-5.2`** are clean API evaluations: each model saw
  only the task prompt and the stubbed signature, never the reference
  solution.
- An earlier 12-task run also scored `claude-opus-4-7` at 12/12, but those
  solutions were written by the Claude agent that built this repository
  (full repo context) — **not** a clean blind evaluation, so it is omitted
  here. A clean Claude run needs an `ANTHROPIC_API_KEY`.
- `gpt-5.5` was requested but is not offered by the OpenAI API; `gpt-5.2`
  (released 2025-12-11) was the newest model available at run time.
- A few easy-tier tasks have loose contracts (a constant satisfies them) —
  see [README.md](README.md). The medium and hard tiers are designed so
  only a genuinely correct solution verifies.

## Next step

The hard tier (20/25 for both models) is the discriminating frontier.
Growing it — more trap tasks, contracts requiring the SMT path, multi-
function programs — would sharpen the benchmark further. The score is no
longer pinned at 100%, so it now has room to *measure* progress.

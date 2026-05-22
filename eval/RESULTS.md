# Benchmark Results

Results of the Aether verified-code benchmark. A task counts as solved only
when `aether check` **proves** its refinement contract — zero errors, zero
warnings. The verifier is an exact oracle: no hidden tests, no LLM judge.

## v2 — procedural, contamination-resistant

The v2 benchmark is procedurally generated (`eval/templates.py` +
`gen_tasks.py`): every run instantiates a fresh seeded task set, so no fixed
instance can be memorized. Every contract is a mutation-audited tight
functional spec — a degenerate body (a constant, a bare parameter) is
refuted. Scores carry a Wilson 95% confidence interval; the 112 tasks are a
*sample*, so the headline number has sampling error even though the scorer
is exact.

Run date: 2026-05-21. Task set: 112 tasks, seed 1, 10 families across five
difficulty tiers.

| Model | Provider | Score | 95% CI | easy | medium | medium-hard | hard | expert |
|---|---|---:|---|---:|---:|---:|---:|---:|
| `gpt-5.4` | openai (API) | **111/112 — 99%** | [95%, 100%] | 20/20 | 8/8 | 35/36 | 41/41 | 7/7 |
| `gpt-5.5` | openai (API) | **109/112 — 97%** | [92%, 99%] | 20/20 | 8/8 | 33/36 | 41/41 | 7/7 |
| `gemma4:26b` | ollama (local) | **108/112 — 96%** | [91%, 99%] | 20/20 | 8/8 | 33/36 | 40/41 | 7/7 |
| `qwen2.5-coder:7b` | ollama (local) | **70/112 — 62%** | [53%, 71%] | 14/20 | 0/8 | 24/36 | 32/41 | 0/7 |

### The finding: v2 is saturated

The three capable models cluster at **96–99%** — and their confidence
intervals overlap heavily, so a McNemar paired test finds **no significant
difference** between them. The benchmark, as of v2, no longer separates
strong models: it is saturated.

This is not a flaw in the task *writing* — it is structural. **A
verified-code benchmark cannot be harder than its verifier is expressive.**
The v2 task families are single, straight-line functions over linear integer
arithmetic — an inherently easy proof class. The verified-code benchmarks
that are *not* saturated (miniF2F, Verina's proof track) are hard because
their verifiers demand loop invariants, induction, and termination.

`qwen2.5-coder:7b` (62%) is the one model the v2 set still discriminates —
and informatively: it **collapses on whole tiers** (medium 0/8, expert 0/7)
while still managing easy integer functions. A small model fails structured
verification entirely.

### Agentic mode

The verifier-in-the-loop mode (`agentic.py`) lets a model see `aether check`'s
diagnostics — including refutation counterexamples — and retry, up to 5
turns.

- `gpt-5.4`: pass@1 111 → pass@5 112 (**+1**) — already at the ceiling, no
  room for feedback to help.
- `qwen2.5-coder:7b`: pass@1 73 → pass@5 73 (**+0**) — a 7B model gains
  nothing from the counterexamples; it resubmits variations of the same
  wrong answer.

The agentic gain is real (the verified-code literature shows ~2× for
mid-capability models on hard tasks), but it is invisible here because every
model tested is either at the ceiling or too weak to use the feedback. It
will show once the task set is hard enough to give strong models headroom.

## v2.1 — co-evolution: a more expressive verifier

The saturation finding drove the next phase: deepen the *verifier* so harder
task families become expressible and checkable. Three capabilities were added
to the Aether compiler:

- **Modular contract reasoning** — a call site imports the callee's `where`
  postcondition (and reasons into `match` arms). New `compose` family:
  multi-function tasks. Bonus: this also made recursive verification work —
  a self-call is the inductive hypothesis.
- **`decreases` termination checking** — a new language clause; recursive
  functions must prove they terminate. New `recursive` family.
- **`len` as a solver term** — list-length contracts (`len(result) == len(xs)`)
  are now decidable.

### Re-benchmark — and the honest result

The v2.1 set is 136 tasks across 12 families. `gpt-5.4` scored
**136/136 — 100%**, including `compose` 12/12 and `recursive` 12/12.

The compiler half of the co-evolution loop worked: the new families are
sound, verify, and genuinely exercise modular and recursive verification.
But the **tasks are still not hard** — a frontier model handles two-function
composition and termination-checked recursion trivially. Making the *verifier*
more expressive enabled new task *types*; it did not, by itself, make the
benchmark harder.

The honest conclusion: the next iteration of the loop is **task authoring,
not verifier capability** — deep multi-level compositions, recursion that
needs a non-obvious measure or invariant, contracts near the edge of the
solver. The machinery to express and check such tasks now exists; the tasks
themselves must be written to be hard.

## The spec-writing track

A separate hypothesis: writing a *tight contract* — given a correct body — is
harder than writing the body. `specwrite.py` tests it: the model writes the
`where` clause, which must be **sound** (the reference verifies against it)
and **strong** (it refutes every mutant the canonical contract refutes).

`gpt-5.4` scored **124/124 — 100%** on the v2.1 contract-bearing tasks. Spec
writing did not discriminate the frontier either.

## Bottom line

Across three framings — write the body, write the contract, iterate with
verifier feedback — a frontier model saturates this benchmark. The reason is
structural and worth stating plainly: **the tasks are small functions over
linear integer arithmetic, and a frontier model finds those easy no matter
what is asked.** Question framing is not the difficulty lever; *program
complexity* is.

What the benchmark genuinely delivers today:

- a **rigorous, exact-oracle methodology** — the compiler proves the
  contract; no test flakiness, no LLM judge;
- **contamination resistance** — every run regenerates fresh from templates;
- **mutation-audited tight contracts** — task quality is a measured property,
  not a hope;
- **broad-range discrimination** — it cleanly separates capability tiers: a
  7B local model scores 62% and fails whole tiers where frontier models score
  ~100%.

What it does **not** yet do is separate *frontier* models from one another.
That needs genuinely complex verified programs — many interacting functions,
deep data-structure invariants, proof obligations the auto-prover cannot
discharge unaided — or a verifier expressive enough to demand non-trivial
proof artifacts (loop invariants, lemmas). Both are substantial efforts. The
v2 infrastructure and the three verifier capabilities added this cycle
(modular contract reasoning, `decreases` termination, `len` terms) are the
foundation they would build on — the co-evolution loop is in place; the next
turn of it is hard, complex *task authoring*.

## Methodology notes

- **Local models** (`gemma4:26b`, `qwen2.5-coder:7b`) ran end-to-end through
  Ollama — no API key, no cost. `gemma4:26b` timed out on a few tasks under
  the 240 s request limit; those count as unsolved.
- **`gpt-5.4` / `gpt-5.5`** were called through the OpenAI Responses API at
  `reasoning.effort = medium`. Each model saw only the task prompt and
  stubbed signature.
- Every score is reproducible: the seed is recorded in the task set's
  `_manifest.json`, and generated solutions are committed under
  `eval/instances/<set>/candidates/`.
- Significance: with ~112 tasks, a McNemar paired test detects only gaps of
  roughly 8+ tasks. Smaller gaps — including the 96% vs 99% spread above —
  are within noise.

# A Verified-Code Evaluation with an Exact Compiler Oracle: Method, an Honest Saturation Result, and an RL-Environment Repositioning

## Abstract

Mainstream evaluations of LLM-generated code score a solution by running a
hidden test suite or by asking an LLM judge to grade it. Both are
*probabilistic* oracles: a passing test suite certifies correctness only on
the inputs it happens to exercise, an LLM judge is itself an unreliable model,
and both are contamination-prone and reward-hackable. We describe the Aether
verified-code evaluation, which replaces the probabilistic oracle with an
*exact* one: each task ships an Aether function signature carrying a
refinement-type `where` contract, and the Aether compiler must **prove** that
contract holds for every input before the task counts as solved. We pair this
exact oracle with three further mechanisms — procedural, per-seed task
generation that resists training-corpus contamination; a mutation audit that
certifies every contract is *tight* (no degenerate body satisfies it); and a
verifier-in-the-loop agentic mode — and report results under proper inference
(Wilson confidence intervals, exact McNemar paired tests). The honest finding
is that on small linear-arithmetic functions the benchmark *saturates*:
frontier models score 97–100% whether they write the body, write the contract,
or iterate with verifier feedback, because question framing is not the
difficulty lever — program complexity is. The benchmark still cleanly
discriminates the broad capability range (a 7B model scores 62% and collapses
on whole difficulty tiers). We argue the exact-oracle machinery is most
valuable not as a static leaderboard but as an open-source reinforcement-
learning *environment*: a dense, non-reward-hackable, contamination-free
reward signal for training verified-code models — a thing the RL-environments
ecosystem currently lacks.

## 1. Introduction

A coding benchmark is only as trustworthy as its oracle — the procedure that
decides whether a candidate solution is correct. HumanEval, MBPP, and
SWE-bench score a solution by executing a hidden test suite: the answer they
give is "correct on the inputs we tried," not "correct for all inputs." An LLM
judge replaces the test suite with another language model and inherits that
model's unreliability. Both oracle styles share three structural weaknesses.
They are *probabilistic* — a finite test suite cannot witness correctness on
the unbounded input space, and a judge model is not sound. They are
*contamination-prone* — once a static benchmark is crawled into a training
corpus, a model can reproduce memorized solutions, and the benchmark silently
stops measuring capability. And they are *reward-hackable* — a solution that
special-cases the visible tests, or that flatters a judge, scores well without
being correct.

The Aether project takes a different route. Aether is a small statically typed
language whose compiler proves *refinement-type* contracts at compile time: a
function signature may carry a `where` clause — a postcondition over the
parameters and the result — and `aether check` either proves it for all inputs
or rejects the program. This turns the compiler into an *exact* oracle for a
code-generation task. A task ships a signature with a fixed `where` contract;
a candidate counts as solved if and only if `aether check` reports zero errors
and zero warnings. There are no hidden tests, no judge model, and no flaky-test
noise: a task is proved or it is not.

This document describes the evaluation built on that oracle, reports its
results without varnish, and draws a repositioning conclusion. The benchmark
machinery works and is rigorous; on the current task families it saturates for
frontier models; and its real contribution is as an RL training environment
rather than a discriminating leaderboard.

## 2. Method

### 2.1 The exact oracle

Each task is a directory holding a natural-language `prompt.md`, a
`signature.ae` with a stub body, and a verified `reference.ae`. The signature
carries the contract the solution must satisfy. The scorer (`run.py`) runs
every candidate through `aether check --json` and classifies the result into
one of six outcomes: `VERIFIED` (type-checks, effect-checks, and the contract
is *proved*), `UNPROVEN` (type-clean but a postcondition is refuted or
unprovable), `TYPE-ERROR`, `EFFECT-ERROR`, `PARSE-ERROR`, or `MISSING`. Only
`VERIFIED` counts as solved, and a refinement-checker *warning* ("could not
prove") is not good enough — verification means proved, not "no counterexample
found." The compiler never misjudges, so the per-task verdict carries no
measurement error.

### 2.2 Procedural, contamination-resistant generation

The benchmark is not a fixed set of files. `templates.py` defines parametric
task *families* — `clamp`, `bucket`, `minmax`, `saturating_add`, `mirror`,
`compose`, `recursive`, and others — each a function from a seeded RNG to a
fresh instance: a prompt, a stubbed signature, and a verified reference.
`gen_tasks.py` samples a seed and materializes a whole task set, varying
literals, identifier names (drawn from disjoint pools to perturb surface
form), branch counts, and thresholds. Because every evaluation run can use a
new seed, no fixed instance can be memorized; a static, committed benchmark
becomes un-scoreable once crawled, whereas a procedural generator produces
tasks no corpus has seen. The seed is recorded in `_manifest.json`, so any run
is exactly reproducible. For an RL setting the same mechanism partitions
cleanly: train and evaluate on disjoint seed ranges and the model has provably
never seen the held-out data.

### 2.3 Mutation-audited contracts

A `where` contract is only as good as the wrong programs it rejects. A loose,
"vacuous" specification — one a constant return value already satisfies —
would let a degenerate solution score as solved, and spec vacuity is a known
open problem in the verified-code literature. `mutate.py` turns it into a
measured property. It treats each contract as a test oracle: it generates
mutants of the reference solution — operator swaps, off-by-one literal
perturbations, and the *degenerate gauntlet* (replace the entire body with a
constant `0`/`1`/`2` or a bare parameter) — and checks that `aether check`
*refutes* every one. A surviving degenerate mutant is direct evidence the
contract cannot distinguish a correct solution from nothing. The two
invariants `gen_tasks.py --validate` enforces are exactly this: the reference
must verify, and the stub must *not*. Contracts are therefore written as tight
functional specs — a disjunction of `(branch-condition && result == value)`
clauses, or a relational postcondition — never as a loose one-sided bound. The
audit runs in CI; task quality is a checked fact, not a hope. (Operator and
literal survivors are also reported but may be *equivalent mutants* needing a
human eye; a degenerate survivor never is.)

### 2.4 Three framings of the same task

The harness probes the same tasks from three angles. *Single-shot generation*
(`generate.py`) asks a model to write the body blind. *Spec-writing*
(`specwrite.py`) inverts the task: the model is shown a correct body and must
write the tightest `where` contract — scored, in the Verina discipline, on
*soundness* (the reference verifies against it) and *strength* (it kills every
mutant the canonical contract kills). *Agentic verifier-in-the-loop*
(`agentic.py`) runs the realistic development loop: the model submits a body,
receives the verbatim `aether check` diagnostics including any refutation
counterexample, and retries up to `N` turns; it reports a pass@turn curve
whose pass@1-to-pass@N gap measures the value of verifier feedback. The only
feedback is the verifier's own output — the reference is never shown, turns
are hard-capped, and the final submission is scored by the single-shot rule.

### 2.5 Proper statistics

The scorer is exact, but a generated task set is still a *sample*, so the
headline rate has sampling error. `run.py` reports a Wilson 95% confidence
interval on every score (the Wilson interval is the standard choice near 0%
and 100%, where the Wald interval misbehaves). `compare.py` runs an exact
McNemar paired test for every model pair — the two models face identical
tasks, so a paired test is correct, and at ~100 tasks the exact binomial
p-value is preferred over the chi-square approximation. A raw score gap is
reported as real only when the test says so; at ~112 tasks, gaps under roughly
8–10 tasks are within noise. `compare.py` also reports *item discrimination* —
how many tasks are solved by all models, by none, or split — so a saturated
set is visible directly.

## 3. Results

### 3.1 The benchmark saturates for frontier models

The v2 task set comprised 112 procedurally generated tasks (seed 1, 10
families, five difficulty tiers). The three capable models clustered tightly:
`gpt-5.4` 111/112 (99%, 95% CI [95%, 100%]), `gpt-5.5` 109/112 (97%, [92%,
99%]), and the local `gemma4:26b` 108/112 (96%, [91%, 99%]). The confidence
intervals overlap heavily and the McNemar paired test finds *no significant
difference* among them. The benchmark, as of v2, does not separate strong
models — it is saturated.

This is not a flaw in task *writing*; it is structural. A verified-code
benchmark cannot be harder than its verifier is expressive. The v2 families
are single, straight-line functions over linear integer arithmetic — an
inherently easy proof class. The verified-code benchmarks that are *not*
saturated (miniF2F, the Verina proof track) are hard precisely because their
verifiers demand loop invariants, induction, and termination arguments.

A v2.1 cycle deepened the *verifier* rather than the task writing: the Aether
compiler gained modular contract reasoning (a call site imports the callee's
`where` postcondition — enabling a `compose` family of multi-function tasks,
and, as a bonus, recursive verification, since a self-call is the inductive
hypothesis), `decreases` termination checking (a `recursive` family), and
`len` as a solver term. The v2.1 set of 136 tasks across 12 families was
scored 136/136 (100%) by `gpt-5.4`, including `compose` 12/12 and `recursive`
12/12. The compiler half of the loop worked — the new families are sound and
genuinely exercise modular and recursive verification — but the tasks are
still not hard: a frontier model handles two-function composition and
termination-checked recursion trivially. Making the verifier more expressive
enabled new task *types*; it did not, by itself, make the benchmark harder.

### 3.2 Framing is not the difficulty lever

The saturation holds across all three framings. In agentic mode, `gpt-5.4`
went pass@1 111 → pass@5 112 (+1) — already at the ceiling, leaving no room
for feedback to help. On the spec-writing track, `gpt-5.4` scored 124/124
(100%): writing the tight contract, given a correct body, did not discriminate
the frontier either. The honest conclusion is that question framing — write
the body, write the contract, iterate with feedback — is not the difficulty
lever. *Program complexity* is. The tasks are small functions over linear
integer arithmetic, and a frontier model finds those easy no matter what is
asked.

### 3.3 The benchmark discriminates the broad range

The set is not undiscriminating in general — it is undiscriminating *at the
frontier*. The 7B local model `qwen2.5-coder:7b` scored 70/112 (62%, 95% CI
[53%, 71%]), and informatively so: it collapsed on whole tiers (medium 0/8,
expert 0/7) while still managing easy integer functions. A small model fails
structured verification entirely. In agentic mode it went pass@1 73 → pass@5
73 (+0) — it gains nothing from refutation counterexamples, resubmitting
variations of the same wrong answer. The agentic gain that the verified-code
literature reports (~2× for mid-capability models on hard tasks) is real but
invisible here: every model tested is either at the ceiling or too weak to use
the feedback. It will appear once the task set is hard enough to give strong
models headroom.

## 4. The RL-Environment Contribution

The saturation result reframes what this work contributes. As a static
leaderboard the benchmark cannot, today, separate frontier models. But the
combination it implements — an exact compiler oracle, procedural per-seed
generation, mutation-audited tight contracts, and a multi-turn
verifier-feedback loop — is exactly the specification of a high-quality
reinforcement-learning *environment*, and that ecosystem has a real gap.

Essentially every code RL environment today rewards on a test suite or an LLM
judge. Both can be reward-hacked: a policy can learn to pass the visible tests
or flatter the grader without being correct, and RL is a relentless optimizer
of exactly such loopholes. An exact-oracle reward closes the loophole at the
source — a proof cannot be hacked. The environment (`aether_verified_code`,
packaged for the `verifiers` framework) therefore offers four properties RL
training genuinely needs:

- **Non-hackable reward.** The reward is the compiler proving a refinement
  contract for all inputs. There is no test suite to overfit and no judge to
  flatter.
- **Contamination-free by construction.** Tasks come from
  `aether_core.new_task(seed)`; train and evaluate on disjoint seed ranges and
  the policy has provably never seen the evaluation data.
- **Dense, graded reward.** Rather than a sparse 0/1, the environment exposes
  a reward ladder: `PARSE-ERROR` 0.00, `TYPE-ERROR` 0.04, `EFFECT-ERROR` 0.08,
  `UNPROVEN` 0.15 + 0.45 × (fraction of contract conjuncts proved, capped
  0.60), and `VERIFIED` 1.00. Because every conjunct is mutation-audited, the
  partial credit is *real* partial correctness, not spec-gaming. The reward is
  the high-water mark across the rollout's turns — monotone, so it cannot be
  farmed by oscillating the diagnostics.
- **Multi-turn verifier feedback.** Each rollout is the agentic loop: the
  policy sees `aether check`'s diagnostics, including refutation
  counterexamples, and retries — a dense within-episode learning signal.

In this framing the v2/v2.1 saturation is not a defect but a *property*: a
clean, easy proof class is a sensible curriculum floor for RL, and the
infrastructure scales to harder families as the verifier's expressiveness
grows. The contribution is the environment — a verifiable, non-hackable,
contamination-free reward signal for training verified-code models — more than
the leaderboard.

## 5. Related Work

This work sits in the *verified-code evaluation* family — evaluations that
score machine-checkable correctness rather than test-pass rate. **DafnyBench**
has LLMs supply the annotations that let the Dafny verifier discharge a
program (~750 programs). **Verina** scores joint code + specification + proof
generation in Lean 4 across 189 tasks, per component; its specifications are
deliberately crafted to reject mutated negative tests — the same discipline
`mutate.py` enforces and `specwrite.py` adopts. **CloverBench** pairs code,
specification, and proof and checks their mutual consistency. **AlphaVerus**
bootstraps verified code generation through a generate–verify–refine loop
against a verifier. **VerifyThisBench** evaluates the end-to-end pipeline from
a natural-language problem through a formal specification to a verified
implementation across several verifiers. Aether's distinguishing angle is the
*compiler as a built-in oracle*: each task ships a fixed `where` contract, and
the same `aether check` that compiles the language decides — exactly — whether
that contract is proved, with no separate proof artifact to grade. On the RL
side, the work targets the `verifiers` environment ecosystem, where the
prevailing reward signals are test suites and LLM judges; an exact-oracle
environment is a deliberate alternative to both.

## 6. Limitations

We state the limitations plainly.

- **A bespoke language.** Aether is a small purpose-built language; transfer
  of any capability or training signal to mainstream languages is *unproven*.
  The evaluation measures provable-correctness ability in Aether, not in
  Python or Rust.
- **A bounded verifier.** The Aether verifier currently proves linear integer
  arithmetic, recursion with `decreases` termination, and modular reasoning
  across call sites; it escalates genuinely non-linear goals to `z3` when
  installed. It does *not* yet reason about heap state or quantifiers.
  Data-structure invariants — sortedness, quantified list properties — are the
  next frontier and are not yet covered. A verified-code benchmark cannot be
  harder than its verifier is expressive, so the difficulty ceiling is the
  verifier's.
- **Curriculum depth.** The task families are shallow: single straight-line
  functions, two- and three-stage compositions, simple recursion. The honest
  next step is *task authoring* — deep multi-level compositions, recursion
  needing a non-obvious measure or invariant, contracts near the solver's
  edge. The machinery to express and check such tasks exists; the hard,
  unglamorous work of writing them does not.
- **A research artifact.** The value demonstrated here is the *method* — that
  an AI coding evaluation can score provable correctness rather than test-pass
  rate, regenerate itself to resist contamination, and prove its own
  specifications are strong — not a definitive frontier-model ranking.

## 7. Conclusion

We set out to replace the probabilistic oracle of mainstream coding benchmarks
with an exact one, and the exact oracle works: the Aether compiler proves a
refinement contract, procedural generation resists contamination, a mutation
audit certifies every contract is tight, and Wilson intervals and McNemar
tests keep the inference honest. The equally honest finding is that on small
linear-arithmetic functions the benchmark saturates for frontier models —
97–100% across writing the body, writing the contract, and iterating with
verifier feedback — because the difficulty lever is program complexity, not
question framing. The benchmark still cleanly separates the broad capability
range, with a 7B model at 62% collapsing on whole tiers. The constructive
conclusion is a repositioning: the exact oracle, procedural generation, dense
graded reward, and verifier-in-the-loop combination is most valuable as an
open-source RL environment — a non-reward-hackable, contamination-free, dense
reward signal for *training* verified-code models, which is precisely what the
RL-environments ecosystem lacks. The leaderboard saturated; the training
environment is the contribution.

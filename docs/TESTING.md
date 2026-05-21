# Aether Testing Strategy

This document surveys what strong open-source language-tooling projects do to test
themselves, honestly inventories where Aether stands today, identifies the gaps, and
lays out a proportionate, prioritised plan for closing them.

---

## 1. Where Aether Is Today

### Test count and surface area

As of the current `main` branch, `cargo test --workspace` runs **417 tests** across
nine crates.  The breakdown by kind:

| Kind | Location | What it covers |
|------|----------|----------------|
| **Unit tests** | inline `#[cfg(test)]` modules in every crate | individual functions in the lexer, parser, type-checker, refinement solver, bytecode compiler, VM, stdlib, WASM binding |
| **End-to-end CLI tests** | `crates/aether-cli/tests/` (7 files) | `aether check/run/snap/verify/bc`, imports, stdlib; all spawn the real binary via `CARGO_BIN_EXE_aether` |
| **Snapshot tests (native)** | `crates/aether-cli/tests/snap_e2e.rs` | the `aether snap` command's own capture/verify/update lifecycle — tests Aether's _built-in_ snapshot feature, not an external framework |
| **Differential / metamorphic** | `crates/aether-difftest/` + `tests/examples.rs` | tree-walker vs bytecode VM: runs every `examples/*.ae` on both runtimes and asserts they agree; tolerance currently 2 divergences |
| **Property-based fuzz** | `crates/aether-parser/tests/fuzz_robustness.rs` | `proptest` against lexer + parser + type-checker; three strategies: pure-random UTF-8, random ASCII, structured token-fragment noise (512 cases each); 10 hand-written regression inputs; one known panic (`#[ignore]`d) |
| **Refinement soundness** | `crates/aether-types/src/refine.rs` (proptest section) | `proptest` verifies that the Fourier-Motzkin solver never claims `Proved` and also emits a witness that refutes the implication |
| **SMT integration** | separate `smt` CI job | exercises the Z3-gated path when `z3` is on PATH; runs `cargo test -p aether-types smt::tests` |
| **Criterion microbenchmark** | `crates/aether-bc/benches/fib_bench.rs` | `fib(20)` via bytecode VM (`bc_fib20`) and tree-walker (`eval_fib20`), compared with Criterion |
| **In-language test blocks** | `examples/09_test_blocks.ae`; `just ae-test` | tests written _in_ Aether using `test { … }` blocks, run through the tree-walker |
| **Eval benchmark harness** | `eval/` (Python harness + 12 tasks) | LLM-generated `.ae` solutions graded against reference programs; measures type-check pass rate, run correctness, and annotation density |

### CI jobs

Five jobs in `.github/workflows/ci.yml`:

| Job | Matrix | What it runs |
|-----|--------|--------------|
| `test` | stable + beta × ubuntu + macos (4 combinations) | `cargo build`, `cargo test --workspace`, `cargo clippy` |
| `fmt` | ubuntu | `cargo fmt --all -- --check` |
| `wasm` | ubuntu | `cargo build -p aether-wasm --target wasm32-unknown-unknown` |
| `vscode` | ubuntu | `npx tsc --noEmit` on the VS Code extension |
| `smt` | ubuntu (z3 installed) | `cargo test -p aether-types smt::tests` |

No coverage measurement, no cargo-fuzz, no regression benchmark tracking in CI.

### What is not tested

- **Parser error messages** — the text and span of diagnostics is not asserted anywhere.
- **Type-checker diagnostics** — error message text is not regression-locked.
- **Formatter / pretty-printer** roundtrip — `crates/aether-parser/src/pretty.rs` has no dedicated tests.
- **LSP responses** — `crates/aether-lsp/` has no test harness.
- **WASM smoke** — `crates/aether-wasm/tests/wasm_smoke.rs` exists but no browser/wasm-bindgen-test runner is in CI.
- **Coverage** — not measured anywhere.
- **Benchmark regression** — `fib_bench` is not run in CI; there is no baseline comparison.

---

## 2. What Strong Projects Do

Research pulled directly from the repositories below in May 2026.  "Verified" means I
read the source or documentation; "inferred" means I extrapolated from directory
structure or partial docs.

### Ruff (astral-sh/ruff) — Python linter in Rust

**Snapshot / fixture testing.**  Ruff's primary test surface is the `ruff_mdtest`
framework: rule tests live as Markdown files under
`crates/ruff_linter/resources/mdtest/` (one `.md` per rule).  Each file embeds Python
code blocks annotated with `# error` (assert a diagnostic fires on this line) or
`# snapshot` (capture a full inline snapshot via `insta`).  `cargo insta review`
updates them interactively; `MDTEST_UPDATE_SNAPSHOTS=1` regenerates in bulk.  This
approach lets rule authors write tests in the language being linted, not in Rust
boilerplate.

**Benchmarking.**  A dedicated `ruff_benchmark` crate uses Criterion.  Workflow:
`cargo bench -p ruff_benchmark -- --save-baseline=main`, then
`cargo bench -p ruff_benchmark -- --baseline=main` on a branch; `critcmp` for
side-by-side output.  Benchmarks cover both the linter and formatter on real-world
corpus files.  Not run in standard CI (verified: separate benchmark CI or manual).

**Ecosystem CI.**  A separate workflow runs the formatter/linter against real-world
projects (verified: mentioned in CONTRIBUTING.md as "ecosystem CI").

### OXC (oxc-project/oxc) — JS toolchain in Rust

**Conformance testing.**  OXC runs three external conformance suites as git
submodules, invoked via `cargo coverage js` (test262), `cargo coverage babel`, and
`cargo coverage ts` (TypeScript).  The `tasks/coverage/` crate contains a harness
that drives the parser/transformer over every test case and records pass/fail.
Snapshots of those results are committed; `UPDATE_SNAPSHOT=1 just c` regenerates
them.  This catches regressions against the full JS spec corpus without writing
per-feature unit tests.

**Snapshot files.**  `tasks/coverage/snapshots/` stores per-suite pass-rate snapshots
committed to git; any drop in conformance shows up as a diff in PR.

**Fuzzing.**  OXC uses `cargo-fuzz` (inferred from repo structure; not verified in
detail).

### Roc (roc-lang/roc)

**Compiler IR snapshot testing.**  The `crates/compiler/test_mono/` crate uses a
`#[mono_test]` proc-macro: each test is a raw Roc string; the macro compiles it to
mono IR and writes the result to `src/generated/<test_name>.txt`.  Tests compare
against these committed text files — `git diff` detects changes.  Hundreds of
generated snapshots cover the full space of language features at the IR level.  This
is the most thorough use of IR-level snapshot testing I found in this survey.

**Language-level test suite.**  Roc tests builtins with `.roc` programs in
`crates/compiler/builtins/roc/` that are compiled and run as part of the test suite.
This is effectively a language-native conformance corpus.

### Gleam (gleam-lang/gleam)

**Snapshot testing.**  `compiler-core/src/snapshots/` contains committed `.snap`
files in the pattern `gleam_core__<module>__<test_name>.snap`, covering config
serialization, dependency resolution, error messages, and diagnostic output.  Tests
use the `insta` crate.  The directory has at least dozens of snapshots (the listing
was truncated at 13 visible).

**Integration tests.**  `make language-test` runs language-level integration tests
requiring Erlang, Elixir, Node, Deno, and Bun — each target backend is exercised with
programs compiled to that target and actually executed.  This is a multi-runtime
conformance check (verified from CONTRIBUTING.md).

**Test structure.**  Tests are co-located with modules (`analyse/`, `type_/`,
`erlang/`, `javascript/`) — each subdirectory likely has its own `tests.rs` in
addition to the `snapshots/` directory.

### rust-analyzer

**Fixture-based inline tests.**  The `crates/test-utils/` crate provides a rich
fixture DSL: `$0` marks cursor positions, `//^^^` annotations point at code ranges,
`<tag></tag>` marks regions with attributes.  Tests are written as Rust string
literals that embed cursor/annotation markup; assertions use `assert_eq_text!` with
diff output.  No separate test files — tests live in the source they test, driven by
the fixture infrastructure.  This avoids snapshot file sprawl while keeping tests
readable.

**Coverage.**  rust-analyzer has not historically gated on coverage metrics (inferred
from absence of coverage CI job).

### Deno (denoland/deno)

**Spec test suite.**  `tests/specs/<category>/<test_name>/__test__.json` — over 50
category folders (run, test, fmt, lint, compile, node, npm, …).  Each `__test__.json`
declares args, expected output file, exit code, and optional platform conditions.
Output files support `[WILDCARD]` and `[UNORDERED_START]` tokens for non-deterministic
output.  Multi-step tests use a `steps` array or `tests` object.  This is a full
language-native conformance/regression suite without depending on any external
framework.

**test262.**  Deno runs a subset of test262 via a separate conformance job (inferred;
Deno's JS engine is V8 so full test262 is V8's problem, but they track WPT for Web
API conformance).

### Nickel (nickel-lang/nickel)

**Language test corpus.**  `core/tests/` contains three subdirectories: `integration`
(general language features), `examples` (tests derived from example programs), and
`manual` (tests derived from the language manual).  Each test is a `.ncl` file
annotated with a TOML header describing expected behaviour.  Multiple tests are
compiled into larger crates to save compile time and exploit parallelism.  Snapshot
tests for error messages live in `nickel-lang-cli`.  This is the closest analogue to
what Aether needs.

### Biome (biomejs/biome)

**Snapshot testing.**  "In some crates, we use snapshot testing. The majority of
snapshot testing is done using `insta`." (verified from CONTRIBUTING.md).  Doc tests
are also exercised via `just test-doc`.

---

## 3. The Gap

Ranked by impact:

| Gap | Severity | Notes |
|-----|----------|-------|
| **No diagnostic snapshot tests** | High | Parser error messages, type-checker diagnostics, and refinement warnings have no regression lock. Any text change is invisible. Every project in this survey (Gleam, Biome, Ruff, rust-analyzer) snapshot-locks diagnostics. |
| **No `.ae` conformance corpus** | High | The 33 `examples/*.ae` files are run for success/failure but not organised as a corpus with expected outcomes (check errors, run output, refinement results). Nickel and Deno both have this. |
| **No benchmark CI tracking** | Medium | `fib_bench` exists but is not run in CI and has no baseline comparison. Performance regressions are undetectable. Ruff and Nickel both track benchmark regressions. |
| **No coverage measurement** | Medium | There is no `llvm-cov` / `tarpaulin` job. The test suite is reasonable in breadth but there is no data on which code paths are actually hit. |
| **No cargo-fuzz targets** | Medium | `proptest` robustness tests exist but they run only 512 cases and target no specific corpus. `cargo-fuzz` with persistent corpora and `libFuzzer` coverage guidance would find deeper bugs (e.g., the known stack-overflow under deep nesting, `aether-bc` crash paths). |
| **No formatter/pretty-printer tests** | Low-Medium | `pretty.rs` (17 KB) has no tests. Idempotency (`format(format(src)) == format(src)`) is trivial to property-test. |
| **No LSP test harness** | Low | `aether-lsp/` (14 KB of logic) has no tests. Acceptable for a research project but a gap if LSP is a first-class feature. |

---

## 4. Recommended Testing Strategy

Proportionate recommendations for a research-scale project.  Ordered by
value-per-effort.

### P1 — Diagnostic snapshot testing with `insta`

**What.** Add `insta` as a dev-dependency in `aether-types` and `aether-parser`.  For
every existing diagnostic (parse errors, type errors, refinement warnings), add a
`#[test]` that runs the checker and calls `insta::assert_snapshot!(diag_text)`.
Commit the generated `.snap` files.  Update with `cargo insta review`.

**Why now.** Any change to error message text, span formatting, or diagnostic
categorisation is currently invisible.  This is the highest-leverage, lowest-effort
improvement.

**Inspired by.** Gleam's `compiler-core/src/snapshots/`; Biome's insta usage;
Ruff's inline snapshots via mdtest.

**Scope.** Start with ~20 representative diagnostic cases (5 parse errors, 5 type
errors, 5 refinement verdicts, 5 verify output shapes).  Expand as bugs are found.

### P2 — A `.ae` conformance corpus under `tests/corpus/`

**What.** Create `tests/corpus/` with subdirectories for language features.  Each
test is a pair: `foo.ae` (source) + `foo.expected` (expected outcome in a simple
format).  A Rust integration test walks the directory, runs `aether check` / `aether
run`, and diffs against expected.

**Why now.** The 417 existing tests are mostly unit-level or exercise example files
that are designed to succeed.  There is no corpus of programs that are _expected to
fail_ with specific errors, or that produce specific output.  This is the standard
practice in every mature language project (Nickel's `core/tests/`, Deno's
`tests/specs/`, Roc's `test_mono/`).

**See "Conformance corpus proposal" (Section 5)** for the full sketch.

### P3 — Benchmark CI with baseline comparison

**What.** Add a `bench` CI job that:
1. Runs `cargo bench -p aether-bc --no-run` (compile check only) on every PR.
2. On pushes to `main`, saves a Criterion baseline JSON artifact.
3. Optionally on PRs touching `aether-bc` or `aether-eval`, runs
   `cargo bench -p aether-bc -- --baseline=main` and posts a comment.

Alternatively, a simpler approach: commit a `bench/baseline.txt` with the last
known timings; a `just bench-compare` target runs the bench and diffs.

**Why now.** `fib_bench` exists but is never run in CI.  The bytecode VM and
tree-walker have non-trivial performance profiles (the eval harness already tracks
LLM-generated solution quality; a performance regression in the VM would affect
that).

**Inspired by.** Ruff's `--save-baseline` / `--baseline` workflow; Nickel's documented
benchmark CI.

### P4 — cargo-fuzz targets for parser and VM

**What.** Add `fuzz/` at the workspace root with two initial targets:
- `fuzz_parse`: feeds arbitrary bytes to `parse_module`; crash = bug.
- `fuzz_bc`: feeds arbitrary bytes to parse + compile + `run_main`; crash = bug.

Seed corpus: the existing `examples/*.ae` files.

Run periodically (not on every PR — it is slow).  A `just fuzz-parser SECS=60`
target is sufficient for a research project.  Collect crashes to `fuzz/artifacts/`
and promote reproductions to `crates/aether-parser/tests/fuzz_robustness.rs`
regression cases.

**Why now.** The known stack-overflow under 10k-deep nesting (`regression_10000_deep_nesting_stack_overflow`, currently `#[ignore]`d) is exactly the kind of bug that cargo-fuzz + persistent corpora would have found earlier.  proptest with 512 cases is not a substitute.

**Inspired by.** OXC (cargo-fuzz, inferred); the `proptest_regressions/refine.txt`
file in `aether-types` already shows the pattern of promoting minimised failures.

### P5 — Coverage measurement (informational, not gated)

**What.** Add a `coverage` CI job using `cargo llvm-cov`:
```yaml
- run: cargo llvm-cov --workspace --lcov --output-path lcov.info
- uses: codecov/codecov-action@v4
  with: { files: lcov.info }
```

Gate on nothing — just publish the badge and let it inform P1/P2 work.

**Why not gate.** Aether is a research project.  Gating on coverage numbers creates
pressure to write low-value tests.  The value here is map: which code paths in
`refine.rs`, `check.rs`, and `vm.rs` are exercised.

### P6 — Pretty-printer idempotency property test

**What.** One `proptest` suite in `crates/aether-parser`:
```rust
proptest! {
    fn pretty_idempotent(src in structured_noise()) {
        if let Ok(m) = parse_module(FileId(0), &src) {
            let printed = pretty_print(&m);
            if let Ok(m2) = parse_module(FileId(0), &printed) {
                assert_eq!(pretty_print(&m2), printed);
            }
        }
    }
}
```
No new dependencies.  Finds bugs in `pretty.rs` that string-comparison tests miss.

---

## 5. Conformance Corpus Proposal

### Goal

A versioned, growing collection of `.ae` programs with declared expected outcomes —
the language's own test262, at research scale.

### Directory layout

```
tests/corpus/
├── README.md          # format spec
├── parse/             # programs that should parse cleanly or produce specific errors
│   ├── ok_minimal_fn.ae
│   ├── ok_minimal_fn.expected
│   ├── err_unterminated_string.ae
│   └── err_unterminated_string.expected
├── types/             # type-check outcomes
│   ├── ok_id_fn.ae
│   ├── ok_id_fn.expected
│   ├── err_wrong_return_type.ae
│   └── err_wrong_return_type.expected
├── refinements/       # refinement verdict outcomes
│   ├── proved_clamp.ae
│   ├── proved_clamp.expected
│   ├── unknown_nonlinear.ae
│   └── unknown_nonlinear.expected
├── runtime/           # programs expected to run and produce specific stdout
│   ├── fib10.ae
│   └── fib10.expected
└── errors/            # programs expected to exit non-zero with specific message fragments
    ├── div_by_zero.ae
    └── div_by_zero.expected
```

### `.expected` format

A plain-text format (not TOML, not JSON — just text, easy to diff):

```
exit: 0
stdout_contains: verified
stderr_contains:
diagnostics: 0 errors, 0 warnings
```

Or for a type error case:

```
exit: 1
stderr_contains: type mismatch
stderr_contains: expected Int
```

Multiple `*_contains` lines are ANDed.  This is essentially what Deno's
`__test__.json` + expected-output files do, simplified.

### Rust harness

A single test in `crates/aether-cli/tests/corpus.rs`:
```rust
#[test]
fn corpus() {
    for entry in walk("tests/corpus/**/*.ae") {
        let expected = parse_expected(entry.with_extension("expected"));
        let out = aether().arg(expected.verb).arg(&entry).output().unwrap();
        check_against(expected, out, entry);
    }
}
```

### Seeding the corpus

Phase 1 (immediate): promote the 12 `eval/tasks/*/reference.ae` files — they are
already known-good programs with expected type-check + run outcomes.  These become
`tests/corpus/runtime/`.

Phase 2: for every diagnostic already tested in the proptest regression cases
(`fuzz_robustness.rs`), extract the hand-written inputs as `tests/corpus/parse/`
entries with `exit: 1` expected.

Phase 3: as language features are added or bugs are fixed, contributors add a corpus
entry before closing the issue (analogous to how Roc contributors add a `test_mono`
snapshot and how OXC contributors add a test262-filtered regression).

### Maintenance norm

- PRs that fix a type-checker or parser bug must include a corpus entry.
- PRs that add a language feature must include at least one `ok_` and one `err_` corpus entry.
- The corpus is owned by contributors, not generated — keep it small and curated, not exhaustive.

### Growth targets (rough)

| Phase | Corpus size | When |
|-------|------------|------|
| Seed | 20–30 cases | After P2 is implemented |
| Stable | 100–150 cases | ~6 months of normal development |
| Mature | 300+ cases | Long term |

At 300 cases this is still a fraction of Nickel's corpus or Deno's specs, appropriate
for Aether's scale.

---

## Appendix: Tool quick-reference

| Tool | Purpose | How to add |
|------|---------|------------|
| `insta` | Snapshot assertions | `cargo add --dev insta` in target crate |
| `cargo-insta` | CLI for reviewing snapshots | `cargo install cargo-insta` |
| `cargo-fuzz` | Coverage-guided fuzzing with libFuzzer | `cargo install cargo-fuzz` |
| `cargo-llvm-cov` | Line coverage via LLVM | `cargo install cargo-llvm-cov` |
| `critcmp` | Compare Criterion baselines | `cargo install critcmp` |
| `proptest` | Already in use | — |
| `criterion` | Already in use | — |

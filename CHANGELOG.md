# Aether Changelog

All notable changes since the language was bootstrapped in this repo.
Dates are calendar dates in UTC.

## Unreleased

### Added — Language
- **Parametric generics**: `fn id<A>(x: A) -> A` — type parameters declared
  with `<...>` and inferred at each call site (substitution + consistency
  checking). A single-letter type parameter shadows builtin abbreviations,
  so `<B>` is a generic, not `Bool`.
- **Tail-call optimization** in the tree-walker — self/mutual tail recursion
  runs in constant stack (200k-deep loops no longer overflow).
- **`let` bindings in `if`/`else`/`match` arms** without explicit braces or
  `in`.
- **`test`/`bench`/`snap` blocks** may now perform any effect (previously
  limited to `Throw`), so tests can call `print`, `random_int`, `fs_*`, etc.

### Added — Verification
- **Optional SMT escalation**: refinement goals outside the linear fragment
  (e.g. `x * y >= 0`) are translated to SMT-LIB2 and discharged by a `z3`
  binary when one is on `PATH`. No build dependency; a no-op when absent.
- Refinement solver: equality-propagation pre-pass, ground witness
  verification, `mod`/`div`-by-literal and constant-folding fragments.

### Added — Stdlib
- 8 new modules: `strlist`, `log`, `term`, `yaml`, `fs`, `cache`, `retry`,
  `http_server` (29 modules total). Real JSON parsing in `std::json`;
  `regex_replace_all`; `fmt4`/`fmt5`/`fmt_list`.

### Added — Tooling & infrastructure
- `aether build` (manifest-driven) and `aether docgen` (static HTML docs).
- `aether verify` — a strict verification gate: exits 0 only if every
  refinement contract is proved and every effect sound (`--json` for
  pipelines). Stricter than `aether check`, which warns rather than fails.
- "did you mean" identifier suggestions in type-checker diagnostics.
- Differential-test harness (`aether-difftest`) locking the two runtimes.
- GitHub Pages deploy of the browser playground; release workflow for
  tagged binaries.

### Added — Evaluation & testing
- **Verified-code benchmark** (`eval/`): 12 tasks, each a function with a
  `where` contract; a solution scores only when the compiler *proves* the
  contract. Multi-provider generator (OpenAI / xAI / Anthropic). First run:
  `gpt-5.2`, `grok-4.3`, `claude-opus-4-7` each 12/12 (see `eval/RESULTS.md`).
- Fuzz / robustness tests (proptest never-panic + parser round-trip),
  `insta` diagnostic snapshot tests, `criterion` performance benchmarks,
  and a `coverage` CI job (`cargo-llvm-cov`).

### Added — Documentation
- `docs/INTERNALS.md` (compiler walkthrough), `docs/TESTING.md` (testing
  strategy), `docs/PERFORMANCE.md` (measured benchmarks), `docs/SHOWCASE.md`.
- `experiments/py-refinements/` — a proof-of-concept porting refinement
  contracts to Python.

## 0.3.0 — production-track MVP (2026-05-20)

### Added — Language
- **Algebraic data types** with named constructors: `type Shape = Circle(Float) | Square(Float)`. Constructor patterns in `match`; best-effort exhaustiveness warning.
- **First-class closures** captured by value (`Value::Closure`).
- **String interpolation**: `"hello ${name}, you are ${str(age)}"`. Escape literal `${` with a backslash.
- **`bench "..." { ... }`** and **`snap "..." { ... }`** blocks (alongside `test "..." { ... }`).

### Added — Stdlib (21 modules)
`std::{plan, iter, mem, proof, json, list, string, map, path, time, env, fmt, result, regex, sys, math, base64, hash, uuid, random, date}`.

### Added — Runtimes
- **Bytecode VM** (`aether-bc`) — substantially faster than the tree-walker
  (later measured at ~23× on `fib(20)`; see `docs/PERFORMANCE.md`).
- **AOT compile**: `aether compile <file> -o out.aebc` (magic `AEBC\0\0\0\x01`). `aether exec` runs precompiled.
- **`aether run --bc`** with silent tree-walker fallback on unsupported features.

### Added — Tooling
- LSP server, VS Code extension, browser playground (`aether-wasm`).
- `aether init --template bin|lib|agent`, `aether watch [--run]`, `aether lint`, `aether snap`.
- `aether check --json`, `aether ast --json`, `aether fmt --check`.

### Added — Network + persistence
- **`--network` flag** turns `http_get`/`llm_complete` into real reqwest+Anthropic calls.
- **Persistent `std::mem`** JSON store at `~/.aether/mem.json` by default.
- **`ToolRegistry`** swap point for any host application.

### Added — Static verification
- Solver: `forall_in` bounded quantifier, `mod`/`div` by constant, `RefutedWith { values }` witness, proptest soundness fuzz (256 cases).

### Added — Project polish
- GitHub Actions CI (test matrix on stable/beta × Linux/macOS + fmt + wasm + vscode).
- `deny.toml`, `rustfmt.toml`, `SECURITY.md`, `CONTRIBUTING.md`, `Justfile`.
- Per-crate Cargo.toml metadata, six new spec docs.

### Test count: 133 → ~270 across 12 crates.

---

## 0.2.0 — language is usable for real (2026-05-20)

### Added
- **Multi-file modules + imports**: real DFS resolver with cycle detection.
  `import std::iter` actually loads the stdlib `.ae` source.
- **Bytecode VM** (`aether-bc` crate): stack-based VM with **24× speedup** vs
  the tree-walker on `fib(20)` (1.5 ms vs 37 ms).
- **VS Code extension** (`vscode-aether/`): TextMate grammar, 10 snippets,
  LSP client, `vsce`-ready package.
- **JSON-serializable AST** under the `serde` feature. New `aether ast --json
  --pretty` subcommand. Schema documented in `spec/AST_JSON_SCHEMA.md`.
- **In-language `test "..." { ... }`** blocks. New `aether test <file>`
  subcommand discovers and runs them, reporting pass/fail per assertion.
- **Tool registry**: `Runtime::tools.register(name, fn_pointer)` lets a host
  override `llm_complete`, `http_get`, etc. at runtime.
- **Content-hash cache** for `summarize`.

### Test count: 98 → 133.

---

## 0.1.0 — bootstrap (2026-05-20)

### Added
- Lexer (logos), Pratt parser, AST, two surface syntaxes (compact + verbose).
- HM type inference + row-polymorphic effect tracking.
- Refinement types with hand-rolled Fourier-Motzkin solver over linear
  rational arithmetic. Path-sensitive postcondition proving.
- Tree-walking interpreter with automatic provenance on every value.
- Agent primitives: `introspect`, `summarize`, `provenance`, `confident`,
  `assume`, `spec`, `tool`.
- Closures (`fn(x) => …`), pattern matching with guards.
- LSP server (`aether-lsp`).
- Standard library skeleton (`std::plan`, `std::iter`, `std::mem`, `std::proof`, `std::json`).
- CLI: `aether check / run / fmt / explain / ast / repl / watch / doc / init`.
- 6 example programs.
- Spec docs: `LANGUAGE.md`, `RATIONALE.md`, `grammar.ebnf`.

### Test count: 0 → 98.

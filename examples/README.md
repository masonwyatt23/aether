# Aether Examples

31 programs that cover every major language feature, from "Hello, World" to real-world agent pipelines. Run any example with:

```bash
aether run  examples/<file>.ae    # execute main
aether test examples/<file>.ae    # run test blocks
aether bench examples/<file>.ae   # run bench blocks
aether snap examples/<file>.ae    # run snapshot blocks
aether check examples/<file>.ae   # type + effect + refinement check only
```

---

## Language Basics

| File | Demonstrates |
|------|-------------|
| `01_hello.ae` | Function declaration, effect annotation (`{IO}`), builtin `print` |
| `02_refinement.ae` | Refinement postcondition (`result <= a && result <= b`) proved at compile time |
| `03_effects.ae` | Effect tracking — `{Net, Throw}` propagates transitively from `http_get` |
| `07_closures_and_match.ae` | First-class lambdas with captured environment; `match` with guards |
| `08_imports.ae` | `import std::iter`; `clamp` from the standard library |
| `09_test_blocks.ae` | In-language `test "..." { ... }` blocks; `assert_eq`, `assert` |
| `10_adts.ae` | Algebraic data types (`type Shape = Circle(Float) \| Square(Float) \| ...`); pattern matching |
| `19_interpolation.ae` | String interpolation (`"hello ${name}, age ${str(age)}"`) |
| `28_branch_lets.ae` | Implicit-block `let` bindings inside `if`/`else` arms and `match` arms |
| `29_effectful_tests.ae` | `test` blocks performing real effects — `IO` (`print`), `Rand` (`random_int`), `Throw` |
| `30_generics.ae` | Parametric generics — `fn id<A>(x: A) -> A`; type parameters inferred per call site |
| `31_verify_gate.ae` | A file that passes `aether verify` — every refinement contract proved, every effect sound |

---

## Refinement and Verification

| File | Demonstrates |
|------|-------------|
| `12_refinement_proven.ae` | Four non-trivial postconditions fully discharged by the linear-arithmetic solver at compile time |
| `02_refinement.ae` | Minimal refinement: `min` with `result <= a && result <= b` |
| `16_result_handling.ae` | `Result = Ok(Str) \| Err(Str)` error handling; `result_unwrap_or`; tests |
| `20_full_showcase.ae` | Refinement on priority range (`0..=10`), `ranked` postcondition, ADTs, string interpolation, imports, and tests all in one file |

---

## Agent Primitives

| File | Demonstrates |
|------|-------------|
| `04_introspect.ae` | `introspect("current")` — returns the typed module surface for agent self-inspection |
| `05_provenance.ae` | `provenance(z)` — first-class provenance DAG; `print_prov` |
| `06_agent_loop.ae` | `iter_refine(seed, step, budget)` — iterative refinement primitive; `confident(v, 0.85)` confidence tagging |
| `11_provenance_query.ae` | Provenance across a function call chain; reading `Call("process")` nodes |
| `13_agent_workflow.ae` | Full agent script: refinement-verified helper, `mem_get`/`mem_set`, `llm_complete` (stub), provenance |

---

## Standard Library Showcases

| File | Demonstrates |
|------|-------------|
| `14_benchmarks.ae` | In-language `bench "..." { ... }` blocks; `aether bench` runner |
| `15_stdlib_showcase.ae` | `std::list`, `std::string`, `std::fmt`, `std::path`, `std::env` |
| `17_snapshots.ae` | In-language `snap "..." { ... }` blocks and golden `.snap` files |
| `18_bytecode.ae` | Bytecode VM (`aether run --bc`); ahead-of-time compile (`aether compile` + `aether exec`) |
| `21_new_stdlib.ae` | `std::base64`, `std::hash` (SHA-256), `std::uuid`, `std::random`, `std::date` |
| `22_log_term_json_yaml.ae` | `std::log`, `std::term` (ANSI colours), `std::json`, `std::yaml` |

---

## Real-World Programs

| File | Demonstrates |
|------|-------------|
| `23_log_analyzer.ae` | Read and tally a log file by severity; coloured summary report |
| `24_markdown_to_text.ae` | Markdown-to-plain-text pipeline; regex-style string helpers; 6 tests |
| `25_cli_arg_parser.ae` | Token-stream CLI argument parser; flag, key=value, and positional args; 5 tests |
| `26_webhook_receiver.ae` | Webhook event processor; JSON extraction; JSONL event log; 3 tests |
| `27_static_site.ae` | Markdown-to-HTML static site builder; file I/O; title extraction; 5 tests |

---

## Notes

- Files with no `main` (pure `test`/`bench`/`snap`) do not produce output with `aether run`.
- `17_snapshots.snap` is the golden file for `17_snapshots.ae`; update it with `aether snap examples/17_snapshots.ae --update`.
- All examples use only deterministic stubs for `http_get` and `llm_complete`; pass `--network` to the CLI for real I/O.

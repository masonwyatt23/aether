# Aether Roadmap

Honest, dateless, and derived from real work — every "future" item below
was surfaced by the differential test harness, the real-world example
programs, or the design notes in `spec/RATIONALE.md`.

## Shipped — v0.3

- Lexer, Pratt parser, dual compact/verbose surface syntax, one AST.
- HM type inference + row-polymorphic effect tracking.
- Refinement types with a hand-rolled Fourier–Motzkin solver over linear
  rational arithmetic; proptest-validated soundness; `forall_in` bounded
  quantifier; integer witnesses on refutation.
- Algebraic data types, constructor patterns, exhaustiveness warnings.
- First-class closures, `match` with guards, string interpolation.
- Tree-walking interpreter with automatic provenance + tail-call optimization
  (200k-deep recursion runs in constant stack).
- Bytecode VM (`aether-bc`) — ~18× faster than the tree-walker; AOT `.aebc`
  artifacts; differentially tested against the tree-walker (0 divergences).
- 29 standard-library modules written in Aether itself.
- Agent primitives: `introspect`, `summarize`, `provenance`, `confident`,
  `assume`, `spec`, `tool`.
- LSP server, VS Code extension, browser playground (WASM).
- In-language `test` / `bench` / `snap` blocks; `aether lint`.
- CLI: check, run, test, bench, snap, lint, build, compile, exec, fmt,
  explain, ast, repl, watch, doc, docgen, init.
- GitHub Actions CI, dual MIT/Apache-2.0 license, package metadata.

## v0.4 — ergonomics & correctness

- First-class parametric generics: `fn map<A, B>(xs: [A], f: (A) -> B) -> [B]`.
  Today `std::list` is `[Int]`-only and `std::strlist` is `[Str]`-only.
- Richer pattern-match exhaustiveness (nested constructors, literal ranges).
- Bytecode VM coverage for eval-only values (`ProvChain`, `ModuleSurface`)
  so `provenance`/`introspect` programs run on the BC path instead of
  being classified `BcSkipped`.
- `regex` multiline/global polish surfaced by the real-world examples.
- More `fmt` ergonomics beyond `fmt5` / `fmt_list`.

## v0.5 — verification depth

- Pluggable SMT backend (Z3 / CVC5) for refinements outside the linear
  fragment — non-linear arithmetic, `mod`, quantifier alternation.
- Refinement quantifiers beyond `forall_in` (unbounded `forall` / `exists`
  with solver support).
- Effect handlers — first-class `handle`/`resume`.

## v1.0 — the vision

- `evolve { … }` machinery — backward-compatible language-evolution
  proposals carrying formal witnesses. Parsed today; not yet enacted.
- Native machine-code codegen (Cranelift backend).
- Multi-agent runtime primitives — spawn, channels, supervised tasks.
- A package registry so `import` can resolve third-party modules.

## Non-goals

- Competing with Python/TypeScript for human-authored application code.
  Aether optimizes for agent reasoning-per-token, not human keystrokes.
- A heavyweight effect system that requires monad transformers.
- Unicode operators or emoji syntax — every operator stays single-byte
  ASCII for clean tokenization.

# Aether Design Rationale

> Why these choices, and what we deliberately rejected.

## Why a new language?

We could have built tooling on top of Python or TypeScript. We didn't, because the agent-vs-human tradeoff is fundamentally different from the human-vs-machine tradeoff that existing languages optimize for:

- Python optimizes for **human keystrokes** (indentation, weak types, runtime checks).
- TypeScript optimizes for **human refactoring** (gradual types, structural typing, IDE intelligence).
- Aether optimizes for **agent reasoning per token** (dense syntax, declared effects, provable refinements, queryable structure).

The opportunity is that agents can:
- Tolerate dense syntax — every dropped token is saved bandwidth.
- Reason about formal properties that human authors find tedious.
- Query module structure deterministically instead of grepping source.

## Dual surface (compact + verbose)

Decision: **one AST, two surface syntaxes**.

The compact form `add(x:I,y:I):I!{}=x+y` is what agents emit. The verbose form `fn add(x: Int, y: Int) -> Int effects {} { x + y }` is what humans read when reviewing. `aether fmt` projects between them deterministically.

Rejected: a JSON / S-expression AST as the only format. Decision: machine-friendly **text** is still strictly more grep-able and diff-able than structured formats, and modern BPE tokenizers handle dense text very efficiently when operators are single-byte.

## Operator alphabet

We restricted operators to `: -> ! { } ( ) [ ] = | & < > ~ + - * / % ; , .` plus a few digraphs (`==`, `!=`, `<=`, `>=`, `&&`, `||`, `++`, `|>`, `=>`, `::`, `:=`, `??`). All single-byte ASCII, all common in coding-model training corpora, all unambiguous to tokenize.

Rejected: Unicode operators (`λ`, `→`, `∀`). Decision: cute but they tokenize unpredictably and break grep/sed.

## Effect rows

Decision: track effects as **unordered sets on every function signature**, with row variables for polymorphism.

This is what Koka and Eff demonstrated. The novelty in Aether is making the row visible in the *compact* syntax — `f(x:I):I!{Net,Throw}=...` — so agents can pattern-match on side effects as easily as on types.

Rejected: monadic effects (Haskell-style). Decision: monad transformers are notoriously hard for both humans and agents to compose; effect rows are more direct.

## Refinements via solver, not via tests

Decision: prove postconditions statically using a built-in **Fourier-Motzkin** solver over linear rational arithmetic.

Why not Z3? It's a big external dep and slow to start up; agent workflows want fast iteration. The linear-arithmetic fragment covers most numeric postconditions agents write (`>=`, `<=`, `+`, `-`, scalar multiplication).

For anything outside the fragment (`x*y`, `mod`, `forall`), the solver returns `Unknown` and the compiler emits a *warning* — not an error. Sound but incomplete is the right default for agent code; we'd rather let an agent ship a useful program with one unverified clause than refuse the whole thing.

## Provenance is automatic

Decision: every runtime value wraps a `ProvChain` arena handle that records the op that produced it, its source span, and the chain ids of its inputs.

The cost: ~2x memory overhead. The benefit: an agent can ask `provenance(v)` and get a deterministic, source-anchored explanation of *how* `v` came to be. This is the kind of introspection an agent needs to debug its own generated code without re-running it.

`@no_prov` opts out for hot paths.

Rejected: provenance via reflection on the call stack. Decision: it has to be data, not introspection, to be queryable by code.

## Agent primitives as keywords

`introspect`, `summarize`, `provenance`, `confident`, `assume`, `spec` are reserved words. Decision: they are central enough to the language's purpose that they deserve dedicated syntax (no special-casing of function calls).

Rejected: making them stdlib functions accessible only by name. Decision: keywords make agents emit them more reliably (training data shows keyword usage > function-call patterns).

## Built-in `iter_refine`, not a higher-order helper

The agent workflow primitive `iter_refine(seed, step, budget)` is built in rather than written in Aether using lambdas. Decision: closures aren't yet first-class in the MVP, and even when they are, the iterative-refinement pattern is common enough to deserve a dedicated builtin with a known cost model.

Future: when closures land, `std.iter` becomes a regular Aether module.

## What we postponed

| Feature | Why postponed |
|---|---|
| LSP server | The diagnostics infra is in place; LSP can be added without breaking changes. |
| LLVM codegen | Interpreter is fast enough for agent iteration loops; codegen pays off only at scale. |
| Z3 backend | Linear arithmetic is enough for ~90% of real refinements in our corpus. |
| `evolve { … }` machinery | Aspirational. Parsed but not yet executed; needs versioned AST snapshots. |
| Multi-agent runtime | One language one runtime first; multi-agent is an orthogonal concern. |
| Pattern matching | The parser handles basic ctor patterns; exhaustiveness checking + match expressions are a follow-up. |

Each of these is documented in [LANGUAGE.md](LANGUAGE.md) §9.

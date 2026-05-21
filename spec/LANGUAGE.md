# Aether Language Reference

> Version 0.3. The language and this document evolve together; future
> changes will be governed by `evolve { … }` proposals (parsed today,
> enacted in a later release).

## 1. Philosophy

Aether is designed for **AI agents** as the primary author and reader. The
guiding principles:

1. **Token efficiency over keystroke efficiency.** The compact form `f(x:I,y:I):I!{}=x+y` tokenizes more cleanly than the human-readable equivalent, and the same AST round-trips between the two.
2. **Explicit effects.** Every side effect is named on the function signature. Pure functions are the default.
3. **Refinement types.** Functions can promise things (`where result <= a`) and the compiler proves them via a built-in solver.
4. **Provenance is automatic.** Every runtime value carries a DAG of operations that produced it. Agents querying *why* always get an answer.
5. **Agent primitives are first-class.** `introspect`, `summarize`, `provenance`, `confident`, `assume`, and `spec` are reserved words with built-in semantics.
6. **One AST, two surfaces.** Compact and verbose forms parse to the same tree; `aether fmt` projects either way.

## 2. Lexical structure

Source is UTF-8. Whitespace is insignificant outside literals. Comments are
`# … end-of-line`; doc lines are `## …` and attach to the next declaration.

Numeric literals: `42`, `-7`, `3.14`. Underscores allowed: `1_000_000`.
String literals: `"with \n \t \" escapes"`.

### Keywords

```
fn  let  in  if  then  else  match  with  type  import  from  as  module
effects  effect  where  ensuring  requires  ensures  spec  tool
introspect  summarize  provenance  confident  assume  confidence
result  not  and  or  do
true  false
```

### Operators

```
: -> => ! { } ( ) [ ] , ; .  ?  ~  |  |>  ::  =  :=  ==  !=  <  <=  >  >=
+  -  *  /  %  ++  &&  ||  &  ??
```

All operators were chosen for clean BPE tokenization. No Unicode operators,
no emoji.

### Annotations

`@no_prov` (suppress provenance for this fn body), `@pure` (assert purity),
`@inline` (inlining hint). Annotations precede a declaration.

## 3. Types

### Base types

| Verbose | Compact | Description |
|---|---|---|
| `Int` | `I` | 64-bit signed integer |
| `Float` | `F` | 64-bit float |
| `Bool` | `B` | boolean |
| `Str` | `Str` | UTF-8 string |
| `Bytes` | `Bytes` | byte sequence |
| `Unit` | `U` | the unit value `()` |
| `ModuleSurface` | — | result of `introspect(...)` |
| `ProvChain` | — | first-class provenance chain |

### Composites

- Tuples: `(Int, Str)`
- Lists: `[Int]`
- Records: `{x: Int, y: Int}`
- Sums: `Int | Str`  *(parsed; type-check support partial in MVP)*
- Options: `T?`
- Generics: `Map<K, V>`
- Functions: `(Int, Int) -> Int !{IO}`

### Refinement types

`Base{binder: predicate}` introduces a refinement.

```
Int{n: n > 0}       # positives
Str{s: len(s) < 256}
Int{r: r * r <= n}  # uses outer-scope `n` (dependent)
```

In return position, `where predicate` is sugar; the binder is `result`:

```
fn min(a: Int, b: Int) -> Int where result <= a && result <= b
  effects {} { if a <= b then a else b }
```

### Confidence types

`T ~ confidence(p)` marks a value as having confidence `p ∈ [0,1]`.

```
let answer : Str ~ confidence(0.83) = llm.complete(prompt)
```

`confident(value, p)` is the runtime constructor.

## 4. Effects

Effect rows are unordered sets, declared on function signatures. The
built-in effects:

```
IO     # console / stdout
Net    # network access
FS     # filesystem
State  # mutable state (cells)
Rand   # randomness
Async  # async / awaitable
Throw  # may raise
```

A function may also reference a *row variable* (lowercase single letter)
to be effect-polymorphic: `fn pipe<E>(x: Str, f: (Str) -> Str !E) -> Str !E`.

**Effect inference is conservative**: a call's effects are added to the
caller's observed set; any observed effect not in the declared row is a
compile error.

## 5. Refinement verification

The solver in `aether-types::refine` decides implications in **linear
rational arithmetic** with boolean combinations:

- Atoms: `(linear expression) op 0` for `op ∈ {<, ≤, =, ≠}`.
- Booleans: `&&`, `||`, `!`, `=>`.
- Combinations of atoms by conjunction, disjunction, negation.

The procedure is **sound** (whenever it says *Proved*, the implication
holds) and **incomplete**: anything outside the linear fragment returns
`Unknown`, which the compiler downgrades to a warning so that useful
programs aren't rejected.

### Extended fragments (since v0.3.1)

**Constant-folding multiplication.** `k * x` and `x * k` for any literal `k`
are linearised: `0 * x → 0`, `1 * x → x`, `3 * x → 3x`.  Fully-constant
products (`2 * 3`) fold to their value.  Genuine non-linear terms (`x * y`)
still bail to `Unknown`.

**`mod` / `div` by a positive literal.**  `x % k` for literal `k > 0`
introduces a fresh variable bounded `0 ≤ m < k`, enabling proofs like
`x % 3 >= 0` and `x % 3 < 3`.  `x / k` introduces a fresh variable with
no additional bounds (sound but incomplete).

**Equality propagation.**  Any hypothesis of the form `x == k` (single
variable equals constant) is eagerly substituted into all other hypotheses
and the goal before Fourier–Motzkin runs.  This lets the solver prove chains
like `[x == 5, y == x + 1] ⊢ y == 6` in one pass.

**Bounded universal quantifier.**  `forall_in(x, lo, hi, pred)` unrolls the
quantifier over `[lo, hi]` (max 1024 iterations) by conjoining the
instantiated predicate for each integer in range.

### Optional SMT escalation

When the built-in solver returns `Unknown` — a goal genuinely outside the
linear fragment, such as `x * y >= 0` — the query is escalated to an
external SMT solver. If a `z3` binary is on `PATH`, the goal is translated to
SMT-LIB2 over the integers and `H ∧ ¬G` is checked for satisfiability:
`unsat` proves the implication, `sat` yields a counterexample. This needs no
build-time dependency: when `z3` is absent (or `AETHER_DISABLE_SMT` is set)
the escalation is a no-op and the verdict stays `Unknown`. Escalation only
ever upgrades an `Unknown`, so it can never turn a sound result unsound.

Path-sensitive reasoning: when checking ensures-clauses, the checker
case-splits on `if`/`else` and threads `let` bindings into the assumed
context.

## 6. Agent primitives

| Primitive | Signature | Semantics |
|---|---|---|
| `introspect(target: Str, depth: Int?) -> ModuleSurface` | Returns the module's exports, types, effects, and docs in one structured value. |
| `summarize(scope: Str, budget: Int) -> Str` | Returns a budgeted natural-language précis (content-hash cached). |
| `provenance(v) -> ProvChain` | Returns the DAG of operations that produced `v`. |
| `confident(v, p: Float) -> T~confidence(p)` | Tags `v` with confidence `p`. |
| `assume(predicate: Bool)` | Declares an axiom local to the proof scope. Surfaces in provenance. |
| `spec { requires …; ensures …; effects … }` | Attaches a formal contract to the surrounding fn. |
| `tool name(args) -> ret !{effects}` | First-class external tool declaration. |

`evolve { from: vN, to: vN+1, witness: … }` is parsed but not yet enacted in
the MVP — see [RATIONALE.md](RATIONALE.md).

## 7. Standard library

Core built-ins live in `aether-eval::builtins` (`print`, `str`, `int`, `len`,
`abs`, `max`, `min`, `iter_refine`, plus every `*_native` helper). Everything
else is written in Aether itself.

The standard library spans **29 modules** hosted as `.ae` source files
under `crates/aether-stdlib/aether/std/`:
`plan, iter, mem, proof, json, list, strlist, string, map, path, time, env,
fmt, result, regex, sys, math, base64, hash, uuid, random, date, log, term,
yaml, fs, cache, retry, http_server`.

## 8. Grammar

See [grammar.ebnf](grammar.ebnf) for the formal EBNF.

## 9. Shipped since the MVP

- ✅ **LSP server** (`aether-lsp`) — diagnostics, hover, goto-def, completion.
- ✅ **WASM codegen** — `aether-wasm` + browser playground.
- ✅ **Bytecode VM** (`aether-bc`) — ~23× faster on `fib(20)`; AOT `.aebc` artifacts.
- ✅ **Pattern matching** with constructor patterns + exhaustiveness warnings.
- ✅ **First-class closures** captured by value.
- ✅ **Algebraic data types** (`type T = A(..) | B(..)`).
- ✅ **Refinement quantifiers** — `forall_in(x, lo, hi, pred)` bounded universal.
- ✅ **String interpolation**, **in-language test/bench/snapshot/lint**.
- ✅ **Differential testing** locks the tree-walker and bytecode VM to identical behavior.
- ✅ **First-class parametric generics** — `fn id<A>(x: A) -> A`; type
  parameters are inferred at each call site, and a single-letter parameter
  name shadows any builtin abbreviation (so `<B>` is a generic, not `Bool`).

## 10. Future work

- Pluggable SMT backend (Z3 / CVC5) for refinements beyond linear arithmetic.
- `evolve { … }` machinery — backward-compatible language-evolution proposals
  with formal witnesses (currently parse-only).
- Multi-agent runtime primitives.
- Higher-kinded and bounded generics (constraints on type parameters).
- Native machine-code codegen (Cranelift).

# Aether Compiler Internals

> A guided walkthrough of how the Aether compiler works, for someone learning
> to build a verified programming language.

This document is a companion to reading the source. Every code reference uses
`path:line` so you can jump straight to the relevant spot.

---

## 1. Overview

### Pipeline diagram

```
Source text (.ae / .aev)
        │
        ▼
  ┌───────────┐
  │  aether-  │  logos tokenizer → Vec<Token>
  │  lexer    │  crates/aether-lexer/src/lib.rs
  └─────┬─────┘
        │ Vec<Token>
        ▼
  ┌───────────┐
  │  aether-  │  Pratt parser → Module (AST)
  │  parser   │  crates/aether-parser/src/lib.rs
  └─────┬─────┘
        │ Module
        ▼
  ┌───────────┐
  │  aether-  │  HM bidirectional type check + effect inference
  │  types    │  → Vec<Diagnostic>
  └─────┬─────┘
        │ Module (unchanged; diagnostics are advisory)
       / \
      /   \
     ▼     ▼
┌────────┐  ┌────────┐
│aether- │  │aether- │
│eval    │  │bc      │
│(tree   │  │(bytecode│
│walker) │  │VM)     │
└────────┘  └────────┘
     │              │
     └──────┬────────┘
            ▼
      aether-difftest
      (cross-checks outputs)
```

### The 12 crates and what each owns

| Crate | Role |
|-------|------|
| `aether-ast` | Canonical AST, spans, provenance arena |
| `aether-lexer` | logos-based tokenizer |
| `aether-parser` | Pratt parser; compact ↔ verbose pretty-printer |
| `aether-types` | HM type/effect checker, FM refinement solver, SMT escalation |
| `aether-eval` | Tree-walking interpreter with provenance and TCO |
| `aether-bc` | Bytecode compiler and stack-based VM |
| `aether-stdlib` | Standard library modules (`.ae` source files) |
| `aether-cli` | `aether` command (run, check, fmt, test, repl, docgen, …) |
| `aether-lsp` | Language server (hover, diagnostics, symbols) |
| `aether-difftest` | Differential testing harness (tree-walker vs VM) |
| `aether-tools-net` | HTTP and LLM tool integrations |
| `aether-wasm` | WebAssembly build shim |

### Dependency graph (abridged)

```
aether-ast   (no dep on other aether crates)
    ↑
aether-lexer
    ↑
aether-parser
    ↑               ↑
aether-types    aether-eval   aether-bc
         \           |         /
          \          ↓        /
           →  aether-difftest
                    ↑
               aether-cli
```

`aether-ast` is the leaf. Every other crate depends on it. The type checker,
interpreter, and bytecode compiler all consume the same `Module` value that the
parser produces — there is no separate IR.

---

## 2. The AST is the contract

Everything in the pipeline converges on `aether_ast::Module`. The crate
intentionally has no dependencies on any other Aether crate:
`crates/aether-ast/src/lib.rs:1`.

### The six modules of aether-ast

```
aether-ast/src/
  expr.rs   — Expr, Lit, BinOp, UnOp, Stmt, MatchArm, Arg, StrPart
  ty.rs     — Type, TyCon, Effect, EffectRow, Refinement
  decl.rs   — Decl, FnDecl, LetDecl, TypeAliasDecl, ImportDecl, ToolDecl, Module, Param, SpecBlock
  pat.rs    — Pattern
  span.rs   — Span, FileId, SourceMap
  prov.rs   — ProvArena, ProvChain, ProvNode, ProvOp
```

### Spans are mandatory

Every AST node carries a `Span`. The `Span` type is twelve bytes:

```rust
// crates/aether-ast/src/span.rs:11
pub struct Span {
    pub file: FileId,
    pub start: u32,
    pub end: u32,
}
```

`FileId` is a newtype around `u32` that indexes into a `SourceMap`. The source
map stores the raw source text; `SourceMap::line_col` resolves a span to a
`(line, col)` pair by walking the source string (`span.rs:99`). Every error
message, every LSP hover, and every provenance record is anchored to a span.

`Span::DUMMY` (file `FileId(u32::MAX)`, offsets 0..0) exists only for
programmatically synthesised nodes; `is_dummy()` lets callers detect it
(`span.rs:49`).

`Span::join` takes the union of two spans — used throughout the parser when a
node spans from the start of a sub-expression to the end of another:

```rust
// crates/aether-ast/src/span.rs:34
pub fn join(self, other: Span) -> Span {
    Span {
        file: self.file,
        start: self.start.min(other.start),
        end: self.end.max(other.end),
    }
}
```

### The Expr enum

`Expr` is the most important type in the codebase (`expr.rs:116`). Its variants
map directly to language constructs:

```rust
pub enum Expr {
    Lit(Lit, Span),
    Var(String, Span),
    Bin(BinOp, Box<Expr>, Box<Expr>, Span),
    Un(UnOp, Box<Expr>, Span),
    Call { callee: Box<Expr>, args: Vec<Arg>, span: Span },
    Lambda { params: Vec<Param>, ret: Option<Type>, body: Box<Expr>, span: Span },
    Let { pat: Pattern, ty: Option<Type>, value: Box<Expr>, body: Box<Expr>, span: Span },
    If { cond: Box<Expr>, then_branch: Box<Expr>, else_branch: Box<Expr>, span: Span },
    Block { stmts: Vec<Stmt>, tail: Option<Box<Expr>>, span: Span },
    Record(Vec<(String, Expr)>, Span),
    Tuple(Vec<Expr>, Span),
    List(Vec<Expr>, Span),
    Field(Box<Expr>, String, Span),
    Index(Box<Expr>, Box<Expr>, Span),
    Match { scrutinee: Box<Expr>, arms: Vec<MatchArm>, span: Span },
    Confident { value: Box<Expr>, p: Box<Expr>, span: Span },
    Assume(Box<Expr>, Span),
    Annot { expr: Box<Expr>, ty: Type, span: Span },
    StrInterp { parts: Vec<StrPart>, span: Span },
}
```

The `Expr::span()` method (`expr.rs:190`) extracts the span from any variant
using a single `match` — every consumer calls this rather than pattern-matching
on the structure themselves.

### Types and effect rows

`Type` is the second major recursive type (`ty.rs:166`). The key variants for
understanding the design:

- `Type::Var(String, Span)` — a type variable, used for inference and generics.
- `Type::Refined { base, refinement, .. }` — `Int{n: n > 0}` becomes a
  `Refined` wrapping `Type::Con(TyCon::Int, _)` with a `Refinement { binder:
  "n", pred: parse("n > 0"), .. }`.
- `Type::Fun { params, ret, effects, .. }` — function type carrying an
  `EffectRow` directly.

`EffectRow` is structurally an ordered (sorted, deduped) `Vec<Effect>` plus an
optional row-variable tail (`ty.rs:110`):

```rust
pub struct EffectRow {
    pub effects: Vec<Effect>,
    pub tail: Option<String>,  // row polymorphism: !{Net, e}
}
```

`EffectRow::from_iter` sorts and deduplicates to maintain a canonical form
(`ty.rs:123`). `EffectRow::union` merges two rows for effect propagation
(`ty.rs:137`).

Built-in effects are `IO, Net, FS, State, Rand, Async, Throw`; `Custom(String)`
handles user-declared effects (`ty.rs:67`).

### The provenance arena

Every runtime value carries a `ProvChain` — a handle into a shared
`ProvArena`. The arena is a `Mutex<Vec<ProvNode>>` implemented with a hand-rolled
spinlock to avoid an external dependency in `aether-ast` (`prov.rs:55`):

```rust
pub struct ProvArena {
    nodes: parking_lot_lite::Mutex<Vec<ProvNode>>,
}
```

Each `ProvNode` records what operation produced a value, the source span, and
the ids of the parent nodes (`prov.rs:37`):

```rust
pub struct ProvNode {
    pub op: ProvOp,
    pub span: Span,
    pub parents: Vec<usize>,
}
```

`ProvChain` is cheaply cloneable — it is just an `Arc<ProvArena>` plus a `head`
index (`prov.rs:44`). Walking the DAG for `provenance(v)` uses a reverse-postorder
DFS from `head` (`prov.rs:175`).

`ProvOp` tags what kind of node this is: `Lit`, `Var(name)`, `Call(name)`,
`BinOp(symbol)`, `Tool(name)`, etc. (`prov.rs:15`). The key property: every
operation that produces a value must call `ProvChain::extend` with the arena,
the op, the source span, and the parent node ids. Missing a call means that
value's provenance chain is broken.

`FnDecl::no_prov` (`decl.rs:37`) lets a function opt out of provenance
allocation for hot paths via `@no_prov`.

---

## 3. Lexing

The tokenizer lives in `crates/aether-lexer/src/lib.rs`. It uses the `logos`
crate, which generates a DFA-based lexer from attribute annotations directly on
an enum:

```rust
// crates/aether-lexer/src/lib.rs:12
#[derive(Logos, Debug, Clone, PartialEq)]
#[logos(skip r"[ \t\r\n\f]+")]
pub enum Tok {
    #[regex(r"-?[0-9][0-9_]*", |lex| lex.slice().replace('_', "").parse::<i64>().ok())]
    Int(i64),
    // ...
}
```

`logos::skip` consumes whitespace without emitting a token. Each `#[token]` or
`#[regex]` annotation with its closure describes both the pattern and the
payload extraction. The entire grammar is expressed in this single enum — there
is no separate lexer specification file.

### One tokenizer for both surface syntaxes

Compact (`.ae`) and verbose (`.aev`) Aether share exactly one set of tokens.
The difference is at the parser level, not the lexer level. The lexer
recognises both `I` (compact Int) and `Int` (verbose Int) as a bare `Ident`
token; `TyCon::from_str` in the parser maps both to `TyCon::Int` (`ty.rs:49`).

Keywords cover both worlds: `fn`, `let`, `if`/`then`/`else`, `match`/`with`,
`effects`, `where`, `requires`, `ensures`, `spec`, `tool`, `module`, `import`,
`from`, `as`, `do`, plus the agent-native keywords `introspect`, `summarize`,
`provenance`, `confident`, `assume` (`lib.rs:37`–`100`).

### Docstrings vs comments

Plain comments start with `#` and are skipped (`lib.rs:192`):
```rust
#[regex(r"#[^\n]*", logos::skip)]
Comment,
```

Doc lines start with `##` and are emitted as `DocLine(String)` tokens
(`lib.rs:185`). The parser collects consecutive `DocLine` tokens into a `doc_buf`
before each declaration. This is the attachment mechanism: a `## ...` run that
immediately precedes a `fn` binds to that function's `FnDecl::doc` field.

### The `lex` function

`lex(file: FileId, source: &str) -> Result<Vec<Token>, LexError>` (`lib.rs:237`)
drives the logos iterator and wraps each token with its `Span`:

```rust
while let Some(res) = lex.next() {
    let r = lex.span();         // byte range from logos
    match res {
        Ok(tok) => out.push(Token { tok, span: Span::new(file, r) }),
        Err(()) => return Err(LexError::Unexpected { ... }),
    }
}
```

The produced `Vec<Token>` is consumed directly by the parser.

---

## 4. Parsing

The parser is a hand-written recursive descent parser with **Pratt
(precedence-climbing) parsing** for binary expressions.
`crates/aether-parser/src/lib.rs`.

### The Parser struct

```rust
// crates/aether-parser/src/lib.rs:51
struct Parser<'a> {
    file: FileId,
    source: &'a str,
    toks: Vec<Token>,
    pos: usize,
    doc_buf: Vec<String>,
    active_generics: Vec<String>,
}
```

`pos` is a cursor into the flat token slice. There is no token stream or
iterator — random access via `peek_at(offset)` is used to distinguish
ambiguous productions (e.g. `is_adt_body` peeks two tokens ahead to decide
whether `type Foo = Bar(...)` is an ADT or an alias, `lib.rs:392`).

Core token utilities: `peek()`, `bump()`, `eat(want)`, `expect(want, ctx)`,
`expect_ident(ctx)`.

### The two declaration forms

`parse_decl` dispatches based on the leading token (`lib.rs:253`). If the
module keyword is `fn`, it calls `parse_fn_decl` (verbose form). If the leading
token is an `Ident` (not a keyword), it calls `parse_compact_fn_decl`:

**Verbose:**
```
fn add(x: Int, y: Int) -> Int effects {} { x + y }
```
Parsed by `parse_fn_decl` (`lib.rs:237`): `fn` keyword → name → optional `<A,B>`
generics → `(params)` → `->` return type → optional `where` ensures → `effects
{...}` → optional `spec { ... }` block → block body.

**Compact:**
```
add(x:I,y:I):I!{}=x+y
```
Parsed by `parse_compact_fn_decl` (`lib.rs:285`): name → optional generics →
`(params)` → `:` return type → `!{...}` effects → optional `where` → `=` →
expression body.

Both paths produce the same `FnDecl` value. The `pretty.rs` module contains the
inverse: `fn_decl(f, Form::Verbose, out)` vs `fn_decl(f, Form::Compact, out)`
(`pretty.rs:97`). Compact form omits spaces around operators and uses `I`/`B`/`F`/`U`
type abbreviations; verbose form uses `Int`/`Bool`/`Float`/`Unit` and inserts
whitespace.

### Pratt parsing for binary expressions

`parse_expr_bp(min_bp: u8)` is the binding-power (precedence-climbing) entry
point (`lib.rs:787`):

```rust
fn parse_expr_bp(&mut self, min_bp: u8) -> PResult<Expr> {
    let mut lhs = self.parse_expr_unary()?;
    loop {
        let (op, lbp, rbp) = match self.peek() {
            Some(Tok::FatArrow) => (BinOp::Implies, 0, 1),
            Some(Tok::OrOr) | Some(Tok::OrKw) => (BinOp::Or, 1, 2),
            Some(Tok::AndAnd) | Some(Tok::AndKw) => (BinOp::And, 3, 4),
            Some(Tok::EqEq) => (BinOp::Eq, 5, 6),
            // ...
            Some(Tok::Star) => (BinOp::Mul, 11, 12),
            _ => break,
        };
        if lbp < min_bp { break; }
        self.bump();
        let rhs = self.parse_expr_bp(rbp)?;
        let sp = lhs.span().join(rhs.span());
        lhs = Expr::Bin(op, Box::new(lhs), Box::new(rhs), sp);
    }
    Ok(lhs)
}
```

**How it works:**

The table assigns each operator two binding powers: `lbp` (left) and `rbp`
(right). The outer `parse_expr_bp(min_bp)` call says "parse an expression that
can bind at least as tightly as `min_bp` to its left context."

- The loop keeps consuming operators as long as `lbp >= min_bp` — i.e., the
  current operator binds tighter than what the caller required.
- Right-hand-side is parsed with `parse_expr_bp(rbp)`, where `rbp = lbp + 1`
  for left-associative operators. This ensures `a + b + c` folds left: after
  consuming the first `+`, the recursive call for the RHS uses `rbp=10`, which
  refuses to consume another `+` at `lbp=9`.
- For right-associative operators you would set `rbp = lbp` (the `Implies`
  operator uses `(0, 1)` — it's right-associative at the lowest precedence so
  `a => b => c` means `a => (b => c)`).

**Precedence table** (ascending tightness):

```
=>  (0/1)   →  Implies
|| or  (1/2)   →  Or
&& and (3/4)   →  And
== !=  (5/6)   →  Eq, Neq
< <= > >= (7/8) → comparisons
+ - ++ (9/10)  → Add, Sub, Concat
* / %  (11/12) → Mul, Div, Mod
```

Unary operators (`-`, `!`, `not`) are handled before the loop in
`parse_expr_unary` (`lib.rs:800`). Postfix operations — call `f(...)`, field
access `.name`, index `[i]`, pipe `|>` — are handled in `parse_expr_postfix`
(`lib.rs:813`), which wraps them around the result of `parse_expr_atom`.

`parse_expr_top()` is just `parse_expr_bp(0)` — it will accept any
operator.

### Refinement types in the parser

Inline refinements are parsed in `parse_type` as a postfix operator on a base
type (`lib.rs:600`). When the parser sees `{` after a type, it looks ahead:
`{IDENT :` means the user wrote an explicit binder (`Int{n: n > 0}`); anything
else synthesises the binder `"x"`. Either way the result is `Type::Refined`.

### Spec blocks

A `spec { requires ...; ensures ...; effects ... }` block (`lib.rs:319`) fills
`FnDecl::spec: SpecBlock`. The `requires` clauses become assumed
pre-conditions inside the checker's `Scope`; `ensures` clauses become proof
obligations.

---

## 5. Type and effect checking

The checker lives in `crates/aether-types/src/check.rs`. It is a **bidirectional,
two-pass** checker built on Hindley-Milner-style type inference — simplified
significantly from a full HM system because Aether's MVP requires explicit
parameter types on all function declarations.

### Two-pass structure

`check_module` runs two passes over `Module::decls` (`check.rs:48`):

**Pass 1** (signature collection): for every `Decl::Fn`, `Decl::Tool`, and
`Decl::TypeAlias`, register the signature in a `TypeCtx`. ADT constructors are
registered both as constructors in `ctx.ctors` and as callable functions in
`ctx.funs` so `check_call` can validate their argument types. Builtins are
installed first via `install_builtins` in `crates/aether-types/src/builtins.rs`.

**Pass 2** (body checking): for every `Decl::Fn`, call `check_fn`. At this
point all mutual-recursion forward references are already in `ctx`.

### Scope and path conditions

`Scope` is a stack of `(name, Type)` bindings plus a `Vec<Expr>` of accumulated
path conditions (`check.rs:133`):

```rust
struct Scope {
    vars: Vec<(String, Type)>,
    path: Vec<Expr>,  // boolean exprs known to be true here
}
```

Path conditions grow in two places:
1. Refined parameter types: if `p: Int{n: n > 0}`, the predicate `n > 0` (with
   `n` substituted for the binder) is pushed into `scope.path`
   (`check.rs:170`).
2. `spec.requires` clauses are pushed as assumed hypotheses (`check.rs:176`).
3. Inside `prove_ensures`, the `if`/`else` split forks the scope: the then-branch
   gets `cond` added; the else-branch gets `!cond` added (`check.rs:222`).

### Effect tracking

`check_fn` passes a `&mut HashSet<Effect>` called `observed` to
`check_expr`. Every time the checker descends into a `Call`, it merges the
callee's declared `EffectRow::effects` into `observed` (`check.rs:760`). After
the body is checked, `observed` is compared against the function's declared
`effects`: any observed effect not in the declared set is an error
(`check.rs:192`).

Effect rows are `unordered sets` — they have no notion of ordering. The union
operation in `EffectRow::union` collects and re-sorts. A row-polymorphic
function (`fn f<E>(...) !{Net, E}`) stores the row variable in
`EffectRow::tail`; the MVP checker allows the tail to pass through without full
unification.

### Generic instantiation

When the checker encounters a call to a generic function (`fn id<A>(x: A) ->
A`), it instantiates the type parameters by walking the formal parameter types
against the actual argument types (`check.rs:800`):

```rust
fn collect_subst(formal: &Type, actual: &Type, generics: &[String],
                 subst: &mut HashMap<String, Type>) { ... }
```

`as_generic_param` recognises a type variable as a generic parameter if it
appears in the function's `generics` list, matching both `Type::Var("A", _)`
(single-letter lowercase) and `Type::Generic { name: "A", args: [], .. }`
(multi-character or uppercase, `check.rs:830`). The resulting substitution map
is then applied to parameter types and the return type via `apply_subst`.

This is not full HM unification — there is no unification variable and no
occurs-check. It is a one-pass, left-to-right argument scan that pins each
generic to the first actual type it sees. Consistency is verified afterward: if
two arguments both constrain generic `A` but to different types, an error is
emitted.

### "Did you mean?" diagnostics

When a variable or function is not found, the checker calls `closest_match`
from `crates/aether-types/src/suggest.rs`. This is a vanilla Wagner-Fischer
Levenshtein implementation (`suggest.rs:4`) that returns a suggestion only when
there is a single closest candidate within edit distance `max_distance` (ties
return `None` to avoid guessing, `suggest.rs:61`).

---

## 6. Refinement verification

This is the most intellectually interesting part of the codebase. The files are
`crates/aether-types/src/refine.rs` (the decision procedure) and
`crates/aether-types/src/smt.rs` (the optional Z3 escalation).

### From `where` clause to proof obligation

A function like:

```aether
fn abs(n: Int) -> Int{r: r >= 0}
  where result >= 0
  effects {} {
  if n >= 0 then n else -n
}
```

After parsing, `FnDecl::spec.ensures` holds one `Expr`: the parsed form of
`result >= 0`. In `check_fn`, after the body is type-checked, `prove_ensures`
is called (`check.rs:198`).

`prove_ensures` walks the body AST **symbolically**, splitting on `if`/`else`
and `match` branches to generate per-path proof obligations. For each path it
substitutes `result` with the leaf expression and calls `judge`:

```rust
// check.rs:222 (prove_ensures inner walk)
Expr::If { cond, then_branch, else_branch, .. } => {
    let mut s_then = scope.clone();
    s_then.push_assume((**cond).clone());      // then-path: cond is true
    walk(&s_then, ens, then_branch, diags);
    let mut s_else = scope.clone();
    s_else.push_assume(Expr::Un(UnOp::Not, cond.clone(), cond.span()));
    walk(&s_else, ens, else_branch, diags);    // else-path: !cond is true
}
```

For the `abs` example this produces two obligations:
- Path: `n >= 0`. Goal: `n >= 0`. (Trivially proved.)
- Path: `!(n >= 0)`, i.e. `n < 0`. Goal: `-n >= 0`. (Equivalent to `n <= 0`.)

### The Fourier-Motzkin decision procedure

`prove_linear(hypotheses, goal)` in `refine.rs` is the heart of the solver
(`refine.rs` at the `prove_linear` function). It:

1. **Compiles** each hypothesis and the goal from `Expr` to `Form`, a boolean
   formula tree over `Constraint` atoms of the form `Lin op 0` where `Lin` is
   an integer linear expression (`BTreeMap<String, i64>` plus a constant).
   `expr_to_form` handles `And`, `Or`, `Not`, `Implies`, and the comparison
   operators. `expr_to_lin` handles `+`, `-`, `*` (one side constant), `%` and
   `/` (with constant divisors replaced by fresh variables).

2. **To prove H ⊢ G**, the solver checks unsat of `H ∧ ¬G`. It negates `G`
   and converts the combined formula to **DNF** (disjunctive normal form) —
   a list of conjunctions of constraints. If every disjunct is unsatisfiable,
   the implication holds.

3. **Equality propagation** is applied before DNF conversion: any hypothesis
   `x == k` is used to substitute `x = k` throughout all remaining hypotheses
   and the goal, simplifying the system (`refine.rs:455`).

4. **DNF conversion** uses `push_not` (De Morgan) then `nnf_to_dnf`
   (cartesian product for `And`, concatenation for `Or`). A `budget` of 64
   disjuncts caps combinatorial explosion; exceeding it returns `Unknown`.

5. **FM satisfiability check** (`fm_check_sat`) eliminates one variable at a
   time. For a variable `x`, it partitions constraints into those where `x`
   appears with positive coefficient (upper bounds on `x`), negative coefficient
   (lower bounds on `x`), and those where `x` does not appear. FM produces all
   pairwise combinations of an upper and a lower bound (eliminating `x`). The
   remaining constraints are checked recursively. When no variables remain, all
   constant constraints are evaluated directly.

```
To check SAT of { n >= 0, -n < 0 }:
  n has: upper bound none, lower bound n >= 0 (-n <= 0)
         constraint -n < 0 means -n < 0, i.e. n > 0 → lower bound -n+1 <= 0
  Eliminate n: cross product of uppers with lowers. No uppers → check remaining constants.
  All remaining are constant-satisfied → SAT.
```

**Soundness:** The procedure is sound because FM elimination over the rationals
is a complete decision procedure for linear arithmetic over Q. If FM says UNSAT,
no assignment of rationals satisfies the system, so no assignment of integers can
either (the integers are a subset of the rationals). A `Proved` verdict is never
wrong.

**Incompleteness:** FM is incomplete for `Integer arithmetic` (not Q) because
the integer hull of a Q-satisfiable system may be empty. In practice Aether
only has this problem for constraints involving `%` and `/` (where fresh
variables approximate the result) or non-linear expressions (`x * y`). These
return `Verdict::Unknown`, which the checker downgrades to a warning.

### The Verdict type

```rust
// refine.rs:17
pub enum Verdict {
    Proved,
    RefutedWith { values: BTreeMap<String, i64> },
    Unknown,
}
```

`RefutedWith` carries a witness — a partial variable assignment that satisfies
the hypotheses but not the goal. This is extracted from the FM elimination trace.
`judge` in `check.rs:303` emits an error with the witness if available, giving
the user a concrete counterexample.

### SMT escalation

When `prove_linear` returns `Unknown` (outside the linear fragment), `prove`
falls through to `crate::smt::prove_smt` (`refine.rs` at `prove()`):

```rust
pub fn prove(hypotheses: &[Expr], goal: &Expr) -> Verdict {
    match prove_linear(hypotheses, goal) {
        Verdict::Unknown => crate::smt::prove_smt(hypotheses, goal),
        decided => decided,
    }
}
```

`prove_smt` (`smt.rs:38`) translates the goal to an SMT-LIB2 script with
`QF_NIA` (quantifier-free non-linear integer arithmetic) logic and shells out
to a `z3` binary if one is on `PATH`. The question asked is whether `H ∧ ¬G`
is satisfiable:

```
(set-logic QF_NIA)
(declare-const x Int)
(assert (>= x 0))          ; hypothesis
(assert (not (>= x 0)))    ; negated goal
(check-sat)
(get-model)
```

`unsat` → `Proved`. `sat` → `RefutedWith` (model is parsed to extract integer
variable assignments). Any error or `unknown` → `Unknown`. The rule "never
return a false `Proved`" is invariant: every uncertain path returns `Unknown`
(`smt.rs:23`). Z3 is detected once via `OnceLock` and the entire escalation
is a no-op if `AETHER_DISABLE_SMT` is set or no binary exists.

---

## 7. Two runtimes

### The tree-walking interpreter (aether-eval)

`crates/aether-eval/src/lib.rs`. The `Runtime` struct holds the `Module`,
the `ProvArena`, stdout capture, a tool registry, and a summarize cache
(`lib.rs:115`). Every call goes through `eval_fn` → `eval_tail` → `eval`.

**Provenance** is automatic: every `eval` operation that produces a value calls
`ProvChain::extend` with the arena, the `ProvOp` for that operation, the source
span, and the parent node ids. For example, `eval` on `Expr::Bin` evaluates
both operands, then:

```rust
ProvChain::extend(
    self.arena.clone(),
    ProvOp::BinOp(op.as_str().to_string()),
    *span,
    vec![lv.prov().head, rv.prov().head],
)
```

**Tail-call optimisation** uses a trampoline (`lib.rs:384`). `eval_fn` runs a
loop; `eval_tail` analyses the expression's tail position and returns either
`TailStep::Done(value)` or `TailStep::TailCall { fn_name, args, .. }`. On a
tail call the loop re-binds parameters and iterates without growing the Rust
stack. Tail positions tracked: `Block { tail }`, `If` (both branches), `Match`
(each arm body), `Annot` (transparent), and `Call` when the callee is a
module-level user function not shadowed by a local closure (`lib.rs:508`).

**Value type.** Every runtime value is `Value` from `value.rs:8`. Each variant
carries a `ProvChain` as its second (or named `prov`) field:

```rust
pub enum Value {
    Int(i64, ProvChain),
    Float(f64, ProvChain),
    Str(String, ProvChain),
    Closure { params, body, env, prov },
    Ctor { name, args, prov },
    // ...
}
```

`Value::with_prov` replaces the chain without changing the data — used when the
interpreter adds a new provenance node for a compound operation.

`Value::eq_val` and `Value::cmp_val` ignore provenance (`value.rs:146`) —
equality is structural, not identity.

### The bytecode VM (aether-bc)

`crates/aether-bc/src/`. The motivation is performance: the tree-walker pays
for AST cloning and heap allocation on every step; the bytecode VM works on a
flat `Vec<Op>` with a value stack.

The **instruction set** (`op.rs:11`) is a simple stack machine:

- Push literals: `PushInt`, `PushBool`, `PushStr(idx)`, `PushFloat`, `PushUnit`
- Locals: `LoadLocal(slot)`, `StoreLocal(slot)` (parameters are slots 0..arity)
- Arithmetic/comparison/logic: `Add`, `Sub`, `Eq`, `Lt`, etc.
- Control: `Jump(offset)`, `JumpIfFalse(offset)`
- Calls: `Call { fn_idx, argc }`, `CallBuiltin { id, argc }`, `CallBuiltinDyn { name_idx, argc }`, `Ret`
- Closures: `MakeClosure { fn_idx, captured: Vec<u16> }`, `CallClosure { argc }`
- ADTs: `Ctor { name_idx, argc }`, `MatchCtor { name_idx, expect_arity, jump_if_miss }`, `CtorField(idx)`
- Aggregate: `MakeTuple`, `TupleGet`, `MakeRecord`, `FieldGet`, `MakeList`, `Index`
- String interp: `ToStr`, `Concat`

Jump offsets are relative (to the instruction after the jump). The compiler
emits a placeholder `Jump(0)` and then calls `patch_jump(placeholder_ip)` once
the target is known (`compile.rs:214`).

**Compilation** (`compile.rs`). `compile_module` iterates function declarations,
builds a `fn_index: HashMap<String, u32>` for call resolution, and compiles
each `FnDecl` with `compile_fn`. Each function gets a `FnCtx` with a scope
stack, a `n_locals` counter, and the shared constants pool. Parameters become
locals 0..arity. `compile_expr` recursively emits instructions.

Closures are compiled as separate `BytecodeFn` entries appended to the program's
function list. `free_vars_expr` performs a free-variable analysis (`compile.rs`
at the `free_vars_expr` function) to determine what must be captured; `MakeClosure`
emits the capture list as local slot indices into the enclosing frame.

**The bytecode VM does not track provenance.** This is the key semantic
difference: the tree-walker carries a `ProvChain` on every value; the BC VM
strips it entirely. This is why both runtimes exist.

### Differential testing keeps them honest

`crates/aether-difftest/src/lib.rs`. `diff_run(source)` parses the source,
loads stdlib imports, runs both the tree-walker and the bytecode VM, and
compares their `(stdout, return_value)` outputs:

```rust
// difftest/src/lib.rs
pub enum Agreement {
    Match,
    Differ { tree: Box<RunOutcome>, bc: Box<RunOutcome> },
    BcSkipped { reason: String },
    BothFailed { ... },
    Nondeterministic { reason: String },
    // ...
}
```

`Agreement::Differ` is the failure mode that matters. The harness knows about
nondeterministic builtins (uuid, random, etc.) and skips them. The
`crates/aether-difftest/tests/examples.rs` test file runs every file under
`examples/` through this harness.

---

## 8. Guided exercise: add a new builtin function

**Goal:** add a `clamp(n, lo, hi)` builtin that constrains `n` to the range
`[lo, hi]`.

This exercise touches six files in a fixed order. None of the core language
mechanisms change — you are only adding a new entry point into the existing
infrastructure.

### Step 1: Register the type signature

**File:** `crates/aether-types/src/builtins.rs`

`install_builtins` registers all built-in function signatures into the
`TypeCtx` before user code is checked. Add:

```rust
// after the existing `min` entry
ctx.insert_fn(
    "clamp".into(),
    sig(
        vec![("n", ty_con(Int)), ("lo", ty_con(Int)), ("hi", ty_con(Int))],
        ty_con(Int),
        vec![],   // no effects — pure function
    ),
);
```

This makes the type checker accept calls to `clamp(n, lo, hi)` with three
`Int` arguments returning `Int`. Adding it here also means the "did you mean"
engine will suggest `clamp` when you mistype a related name.

### Step 2: Implement it in the tree-walking interpreter

**File:** `crates/aether-eval/src/builtins.rs`

Find the large `match` on function name in the `call_builtin` function (search
for `"min"` to locate the nearby entry). Add:

```rust
"clamp" => {
    let n = args[0].as_int()
        .ok_or_else(|| EvalError::TypeError("clamp: n must be Int".into()))?;
    let lo = args[1].as_int()
        .ok_or_else(|| EvalError::TypeError("clamp: lo must be Int".into()))?;
    let hi = args[2].as_int()
        .ok_or_else(|| EvalError::TypeError("clamp: hi must be Int".into()))?;
    let result = n.clamp(lo, hi);
    let prov = ProvChain::extend(
        arena.clone(),
        ProvOp::Call("clamp".into()),
        span,
        args.iter().map(|a| a.prov().head).collect(),
    );
    Ok(Value::Int(result, prov))
}
```

The `ProvChain::extend` call is important: it links the result's provenance to
all three input values. Skipping this is correct but lossy — a user who calls
`provenance(clamp(n, 0, 100))` will get a broken chain.

### Step 3: Add a BuiltinId for the bytecode VM

**File:** `crates/aether-bc/src/op.rs`

Add a new variant to `BuiltinId`:

```rust
// op.rs:187
pub enum BuiltinId {
    Print = 0,
    Str = 1,
    Int = 2,
    Len = 3,
    Abs = 4,
    Max = 5,
    Min = 6,
    Clamp = 7,   // NEW
}
```

And add it to `BuiltinId::from_name`:

```rust
"clamp" => Some(Self::Clamp),
```

### Step 4: Implement it in the bytecode VM

**File:** `crates/aether-bc/src/builtins.rs`

Find the `match id` dispatch (search for `BuiltinId::Min`). Add:

```rust
BuiltinId::Clamp => {
    let hi = args[0].as_int()?;
    let lo = args[1].as_int()?;   // note: args are popped in reverse push order
    let n  = args[2].as_int()?;
    Ok(BcValue::Int(n.clamp(lo, hi)))
}
```

(Verify the argument order matches how the BC compiler pushes arguments — check
`compile_expr` for `Call` to confirm args are pushed left-to-right and popped
right-to-left.)

### Step 5: Wire the compiler to emit CallBuiltin

**File:** `crates/aether-bc/src/compile.rs`

In `compile_expr`, when compiling a `Call`, the compiler already checks
`BuiltinId::from_name` and emits `Op::CallBuiltin`. Since you added `Clamp` to
`from_name` in Step 3, this is automatic — no change needed here. Verify by
searching for `BuiltinId::from_name` in `compile.rs` and confirming the logic
handles your new entry.

### Step 6: Write a test in aether-cli/tests/e2e.rs

**File:** `crates/aether-cli/tests/e2e.rs`

```rust
#[test]
fn clamp_builtin() {
    let src = r#"fn main() -> Int effects {} { clamp(5, 0, 3) }"#;
    let result = run_source(src);
    assert_eq!(result, "3");
}
```

Run `cargo test -p aether-cli clamp_builtin`. Then also run the differential
test suite to confirm the tree-walker and VM agree:
`cargo test -p aether-difftest`.

### Summary of files touched

| File | Change |
|------|--------|
| `crates/aether-types/src/builtins.rs` | Register type signature |
| `crates/aether-eval/src/builtins.rs` | Tree-walker implementation |
| `crates/aether-bc/src/op.rs` | Add `BuiltinId::Clamp` |
| `crates/aether-bc/src/builtins.rs` | VM implementation |
| `crates/aether-cli/tests/e2e.rs` | End-to-end test |

The type declaration file (`aether-ast`) and the parser are untouched — `clamp`
is a function call, not a new keyword. If you wanted a new keyword instead, you
would also add a `Tok::Clamp` to the lexer, handle it in the parser, add a new
`Expr` variant (or re-use `Call` with a sentinel), update `check_expr`, update
both runtimes, and update the pretty-printer.

---

## Honest assessment of the implementation

**What is elegant:**

- The single AST shared across all passes is the right call. There is no
  "lowering" step; the interpreter and VM both read the same `Module` the parser
  produced, which eliminates a whole class of deserialization bugs.
- The Pratt parser is admirably compact. The full expression grammar is expressed
  in a single `match` table of binding powers (`lib.rs:787`) rather than a
  grammar file and a generated parser. It is easy to extend.
- The FM solver is self-contained (~400 lines including tests and witness
  generation) and takes zero external dependencies. The escalation to Z3 is a
  genuine optional layer, not a hard requirement.
- The `ProvArena` design — a shared bump allocator with cheaply-cloneable
  handles — threads provenance through the interpreter with minimal overhead.
- Differential testing between the tree-walker and bytecode VM is an unusually
  rigorous consistency guarantee for a language this young.

**What is rough:**

- The type checker is not full HM. Explicit type annotations are required on all
  function parameters and all `let` bindings without `ty`. There is no
  unification, no occurs-check, and inference variables are never generalised.
  `Type::Var` is overloaded between "a genuine unknown" and "a generic that was
  not pinned by any argument" — this causes false acceptances in some edge cases.
- `check_expr` is a 600-line `match` with mutable accumulation into a `Scope`
  that it also borrows immutably in places. The borrow checker fights are worked
  around with `scope.clone()` on every branch split — correct but O(depth) in
  memory.
- The bytecode VM's argument-popping order (right-to-left because the last-pushed
  argument is on top of the stack) is undocumented in most call sites; it has
  tripped implementors of new builtins.
- `aether-eval/src/builtins.rs` is 198 KB — nearly the entire interpreter's
  allocated weight. Every stdlib function is a big `match` arm. Splitting this
  into smaller files would make the codebase more navigable.
- `lib.rs.bak` in `crates/aether-parser/src/` (181 KB, not built) should be
  cleaned up.

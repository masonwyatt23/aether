# py-refinements

> **Proof-of-concept — not a production tool.**
>
> This experiment ports one idea from the Aether language — refinement
> contracts — into plain Python.  It is deliberately small and clearly
> labelled.  A reader must never mistake it for a production-grade checker.

## What this demonstrates

Aether lets you write functions whose postconditions the *compiler proves* at
build time:

```aether
fn pos_abs(n: Int) -> Int where result >= 0 effects {} {
  if n >= 0 then n else 0 - n
}
```

The solver case-splits on the `if` branches and discharges `result >= 0` for
each arm via linear-arithmetic reasoning — without ever running the code.

This experiment asks: **what does that idea look like if you bring it to Python,
where people already work?**

## Concept mapping

| Aether concept                              | Python (this module)                            |
|---------------------------------------------|-------------------------------------------------|
| `fn f(x: Int{x: x > 0}) -> ...`            | `@refine(requires=lambda x: x > 0)`             |
| `-> Int where result >= 0`                  | `@refine(ensures=lambda result, ...: result >= 0)` |
| Compiler proves via solver (compile-time)   | `check_static(f)` — *sketch only, see below*    |
| Runtime evaluation of the contract          | Decorator wraps every call — **this works**     |

## What genuinely works — runtime mode

The `@refine` decorator wraps a function so that every call checks:

1. **`requires`** — the precondition, called with the same arguments.  Raises
   `PreconditionError` (a subclass of `RefinementError`) if false.
2. **`ensures`** — the postcondition, called with `(result, *args, **kwargs)`.
   Raises `PostconditionError` if false.

```python
from refine import refine, RefinementError

@refine(
    requires=lambda x, lo, hi: hi >= lo,          # mirrors Aether parameter refinement
    ensures=lambda result, x, lo, hi: lo <= result <= hi,  # mirrors `where` clause
)
def clamp(x: int, lo: int, hi: int) -> int:
    if x < lo: return lo
    if x > hi: return hi
    return x

clamp(50, 0, 100)   # 50  — passes both contracts
clamp(5, 100, 0)    # raises PreconditionError: hi < lo
```

Contract violations produce clear, actionable error messages naming the
function and the argument values involved.

## What is a sketch — static mode

`check_static(fn)` attempts a compile-time-style proof.  It covers *only* this
tiny fragment:

- The postcondition is a single comparison of `result` against an integer
  literal: `result >= 0`, `result < 10`, etc.
- The function body is a single `return <integer-constant>`.

```python
@refine(ensures=lambda result: result >= 0)
def const_five():
    return 5

check_static(const_five)   # StaticResult.PROVED

@refine(ensures=lambda result: result >= 0)
def const_neg():
    return -3

check_static(const_neg)    # StaticResult.VIOLATED
```

For everything else — any `if`, any multi-statement body, any postcondition
involving multiple variables — the checker returns `StaticResult.UNKNOWN`
rather than guessing.  **Honest UNKNOWN is the correct answer here.**

`clamp`, `minimum`, `pos_abs`, and every real-world function you'd care about
will return UNKNOWN.  That is intentional and correct.

## To make this real you would need

This is an honest gap list:

1. **Full AST analysis** — traverse all control-flow paths (if/else, loops,
   try/except) in the function body, not just single-return stubs.
2. **An SMT backend** — e.g. Z3 (via `z3-solver`) or a built-in Farkas/FM
   elimination pass for linear arithmetic.  Aether's solver uses Fourier–
   Motzkin elimination capped at 64 iterations per clause.
3. **Path-sensitive reasoning** — case-split on `if` conditions (exactly what
   Aether does for `pos_abs` and `clamp_to`) and discharge each arm
   independently.
4. **Precondition-to-postcondition flow** — when a `requires` establishes
   `x > 0`, that fact must be available to the postcondition proof.
5. **Symbolic parameter handling** — postconditions like `result <= a` involve
   symbolic variables, not constants; the solver must reason about them as
   unknowns, not evaluate them.
6. **Type inference** — knowing that a variable is `Int` vs `Float` vs
   `Optional[int]` matters for what arithmetic is valid.

Items 2–5 together are roughly what Aether's
`crates/aether-types/src/refine.rs` implements.  Replicating that faithfully
in Python is a multi-month engineering effort, well outside the scope of this
experiment.

## Files

| File              | Purpose                                         |
|-------------------|-------------------------------------------------|
| `refine.py`       | The `@refine` decorator and `check_static`      |
| `examples.py`     | 6 worked examples; run directly to see output   |
| `test_refine.py`  | `unittest` test suite (29 tests)                |

## Running

```sh
# Examples (passing + failing contracts):
python3 experiments/py-refinements/examples.py

# Tests:
python3 -m unittest discover experiments/py-refinements
```

## Relationship to Aether

Aether's refinement system is described in `spec/REFINEMENTS.md` and
`spec/LANGUAGE.md` §5.  The canonical examples are `examples/02_refinement.ae`
and `examples/12_refinement_proven.ae`.  This experiment is a separate,
illustrative port — it does not modify or replace anything in the Aether
compiler itself.

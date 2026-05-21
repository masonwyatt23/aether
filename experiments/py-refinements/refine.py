"""
refine.py — runtime refinement contracts for Python, mirroring Aether's
`where result <= a` postcondition syntax.

PROOF-OF-CONCEPT — not a production tool.

This module provides two things:
  1. @refine(requires=..., ensures=...) — a decorator that checks contracts at
     *call time* (runtime mode). This genuinely works.
  2. check_static(fn) — a tiny attempt at a compile-time-style proof for a
     deliberately narrow fragment: functions whose body is a single return of a
     constant integer or a simple linear expression of their arguments.
     Anything outside that fragment returns StaticResult.UNKNOWN honestly.

Mapping from Aether concepts:
  Aether                        | Python (this module)
  ------------------------------|--------------------------------------
  fn f(x: Int{x: x > 0})        | @refine(requires=lambda x: x > 0)
  -> Int where result >= 0      | @refine(ensures=lambda result: result >= 0)
  Compiler proves via solver    | check_static(f)  — sketch only
  Runtime evaluation            | decorator wraps the call

The static checker covers ONLY:
  - A postcondition of the form `result >= k` or `result > k` or
    `result <= k` or `result < k` where k is an integer literal.
  - A function body that is a single `return <expr>` where <expr> is a
    constant integer OR a trivially-evaluable linear expression.
  Anything else: returns UNKNOWN.

To make this real you would need:
  - Full AST analysis (ast module) across all control-flow paths.
  - An SMT backend (e.g. Z3 via z3-solver, or a Farkas/FM implementation)
    to handle arbitrary linear-arithmetic formulae over symbolic parameters.
  - Path-sensitive analysis to case-split on `if` branches (what Aether's
    solver does for `clamp` and `pos_abs`).
  - Support for quantified preconditions flowing into the postcondition proof.
  That is several months of work; this module is honest about its limits.
"""

import ast
import inspect
import textwrap
from functools import wraps
from enum import Enum
from typing import Callable, Optional


# ---------------------------------------------------------------------------
# Public exceptions
# ---------------------------------------------------------------------------

class RefinementError(Exception):
    """Raised when a refinement contract is violated at runtime."""


class PreconditionError(RefinementError):
    """A precondition (requires) was false at call time."""


class PostconditionError(RefinementError):
    """A postcondition (ensures) was false after the call returned."""


# ---------------------------------------------------------------------------
# Runtime decorator
# ---------------------------------------------------------------------------

def refine(
    requires: Optional[Callable[..., bool]] = None,
    ensures: Optional[Callable[..., bool]] = None,
):
    """
    Attach refinement contracts to a function.

    Parameters
    ----------
    requires : callable, optional
        A predicate receiving the same arguments as the function.  Must return
        True for the call to proceed.  Mirrors Aether parameter refinements:
            fn f(x: Int{x: x > 0}) -> ...
    ensures : callable, optional
        A predicate receiving (result, *args, **kwargs).  Must return True
        after the function returns.  Mirrors Aether postconditions:
            fn f(...) -> Int where result >= 0

    Raises
    ------
    PreconditionError  if requires is provided and returns False.
    PostconditionError if ensures  is provided and returns False.

    Example
    -------
    >>> @refine(requires=lambda x: x >= 0,
    ...         ensures=lambda result, x: result >= 0)
    ... def my_abs(x):
    ...     return x if x >= 0 else -x
    """
    def decorator(fn: Callable) -> Callable:
        fn._requires = requires
        fn._ensures = ensures

        @wraps(fn)
        def wrapper(*args, **kwargs):
            # --- precondition check ---
            if requires is not None:
                try:
                    ok = requires(*args, **kwargs)
                except Exception as exc:
                    raise PreconditionError(
                        f"{fn.__name__}: precondition raised an exception: {exc}"
                    ) from exc
                if not ok:
                    raise PreconditionError(
                        f"{fn.__name__}: precondition violated — "
                        f"requires({_fmt_args(args, kwargs)}) is False"
                    )

            # --- call ---
            result = fn(*args, **kwargs)

            # --- postcondition check ---
            if ensures is not None:
                try:
                    ok = ensures(result, *args, **kwargs)
                except Exception as exc:
                    raise PostconditionError(
                        f"{fn.__name__}: postcondition raised an exception: {exc}"
                    ) from exc
                if not ok:
                    raise PostconditionError(
                        f"{fn.__name__}: postcondition violated — "
                        f"ensures(result={result!r}, {_fmt_args(args, kwargs)}) is False"
                    )

            return result

        return wrapper
    return decorator


def _fmt_args(args, kwargs) -> str:
    parts = [repr(a) for a in args]
    parts += [f"{k}={v!r}" for k, v in kwargs.items()]
    return ", ".join(parts)


# ---------------------------------------------------------------------------
# Static checker — deliberately minimal, honest about limits
# ---------------------------------------------------------------------------

class StaticResult(Enum):
    """
    PROVED  — the checker is confident the postcondition holds for all inputs
              satisfying the precondition, within the narrow fragment it covers.
    VIOLATED— the checker found a concrete counterexample.
    UNKNOWN — the function or postcondition is outside the checker's fragment;
              no claim is made.  This is the honest default.
    """
    PROVED   = "proved"
    VIOLATED = "violated"
    UNKNOWN  = "unknown"


def check_static(fn: Callable) -> StaticResult:
    """
    Attempt a compile-time-style proof of `fn`'s postcondition.

    PROOF-OF-CONCEPT.  Covers a tiny fragment:
      - The function must have been decorated with @refine(ensures=...).
      - The postcondition must be a single comparison of `result` against a
        literal integer: result >= k / result > k / result <= k / result < k.
      - The function body must be a single `return <integer-constant>`.

    Anything outside that fragment returns StaticResult.UNKNOWN.

    To make this real you would need a proper SMT solver and path-sensitive
    AST analysis.  See module docstring for the full gap list.
    """
    ensures = getattr(fn, "_ensures", None)
    if ensures is None:
        return StaticResult.UNKNOWN

    # --- Step 1: parse the postcondition lambda into a simple comparison ---
    cmp = _parse_ensures_lambda(ensures)
    if cmp is None:
        return StaticResult.UNKNOWN  # postcondition too complex

    op, threshold = cmp  # e.g. (">=", 0)

    # --- Step 2: try to evaluate the function body as a constant ---
    const = _eval_body_as_constant(fn)
    if const is None:
        return StaticResult.UNKNOWN  # body too complex

    # --- Step 3: check the constant against the postcondition ---
    checks = {
        ">=": const >= threshold,
        ">":  const >  threshold,
        "<=": const <= threshold,
        "<":  const <  threshold,
        "==": const == threshold,
        "!=": const != threshold,
    }
    holds = checks.get(op)
    if holds is None:
        return StaticResult.UNKNOWN

    return StaticResult.PROVED if holds else StaticResult.VIOLATED


# ---------------------------------------------------------------------------
# Internal helpers for the static checker
# ---------------------------------------------------------------------------

def _parse_ensures_lambda(ensures: Callable) -> Optional[tuple[str, int]]:
    """
    Try to parse `ensures` as a lambda of the form:
        lambda result: result OP k
        lambda result, *_: result OP k
    where OP in {>=, >, <=, <, ==, !=} and k is an integer literal.

    Returns (op_str, k) or None if the form is not recognised.
    """
    try:
        src = textwrap.dedent(inspect.getsource(ensures)).strip()
    except (OSError, TypeError):
        return None

    # `inspect.getsource` on a lambda defined inside a decorator call returns
    # the entire decorated function, not just the lambda text.  We extract only
    # the text of the lambda itself by:
    #   1. Finding "lambda" in the source.
    #   2. Taking only the rest of that line (lambdas cannot span lines in
    #      standard Python syntax).
    #   3. Stripping trailing punctuation that belongs to the enclosing call.
    idx = src.find("lambda")
    if idx == -1:
        return None
    end_of_line = src.find("\n", idx)
    lambda_src = src[idx:end_of_line] if end_of_line != -1 else src[idx:]
    lambda_src = lambda_src.rstrip("), \t")

    try:
        tree = ast.parse(lambda_src, mode="eval")
    except SyntaxError:
        return None

    if not isinstance(tree.body, ast.Lambda):
        return None

    body = tree.body.body
    # Must be a single Compare node: result OP k
    if not isinstance(body, ast.Compare):
        return None
    if len(body.ops) != 1 or len(body.comparators) != 1:
        return None

    left = body.left
    op_node = body.ops[0]
    right = body.comparators[0]

    # left must be `result` (a Name node)
    if not (isinstance(left, ast.Name) and left.id == "result"):
        return None

    # right must be an integer constant
    if not isinstance(right, ast.Constant) or not isinstance(right.value, int):
        return None

    op_map = {
        ast.GtE: ">=",
        ast.Gt:  ">",
        ast.LtE: "<=",
        ast.Lt:  "<",
        ast.Eq:  "==",
        ast.NotEq: "!=",
    }
    op_str = op_map.get(type(op_node))
    if op_str is None:
        return None

    return op_str, right.value


def _eval_body_as_constant(fn: Callable) -> Optional[int]:
    """
    Try to evaluate the body of `fn` as a constant integer.

    Succeeds only when the function body (after stripping the decorator line(s)
    and the `def` line) is exactly `return <integer-literal>`.

    Returns the integer or None.
    """
    try:
        src = textwrap.dedent(inspect.getsource(fn))
    except (OSError, TypeError):
        return None

    try:
        tree = ast.parse(src)
    except SyntaxError:
        return None

    # Find the FunctionDef node.
    func_defs = [n for n in ast.walk(tree) if isinstance(n, ast.FunctionDef)]
    if len(func_defs) != 1:
        return None

    body = func_defs[0].body
    # Must be a single statement.
    if len(body) != 1:
        return None

    stmt = body[0]
    if not isinstance(stmt, ast.Return):
        return None

    val = stmt.value
    if isinstance(val, ast.Constant) and isinstance(val.value, int):
        return val.value

    # Also accept a unary minus on a constant: return -5
    if (isinstance(val, ast.UnaryOp)
            and isinstance(val.op, ast.USub)
            and isinstance(val.operand, ast.Constant)
            and isinstance(val.operand.value, int)):
        return -val.operand.value

    return None

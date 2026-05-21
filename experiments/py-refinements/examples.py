"""
examples.py — worked examples of Aether-style refinement contracts in Python.

PROOF-OF-CONCEPT — not a production tool.

Each example shows the Aether original (as a comment) and the Python equivalent
using the @refine decorator from refine.py.  The script ends by deliberately
triggering contract violations and catching the resulting RefinementError so
the output demonstrates both the passing and failing paths.
"""

from refine import refine, PreconditionError, PostconditionError, RefinementError


# ---------------------------------------------------------------------------
# Example 1: min — mirrors examples/02_refinement.ae
#
# Aether:
#   fn min(a: Int, b: Int) -> Int
#     where result <= a && result <= b
#     effects {} { if a <= b then a else b }
# ---------------------------------------------------------------------------

@refine(
    ensures=lambda result, a, b: result <= a and result <= b,
)
def minimum(a: int, b: int) -> int:
    return a if a <= b else b


# ---------------------------------------------------------------------------
# Example 2: pos_abs — mirrors examples/12_refinement_proven.ae
#
# Aether:
#   fn pos_abs(n: Int) -> Int where result >= 0 effects {} {
#     if n >= 0 then n else 0 - n
#   }
# ---------------------------------------------------------------------------

@refine(
    ensures=lambda result, n: result >= 0,
)
def pos_abs(n: int) -> int:
    return n if n >= 0 else -n


# ---------------------------------------------------------------------------
# Example 3: clamp — mirrors examples/12_refinement_proven.ae
#
# Aether:
#   fn clamp_to(x: Int, lo: Int, hi: Int{h: h >= lo}) -> Int
#     where result >= lo && result <= hi
#     effects {} { ... }
# ---------------------------------------------------------------------------

@refine(
    requires=lambda x, lo, hi: hi >= lo,
    ensures=lambda result, x, lo, hi: lo <= result <= hi,
)
def clamp(x: int, lo: int, hi: int) -> int:
    if x < lo:
        return lo
    if x > hi:
        return hi
    return x


# ---------------------------------------------------------------------------
# Example 4: safe_div — precondition prevents division by zero
#
# Aether (hypothetical):
#   fn safe_div(a: Int, b: Int{x: x != 0}) -> Int effects {} { a / b }
# ---------------------------------------------------------------------------

@refine(
    requires=lambda a, b: b != 0,
)
def safe_div(a: int, b: int) -> int:
    return a // b


# ---------------------------------------------------------------------------
# Example 5: next_pos — mirrors examples/12_refinement_proven.ae
#
# Aether:
#   fn next_pos(n: Int{x: x > 0}) -> Int where result > 0 effects {} { n + 1 }
# ---------------------------------------------------------------------------

@refine(
    requires=lambda n: n > 0,
    ensures=lambda result, n: result > 0,
)
def next_pos(n: int) -> int:
    return n + 1


# ---------------------------------------------------------------------------
# Example 6: at_least_one — identity with a range contract
#
# Aether:
#   fn at_least_one(n: Int{x: x >= 1}) -> Int where result >= 1 { n }
# ---------------------------------------------------------------------------

@refine(
    requires=lambda n: n >= 1,
    ensures=lambda result, n: result >= 1,
)
def at_least_one(n: int) -> int:
    return n


# ---------------------------------------------------------------------------
# Demo runner
# ---------------------------------------------------------------------------

def run():
    separator = "-" * 55

    print("Aether refinement contracts — Python proof-of-concept")
    print(separator)

    # --- passing cases ---
    print("\nPassing contracts:")
    print(f"  minimum(3, 7)          = {minimum(3, 7)}")
    print(f"  pos_abs(-5)            = {pos_abs(-5)}")
    print(f"  pos_abs(5)             = {pos_abs(5)}")
    print(f"  clamp(50, 0, 100)      = {clamp(50, 0, 100)}")
    print(f"  clamp(-10, 0, 100)     = {clamp(-10, 0, 100)}")
    print(f"  clamp(200, 0, 100)     = {clamp(200, 0, 100)}")
    print(f"  safe_div(10, 3)        = {safe_div(10, 3)}")
    print(f"  next_pos(7)            = {next_pos(7)}")
    print(f"  at_least_one(1)        = {at_least_one(1)}")

    # --- deliberately failing cases ---
    print("\nDeliberately failing contracts (each caught as RefinementError):")

    # Violate safe_div precondition: b == 0
    try:
        safe_div(10, 0)
    except RefinementError as e:
        print(f"  [precondition] {e}")

    # Violate clamp precondition: hi < lo
    try:
        clamp(5, 100, 0)
    except RefinementError as e:
        print(f"  [precondition] {e}")

    # Violate next_pos precondition: n <= 0
    try:
        next_pos(0)
    except RefinementError as e:
        print(f"  [precondition] {e}")

    # Violate at_least_one postcondition via a patched version
    # (simulate a buggy implementation to show postcondition checking)
    @refine(
        requires=lambda n: n >= 1,
        ensures=lambda result, n: result >= 1,
    )
    def buggy_at_least_one(n: int) -> int:
        return n - 2  # bug: always subtracts 2, violating result >= 1

    try:
        buggy_at_least_one(1)
    except RefinementError as e:
        print(f"  [postcondition] {e}")

    # Violate minimum postcondition with a buggy version
    @refine(
        ensures=lambda result, a, b: result <= a and result <= b,
    )
    def buggy_minimum(a: int, b: int) -> int:
        return a + b  # clearly wrong

    try:
        buggy_minimum(3, 7)
    except RefinementError as e:
        print(f"  [postcondition] {e}")

    print(f"\n{separator}")
    print("All done.  Runtime contract checking works as shown above.")
    print("See README.md for what the static checker can and cannot do.")


if __name__ == "__main__":
    run()

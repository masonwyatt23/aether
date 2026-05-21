"""
test_refine.py — stdlib unittest tests for refine.py

Run with:
    python3 -m unittest experiments/py-refinements/test_refine.py
or from the repo root:
    python3 -m unittest discover experiments/py-refinements

PROOF-OF-CONCEPT — not a production tool.
"""

import sys
import os
import unittest

# Ensure refine.py is importable when run from any directory.
sys.path.insert(0, os.path.dirname(__file__))

from refine import (
    refine,
    RefinementError,
    PreconditionError,
    PostconditionError,
    check_static,
    StaticResult,
)


# ---------------------------------------------------------------------------
# Helper functions used across tests (defined at module level so inspect can
# retrieve their source for the static checker tests).
# ---------------------------------------------------------------------------

@refine(
    requires=lambda x: x >= 0,
    ensures=lambda result, x: result >= 0,
)
def my_abs(x: int) -> int:
    return x if x >= 0 else -x


@refine(
    requires=lambda a, b: b != 0,
)
def safe_div(a: int, b: int) -> int:
    return a // b


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


@refine(
    ensures=lambda result, a, b: result <= a and result <= b,
)
def minimum(a: int, b: int) -> int:
    return a if a <= b else b


# Static-checker test helpers — single-return bodies only
@refine(ensures=lambda result: result >= 0)
def const_positive():
    return 5


@refine(ensures=lambda result: result >= 0)
def const_zero():
    return 0


@refine(ensures=lambda result: result >= 0)
def const_negative():
    return -3  # postcondition is false → VIOLATED


@refine(ensures=lambda result: result >= 0)
def non_trivial_body(x: int) -> int:
    # Multi-statement body → UNKNOWN
    y = x + 1
    return y


@refine(ensures=lambda result, a, b: result <= a and result <= b)
def complex_ensures(a, b):
    return 0  # ensures is multi-term → UNKNOWN


# ---------------------------------------------------------------------------
# Test cases
# ---------------------------------------------------------------------------

class TestRuntimePrecondition(unittest.TestCase):
    """Precondition (requires) is checked before the call."""

    def test_satisfied_precondition_passes(self):
        self.assertEqual(safe_div(10, 2), 5)

    def test_zero_divisor_raises_precondition_error(self):
        with self.assertRaises(PreconditionError):
            safe_div(10, 0)

    def test_clamp_valid_range_passes(self):
        self.assertEqual(clamp(5, 0, 10), 5)
        self.assertEqual(clamp(-5, 0, 10), 0)
        self.assertEqual(clamp(15, 0, 10), 10)

    def test_clamp_inverted_range_raises(self):
        with self.assertRaises(PreconditionError):
            clamp(5, 100, 0)

    def test_precondition_error_is_refinement_error(self):
        """PreconditionError must be a subclass of RefinementError."""
        with self.assertRaises(RefinementError):
            safe_div(1, 0)

    def test_negative_input_to_my_abs_raises(self):
        # requires x >= 0 on my_abs
        with self.assertRaises(PreconditionError):
            my_abs(-1)


class TestRuntimePostcondition(unittest.TestCase):
    """Postcondition (ensures) is checked after the call returns."""

    def test_satisfied_postcondition_passes(self):
        self.assertLessEqual(minimum(3, 7), 3)
        self.assertLessEqual(minimum(3, 7), 7)

    def test_violated_postcondition_raises(self):
        @refine(ensures=lambda result, a, b: result <= a and result <= b)
        def buggy_min(a: int, b: int) -> int:
            return a + b  # always wrong

        with self.assertRaises(PostconditionError):
            buggy_min(3, 7)

    def test_postcondition_error_is_refinement_error(self):
        @refine(ensures=lambda result, x: result >= 0)
        def always_negative(x: int) -> int:
            return -abs(x) - 1

        with self.assertRaises(RefinementError):
            always_negative(5)

    def test_my_abs_positive_input(self):
        self.assertEqual(my_abs(0), 0)
        self.assertEqual(my_abs(5), 5)

    def test_violated_postcondition_message_contains_function_name(self):
        @refine(ensures=lambda result, x: result > 100)
        def small_value(x: int) -> int:
            return x

        try:
            small_value(1)
            self.fail("Expected PostconditionError")
        except PostconditionError as exc:
            self.assertIn("small_value", str(exc))


class TestReturnValuePreserved(unittest.TestCase):
    """The decorator must pass the return value through unchanged."""

    def test_clamp_returns_correct_value(self):
        self.assertEqual(clamp(50, 0, 100), 50)
        self.assertEqual(clamp(-1,  0, 100), 0)
        self.assertEqual(clamp(200, 0, 100), 100)

    def test_minimum_returns_correct_value(self):
        self.assertEqual(minimum(3, 7), 3)
        self.assertEqual(minimum(7, 3), 3)
        self.assertEqual(minimum(5, 5), 5)

    def test_safe_div_returns_correct_value(self):
        self.assertEqual(safe_div(10, 3), 3)
        self.assertEqual(safe_div(9, 3), 3)


class TestStaticChecker(unittest.TestCase):
    """
    The static checker is a sketch.  These tests verify that:
      - it accepts trivially-provable postconditions,
      - it detects trivially-false postconditions,
      - it is honest (returns UNKNOWN) for anything outside its tiny fragment.
    """

    def test_constant_positive_is_proved(self):
        """A function returning 5 satisfies result >= 0: PROVED."""
        result = check_static(const_positive)
        self.assertEqual(result, StaticResult.PROVED)

    def test_constant_zero_satisfies_ge_zero(self):
        """A function returning 0 satisfies result >= 0: PROVED."""
        result = check_static(const_zero)
        self.assertEqual(result, StaticResult.PROVED)

    def test_constant_negative_is_violated(self):
        """A function returning -3 violates result >= 0: VIOLATED."""
        result = check_static(const_negative)
        self.assertEqual(result, StaticResult.VIOLATED)

    def test_multi_statement_body_is_unknown(self):
        """Body with multiple statements is outside the checker's fragment."""
        result = check_static(non_trivial_body)
        self.assertEqual(result, StaticResult.UNKNOWN)

    def test_complex_ensures_is_unknown(self):
        """Multi-term postcondition is outside the checker's fragment."""
        result = check_static(complex_ensures)
        self.assertEqual(result, StaticResult.UNKNOWN)

    def test_no_ensures_is_unknown(self):
        """A plain function with no @refine contract is UNKNOWN."""
        def plain(x):
            return x
        result = check_static(plain)
        self.assertEqual(result, StaticResult.UNKNOWN)

    def test_static_checker_never_claims_proved_falsely(self):
        """
        The checker must never return PROVED for a function whose body
        clearly violates the postcondition.  VIOLATED or UNKNOWN are both
        acceptable; PROVED is not.
        """
        result = check_static(const_negative)
        self.assertNotEqual(result, StaticResult.PROVED)

    def test_static_result_is_unknown_for_clamp(self):
        """clamp has a multi-arm body; checker must return UNKNOWN."""
        result = check_static(clamp)
        self.assertEqual(result, StaticResult.UNKNOWN)

    def test_static_result_is_unknown_for_minimum(self):
        """minimum has a conditional; checker must return UNKNOWN."""
        result = check_static(minimum)
        self.assertEqual(result, StaticResult.UNKNOWN)


class TestDecoratorTransparency(unittest.TestCase):
    """@refine must preserve function metadata."""

    def test_function_name_preserved(self):
        self.assertEqual(safe_div.__name__, "safe_div")

    def test_function_doc_preserved(self):
        @refine(requires=lambda x: x > 0)
        def documented(x):
            """A documented function."""
            return x
        self.assertIn("documented", documented.__doc__)

    def test_requires_stored_on_function(self):
        self.assertIsNotNone(safe_div._requires)

    def test_ensures_stored_on_function(self):
        self.assertIsNotNone(my_abs._ensures)


if __name__ == "__main__":
    unittest.main()

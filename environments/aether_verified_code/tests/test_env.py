"""Tests for the aether-verified-code RL environment.

`aether_core` is framework-free, so most tests run without `verifiers` or the
`aether` compiler installed. Tests that need the compiler are skipped when it
is absent; the one test that needs `verifiers` is skipped when it is absent.

Run from the environment directory:  python3 -m pytest
"""

import os
import shutil
import sys

import pytest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

import aether_core as core  # noqa: E402

HAVE_AETHER = bool(os.environ.get("AETHER_BIN")) or shutil.which("aether")
INSTANCE_KEYS = {"family", "tier", "title", "intro", "spec",
                 "signature", "reference", "seed"}


def test_import():
    """The core and the verifiers adapter import without error."""
    assert core.CANARY.startswith("AETHER-VERIFIED-CODE-CANARY-")
    # The adapter imports only if `verifiers` is installed.
    try:
        import aether_verified_code  # noqa: F401
    except ImportError as e:
        if "verifiers" not in str(e) and "datasets" not in str(e):
            raise


def test_templates_complete():
    """Every template family produces a well-formed instance."""
    for family in core.FAMILIES:
        assert family in core.FAMILY_TIERS, f"{family} missing from FAMILY_TIERS"
        rng = __import__("random").Random(family)
        inst = core.generate(family, rng)
        assert INSTANCE_KEYS - {"seed"} <= set(inst), family
        assert inst["tier"] in core.TIER_ORDER, inst["tier"]


def test_new_task_deterministic():
    """A seed maps deterministically to the same task."""
    a, b = core.new_task(7), core.new_task(7)
    assert a == b


def test_new_task_tier_range():
    """Tier-windowed generation always lands in range."""
    for seed in range(40):
        assert core.new_task(seed, "easy", "easy")["tier"] == "easy"
        assert core.new_task(seed, "expert", "expert")["tier"] == "expert"
        t = core.new_task(seed, "medium", "hard")["tier"]
        assert t in {"medium", "medium-hard", "hard"}


def test_classify():
    """The outcome classifier maps diagnostics onto the ladder."""
    assert core.classify(0, 0, []) == "VERIFIED"
    assert core.classify(1, 0, [{"message": "parse error: x"}]) == "PARSE-ERROR"
    assert core.classify(1, 0, [{"message": "undeclared effect IO"}]) == "EFFECT-ERROR"
    assert core.classify(0, 1, [{"message": "postcondition could not be verified"}]) == "UNPROVEN"
    assert core.classify(1, 0, [{"message": "type mismatch"}]) == "TYPE-ERROR"


def test_reward_ladder(monkeypatch):
    """The dense reward respects the graded ladder and the partial-credit cap."""
    task = core.new_task(0)
    for outcome, lo, hi in [("PARSE-ERROR", 0.0, 0.0), ("TYPE-ERROR", 0.04, 0.04),
                            ("EFFECT-ERROR", 0.08, 0.08), ("VERIFIED", 1.0, 1.0)]:
        monkeypatch.setattr(core, "check", lambda *a, **k: {
            "outcome": outcome, "errors": 0, "warnings": 0,
            "diagnostics": [], "feedback": ""})
        r = core.score(task, "fn f()->Int effects {} { 0 }")["reward"]
        assert lo <= r <= hi, (outcome, r)
    # UNPROVEN floor, and the partial-credit cap.
    monkeypatch.setattr(core, "check", lambda *a, **k: {
        "outcome": "UNPROVEN", "errors": 0, "warnings": 1,
        "diagnostics": [], "feedback": ""})
    monkeypatch.setattr(core, "partial_credit", lambda *a, **k: 1.0)
    r = core.score(task, "x")["reward"]
    assert r == core.PARTIAL_CAP, r


def test_split_and():
    """Top-level `&&` splitting respects parentheses."""
    assert core._split_and("a && b && c") == ["a", "b", "c"]
    assert core._split_and("(x && y) || z") == ["(x && y) || z"]
    assert core._split_and("result >= 0") == ["result >= 0"]


@pytest.mark.skipif(not HAVE_AETHER, reason="aether compiler not on PATH")
def test_score_reference():
    """A task's reference verifies (reward 1.0); its stub does not."""
    task = core.new_task(3)
    assert core.score(task, task["reference"])["solved"] is True
    assert core.score(task, task["signature"])["solved"] is False


@pytest.mark.skipif(not HAVE_AETHER, reason="aether compiler not on PATH")
def test_canary_not_in_tasks():
    """The canary must not leak into generated task text."""
    for seed in range(20):
        t = core.new_task(seed)
        assert core.CANARY not in t["signature"] + t["reference"]


def test_load_environment():
    """`load_environment` builds a verifiers Environment (skipped without it)."""
    try:
        import aether_verified_code
    except ImportError:
        pytest.skip("verifiers/datasets not installed")
    env = aether_verified_code.load_environment(num_tasks=3, max_turns=2,
                                                tiers=["easy"])
    assert len(env.dataset) == 3
    for row in env.dataset:
        assert {"prompt", "answer", "info"} <= set(row)
        assert row["info"]["tier"] == "easy"

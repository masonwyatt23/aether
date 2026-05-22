#!/usr/bin/env python3
"""Aether verified-code RL environment -- verifier-agnostic core.

This module turns the Aether compiler's exact refinement-type oracle into a
dense RL reward, and knows nothing about any RL framework (the `verifiers`
adapter, `aether_verified_code.py`, sits on top). It is deliberately
dependency-free (standard library only) so it can be unit-tested without the
`verifiers` package installed.

Three jobs:
  * `new_task(seed)`   -- generate a fresh task from the procedural templates.
                          A seed maps to a never-committed instance, so an RL
                          run trains on contamination-free data.
  * `check(src)`       -- run `aether check` (the exact oracle) and classify
                          the result on a graded outcome ladder.
  * `score(task,code)` -- turn a model submission into a DENSE reward in
                          [0, 1].

Why this is a sound RL signal: the reward is the compiler *proving* the
refinement contract. Unlike a test suite or an LLM judge it cannot be
reward-hacked -- and because the task contracts are mutation-audited (each
provably rejects every wrong mutant of its reference), a rewarded solution
genuinely satisfies the specification.
"""

import json
import os
import random
import re
import subprocess
import sys
import tempfile

# `templates.py` is vendored into this package, so the environment is
# self-contained and publishable to the Environments Hub.
from templates import FAMILIES, generate

# Contamination canary -- if this string surfaces in a model's training data,
# this environment's tasks have leaked. Do not remove.
CANARY = "AETHER-VERIFIED-CODE-CANARY-97713582-970b-4d21-bc29-7a90af57a652"

# --- the graded outcome ladder ---------------------------------------------
# A partially-correct submission earns partial reward, so the RL signal is
# dense rather than sparse pass/fail. Only VERIFIED -- the compiler proved the
# contract, zero errors AND zero warnings -- is a full solve.
LADDER = {
    "PARSE-ERROR": 0.00,   # does not parse
    "TYPE-ERROR": 0.04,    # parses, ill-typed
    "EFFECT-ERROR": 0.08,  # types, but uses an undeclared effect
    "UNPROVEN": 0.15,      # type/effect-clean, contract not proved (floor)
    "VERIFIED": 1.00,      # the compiler PROVED the contract -- solved
}
# An UNPROVEN submission earns its floor plus this span times the fraction of
# the contract's conjuncts it does prove -- capped so UNPROVEN < VERIFIED.
PARTIAL_SPAN = 0.45
PARTIAL_CAP = 0.60


def aether_bin() -> str:
    """Locate the `aether` compiler: `$AETHER_BIN`, else `aether` on `PATH`."""
    return os.environ.get("AETHER_BIN") or "aether"


# --- difficulty tiers (for curriculum sampling) ----------------------------
TIER_ORDER = ["easy", "medium", "medium-hard", "hard", "expert"]
TIER_RANK = {t: i for i, t in enumerate(TIER_ORDER)}

# The difficulty tier(s) each family can emit. `bucket` spans two: a 5-way
# bucket is `expert`, fewer are `hard`.
FAMILY_TIERS = {
    "minmax": {"easy"}, "abs": {"easy"},
    "sign": {"medium"},
    "clamp": {"medium-hard"}, "window": {"medium-hard"}, "adt": {"medium-hard"},
    "sat_add": {"hard"}, "sat_sub": {"hard"}, "mirror": {"hard"},
    "list_sum": {"hard"}, "compose": {"hard"}, "bucket": {"hard", "expert"},
    "recursive": {"expert"}, "recursive_range": {"expert"},
    "compose_chain": {"expert"},
}


def _tiers_in_range(min_tier: str, max_tier: str) -> set:
    lo, hi = TIER_RANK[min_tier], TIER_RANK[max_tier]
    return {t for t in TIER_ORDER if lo <= TIER_RANK[t] <= hi}


def new_task(seed: int, min_tier: str = "easy",
             max_tier: str = "expert") -> dict:
    """Generate one fresh task instance, deterministically from `seed`, whose
    realized difficulty tier falls within `[min_tier, max_tier]`.

    Returns the template instance: family, tier, title, intro, spec,
    signature (the stubbed function the model completes), reference, seed.

    Tier selection is by rejection sampling -- a family's tier can be
    data-dependent (`bucket`) -- so the realized `tier` is always verified.
    """
    allowed = _tiers_in_range(min_tier, max_tier)
    rng = random.Random(seed)
    pool = sorted(f for f in FAMILIES if FAMILY_TIERS.get(f, set()) & allowed)
    if not pool:
        raise ValueError(f"no task families in tier range "
                         f"{min_tier}..{max_tier}")
    for _ in range(64):
        inst = generate(rng.choice(pool), rng)
        if inst["tier"] in allowed:
            inst["seed"] = seed
            return inst
    # Deterministic fallback: a family guaranteed in range.
    inst = generate(pool[0], random.Random(seed))
    inst["seed"] = seed
    return inst


def build_prompt(task: dict) -> str:
    """The model-facing problem statement for a task."""
    spec = "\n".join("- " + s for s in task["spec"])
    return (
        f"# {task['title']}\n\n{task['intro']}\n\n"
        f"## Specification\n\n{spec}\n\n"
        f"## Signature (provided -- do not change the contract)\n\n"
        f"```aether\n{task['signature']}```\n\n"
        f"Replace the stub body with an implementation the Aether compiler "
        f"can prove satisfies the `where` contract. Return only the complete "
        f"Aether function(s)."
    )


def classify(errors: int, warnings: int, diagnostics: list) -> str:
    """Map an `aether check` result onto the outcome ladder."""
    if errors == 0 and warnings == 0:
        return "VERIFIED"
    msgs = " ".join(d.get("message", "").lower() for d in diagnostics)
    if "parse error" in msgs:
        return "PARSE-ERROR"
    if "effect" in msgs:
        return "EFFECT-ERROR"
    if any(k in msgs for k in ("postcondition", "counterexample", "refuted",
                               "could not be verified", "termination")):
        return "UNPROVEN"
    return "TYPE-ERROR"


def format_feedback(diagnostics: list) -> str:
    """The verifier's diagnostics as text -- fed back to the model between
    turns of the agentic loop. Refutation counterexamples are included; they
    are the richest training signal."""
    if not diagnostics:
        return "(no diagnostics)"
    return "\n".join(
        f"- [{d.get('severity')}] line {d.get('line')}: {d.get('message')}"
        for d in diagnostics)


def check(src: str, binary: str = None, timeout: int = 30) -> dict:
    """Run `aether check --json` on Aether source text.

    Returns {outcome, errors, warnings, diagnostics, feedback}. A solver
    timeout or unparseable output is treated conservatively -- never VERIFIED.
    """
    binary = binary or aether_bin()
    fd, path = tempfile.mkstemp(suffix=".ae")
    try:
        with os.fdopen(fd, "w") as f:
            f.write(src)
        proc = subprocess.run([binary, "check", "--json", path],
                              capture_output=True, text=True, timeout=timeout)
    except (subprocess.TimeoutExpired, FileNotFoundError) as exc:
        return {"outcome": "PARSE-ERROR", "errors": 1, "warnings": 0,
                "diagnostics": [], "feedback": f"harness error: {exc}"}
    finally:
        try:
            os.unlink(path)
        except OSError:
            pass
    out = proc.stdout.strip()
    start = out.find("{")
    if start < 0:
        return {"outcome": "PARSE-ERROR", "errors": 1, "warnings": 0,
                "diagnostics": [], "feedback": "no JSON from aether check"}
    try:
        r = json.loads(out[start:])
    except json.JSONDecodeError:
        return {"outcome": "PARSE-ERROR", "errors": 1, "warnings": 0,
                "diagnostics": [], "feedback": "unparseable aether check output"}
    errs, warns = int(r.get("errors", 0)), int(r.get("warnings", 0))
    diags = r.get("diagnostics", [])
    return {"outcome": classify(errs, warns, diags), "errors": errs,
            "warnings": warns, "diagnostics": diags,
            "feedback": format_feedback(diags)}


def _split_and(pred: str) -> list:
    """Split a contract predicate on top-level `&&` (at paren depth 0)."""
    parts, depth, buf, i = [], 0, "", 0
    while i < len(pred):
        c = pred[i]
        if c in "([{":
            depth += 1
        elif c in ")]}":
            depth -= 1
        if depth == 0 and pred[i:i + 2] == "&&":
            parts.append(buf.strip())
            buf, i = "", i + 2
            continue
        buf += c
        i += 1
    if buf.strip():
        parts.append(buf.strip())
    return parts


def _split_one_fn(src: str):
    """(head_before_where, contract, tail) for a single-function source, or
    None for a multi-function task (partial credit is not attempted there)."""
    if src.count("\nfn ") + int(src.lstrip().startswith("fn ")) > 1:
        return None
    m = re.search(r"^[ \t]*where (.+)$", src, re.M)
    if not m:
        return None
    return src[:m.start()], m.group(1).strip(), src[m.end():].lstrip("\n")


def partial_credit(task: dict, code: str, binary: str = None) -> float:
    """Fraction (0..1) of the contract's top-level conjuncts the submission
    proves -- so an UNPROVEN multi-clause `where` reflects 'almost right'.

    Each conjunct was mutation-audited, so proving a subset is genuine partial
    correctness, not spec-gaming. Returns 0.0 for an atomic contract or a
    multi-function task.
    """
    binary = binary or aether_bin()
    parts = _split_one_fn(task["signature"])
    sub = _split_one_fn(code)
    if parts is None or sub is None:
        return 0.0
    conjuncts = _split_and(parts[1])
    if len(conjuncts) < 2:
        return 0.0
    proved = sum(
        check(f"{sub[0]}  where {cj}\n{sub[2]}", binary)["outcome"] == "VERIFIED"
        for cj in conjuncts)
    return proved / len(conjuncts)


def score(task: dict, code: str, binary: str = None) -> dict:
    """Score a model submission -> a dense reward in [0, 1].

    VERIFIED earns the full reward. Otherwise the outcome ladder gives a
    floor, and an UNPROVEN multi-clause contract adds graded credit for the
    fraction of conjuncts proved (capped below VERIFIED). The result also
    carries `feedback` -- the verifier diagnostics -- for the next turn.
    """
    binary = binary or aether_bin()
    res = check(code, binary)
    reward = LADDER[res["outcome"]]
    frac = 0.0
    if res["outcome"] == "UNPROVEN":
        frac = partial_credit(task, code, binary)
        reward = min(PARTIAL_CAP, LADDER["UNPROVEN"] + PARTIAL_SPAN * frac)
    return {
        "outcome": res["outcome"],
        "reward": round(reward, 4),
        "solved": res["outcome"] == "VERIFIED",
        "contract_fraction": round(frac, 3),
        "errors": res["errors"],
        "warnings": res["warnings"],
        "feedback": res["feedback"],
    }


if __name__ == "__main__":
    # Smoke self-test: a task's reference must score 1.0; a degenerate body
    # (stub) must score well below it.
    b = aether_bin()
    t = new_task(int(sys.argv[1]) if len(sys.argv) > 1 else 0)
    print(f"task: {t['family']} / {t['title']}  (tier {t['tier']})")
    ref = score(t, t["reference"], b)
    print(f"  reference  -> {ref['outcome']:12} reward={ref['reward']}")
    stub = score(t, t["signature"], b)
    print(f"  stub       -> {stub['outcome']:12} reward={stub['reward']}"
          f"  contract_fraction={stub['contract_fraction']}")
    assert ref["solved"], "reference must verify"
    assert not stub["solved"], "stub must not verify"
    print("self-test OK")

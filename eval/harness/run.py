#!/usr/bin/env python3
"""Aether verified-code benchmark — scorer.

Runs every candidate solution through `aether check --json` and classifies it.
Unlike a test-pass-rate benchmark, a task only counts as solved when the
compiler *proves* its refinement contract — `errors == 0 and warnings == 0`.

Usage:
    python3 eval/harness/run.py <candidate-dir> [--json]

`<candidate-dir>` holds one `.ae` file per task, named after the task
directory (e.g. `04_clamp.ae`). `eval/baseline/` is the reference set and
should always score 100% — run it as the harness self-test:

    python3 eval/harness/run.py eval/baseline

`--json` emits the result as a single JSON object on stdout (model, score,
per-tier breakdown, per-task outcomes, z3 availability) instead of the
human-readable table — consumed by `compare.py` and CI.

Environment:
    AETHER_BIN   path to the `aether` binary (default: ./target/release/aether,
                 then `aether` on PATH).
"""

import json
import os
import shutil
import subprocess
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
# AETHER_EVAL_TASKS lets the harness score a generated instance set (see
# gen_tasks.py) instead of the static eval/tasks/ tree.
TASKS_DIR = Path(os.environ.get("AETHER_EVAL_TASKS", REPO_ROOT / "eval" / "tasks"))

# Classification outcomes, worst-to-best for reporting order.
VERIFIED = "VERIFIED"
UNPROVEN = "UNPROVEN"
EFFECT_ERROR = "EFFECT-ERROR"
TYPE_ERROR = "TYPE-ERROR"
PARSE_ERROR = "PARSE-ERROR"
MISSING = "MISSING"


def aether_bin() -> str:
    """Locate the `aether` binary."""
    explicit = os.environ.get("AETHER_BIN")
    if explicit:
        return explicit
    release = REPO_ROOT / "target" / "release" / "aether"
    if release.exists():
        return str(release)
    return "aether"  # fall back to PATH


# Difficulty tiers, easiest first (for ordered reporting).
TIER_ORDER = ["easy", "medium", "medium-hard", "hard", "expert"]


def task_difficulty(task: str) -> str:
    """Read the `**Difficulty:**` line from a task's prompt.md.

    Tasks without one — the original benchmark tier — are treated as `easy`.
    """
    prompt = TASKS_DIR / task / "prompt.md"
    try:
        text = prompt.read_text()
    except OSError:
        return "easy"
    for line in text.splitlines():
        low = line.lower()
        if "difficulty:" in low:
            tier = low.split("difficulty:", 1)[1].strip().strip("*").strip()
            return tier or "easy"
    return "easy"


def classify(diagnostics: list, errors: int, warnings: int) -> str:
    """Map a `check --json` result to a benchmark outcome.

    A task is solved only when the contract is fully discharged: no errors
    and no warnings (a warning from the refinement checker means "could not
    prove", which is not good enough for a verified benchmark).
    """
    if errors == 0 and warnings == 0:
        return VERIFIED
    msgs = " ".join(d.get("message", "").lower() for d in diagnostics)
    if "parse error" in msgs:
        return PARSE_ERROR
    if "effect" in msgs:
        return EFFECT_ERROR
    # Refinement-related failures: refuted postconditions or "could not verify".
    if any(k in msgs for k in ("postcondition", "counterexample", "could not be verified")):
        return UNPROVEN
    return TYPE_ERROR


def check_file(binary: str, path: Path) -> str:
    """Run `aether check --json` on `path` and classify the result."""
    try:
        proc = subprocess.run(
            [binary, "check", "--json", str(path)],
            capture_output=True,
            text=True,
            timeout=30,
        )
    except (subprocess.TimeoutExpired, FileNotFoundError) as exc:
        print(f"  ! could not run aether on {path}: {exc}", file=sys.stderr)
        return PARSE_ERROR
    # The JSON object is the last `{...}` block on stdout.
    out = proc.stdout.strip()
    start = out.find("{")
    if start < 0:
        return PARSE_ERROR
    try:
        result = json.loads(out[start:])
    except json.JSONDecodeError:
        return PARSE_ERROR
    return classify(
        result.get("diagnostics", []),
        int(result.get("errors", 0)),
        int(result.get("warnings", 0)),
    )


def task_list() -> list:
    """Sorted list of task directory names under eval/tasks/."""
    return sorted(p.name for p in TASKS_DIR.iterdir() if p.is_dir())


def z3_available() -> bool:
    """Whether the `z3` SMT solver is on PATH.

    Aether's core solver decides linear integer arithmetic on its own; it
    only shells out to z3 to discharge a genuinely non-linear goal. A few
    expert-tier tasks need that path, so a run without z3 is reported as such
    rather than silently scoring those tasks as model failures.
    """
    return shutil.which("z3") is not None


def score_candidate(binary: str, candidate_dir: Path, tasks: list) -> dict:
    """Classify every task for one candidate dir. Returns {task: outcome}."""
    results = {}
    for task in tasks:
        candidate = candidate_dir / f"{task}.ae"
        results[task] = check_file(binary, candidate) if candidate.exists() else MISSING
    return results


def tier_breakdown(results: dict, tasks: list) -> dict:
    """Return {tier: [verified, total]} for a results dict, in tier order."""
    tiers: dict[str, list[int]] = {}
    for task in tasks:
        bucket = tiers.setdefault(task_difficulty(task), [0, 0])
        bucket[0] += 1 if results[task] == VERIFIED else 0
        bucket[1] += 1
    return {t: tiers[t] for t in sorted(
        tiers, key=lambda t: TIER_ORDER.index(t) if t in TIER_ORDER else 99)}


def wilson_ci(k: int, n: int, z: float = 1.96) -> tuple:
    """Wilson score 95% confidence interval for k successes in n trials.

    The benchmark's scorer is exact (the compiler never misjudges), but the
    69 tasks are still a *sample* -- the headline pass-rate has sampling
    error. The Wilson interval is the standard small-n / extreme-proportion
    interval (the plain Wald interval misbehaves near 0% and 100%).
    Returns (low_pct, high_pct).
    """
    if n == 0:
        return (0.0, 0.0)
    p = k / n
    denom = 1.0 + z * z / n
    center = (p + z * z / (2 * n)) / denom
    half = z * ((p * (1 - p) / n + z * z / (4 * n * n)) ** 0.5) / denom
    return (max(0.0, center - half) * 100.0, min(1.0, center + half) * 100.0)


def main() -> int:
    args = [a for a in sys.argv[1:] if a != "--json"]
    as_json = "--json" in sys.argv
    if len(args) != 1:
        print(__doc__)
        return 2
    candidate_dir = Path(args[0])
    if not candidate_dir.is_dir():
        print(f"error: {candidate_dir} is not a directory", file=sys.stderr)
        return 2

    binary = aether_bin()
    tasks = task_list()
    if not tasks:
        print(f"error: no tasks found in {TASKS_DIR}", file=sys.stderr)
        return 2

    results = score_candidate(binary, candidate_dir, tasks)
    verified = sum(1 for o in results.values() if o == VERIFIED)
    total = len(tasks)
    pct = 100.0 * verified / total
    tiers = tier_breakdown(results, tasks)
    is_baseline = candidate_dir.resolve() == (REPO_ROOT / "eval" / "baseline").resolve()

    if as_json:
        print(json.dumps({
            "model": candidate_dir.name,
            "candidate_dir": str(candidate_dir),
            "aether": binary,
            "z3": z3_available(),
            "tasks": total,
            "verified": verified,
            "score_pct": round(pct, 1),
            "ci95": [round(c, 1) for c in wilson_ci(verified, total)],
            "by_tier": tiers,
            "per_task": results,
        }, indent=2))
        return 1 if (is_baseline and verified != total) else 0

    z3_note = "present" if z3_available() else \
        "absent (non-linear expert tasks may not verify)"
    print(f"Aether verified-code benchmark — {total} tasks")
    print(f"candidate: {candidate_dir}")
    print(f"aether:    {binary}")
    print(f"z3:        {z3_note}\n")

    width = max(len(t) for t in tasks)
    marks = {VERIFIED: "✓", UNPROVEN: "✗", EFFECT_ERROR: "✗",
             TYPE_ERROR: "✗", PARSE_ERROR: "✗", MISSING: "·"}
    for task in tasks:
        print(f"  {marks[results[task]]} {task.ljust(width)}  {results[task]}")

    lo, hi = wilson_ci(verified, total)
    print(f"\n  score: {verified}/{total} verified ({pct:.0f}%)"
          f"   95% CI [{lo:.0f}%, {hi:.0f}%]")
    print("  (a task counts only when the compiler PROVED its contract)")

    print("\n  by difficulty:")
    for tier, (v, t) in tiers.items():
        print(f"    {tier:<13} {v}/{t}")

    # Self-test contract: the reference baseline must verify every task.
    if is_baseline:
        if verified != total:
            print("\nFAIL: the baseline must verify every task.", file=sys.stderr)
            return 1
        print("\nbaseline self-test passed.")
    return 0


if __name__ == "__main__":
    sys.exit(main())

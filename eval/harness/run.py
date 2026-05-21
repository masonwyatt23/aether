#!/usr/bin/env python3
"""Aether verified-code benchmark — scorer.

Runs every candidate solution through `aether check --json` and classifies it.
Unlike a test-pass-rate benchmark, a task only counts as solved when the
compiler *proves* its refinement contract — `errors == 0 and warnings == 0`.

Usage:
    python3 eval/harness/run.py <candidate-dir>

`<candidate-dir>` holds one `.ae` file per task, named after the task
directory (e.g. `04_clamp.ae`). `eval/baseline/` is the reference set and
should always score 12/12 — run it as the harness self-test:

    python3 eval/harness/run.py eval/baseline

Environment:
    AETHER_BIN   path to the `aether` binary (default: ./target/release/aether,
                 then `aether` on PATH).
"""

import json
import os
import subprocess
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
TASKS_DIR = REPO_ROOT / "eval" / "tasks"

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


def main() -> int:
    if len(sys.argv) != 2:
        print(__doc__)
        return 2
    candidate_dir = Path(sys.argv[1])
    if not candidate_dir.is_dir():
        print(f"error: {candidate_dir} is not a directory", file=sys.stderr)
        return 2

    binary = aether_bin()
    tasks = sorted(p.name for p in TASKS_DIR.iterdir() if p.is_dir())
    if not tasks:
        print(f"error: no tasks found in {TASKS_DIR}", file=sys.stderr)
        return 2

    print(f"Aether verified-code benchmark — {len(tasks)} tasks")
    print(f"candidate: {candidate_dir}")
    print(f"aether:    {binary}\n")

    results: dict[str, str] = {}
    for task in tasks:
        candidate = candidate_dir / f"{task}.ae"
        if not candidate.exists():
            results[task] = MISSING
        else:
            results[task] = check_file(binary, candidate)

    width = max(len(t) for t in tasks)
    marks = {VERIFIED: "✓", UNPROVEN: "✗", EFFECT_ERROR: "✗",
             TYPE_ERROR: "✗", PARSE_ERROR: "✗", MISSING: "·"}
    for task in tasks:
        outcome = results[task]
        print(f"  {marks[outcome]} {task.ljust(width)}  {outcome}")

    verified = sum(1 for o in results.values() if o == VERIFIED)
    total = len(tasks)
    pct = 100.0 * verified / total
    print(f"\n  score: {verified}/{total} verified ({pct:.0f}%)")
    print("  (a task counts only when the compiler PROVED its contract)")

    # Self-test contract: the baseline must score a perfect 12/12.
    if candidate_dir.resolve() == (REPO_ROOT / "eval" / "baseline").resolve():
        if verified != total:
            print("\nFAIL: the baseline must verify every task.", file=sys.stderr)
            return 1
        print("\nbaseline self-test passed.")
    return 0


if __name__ == "__main__":
    sys.exit(main())

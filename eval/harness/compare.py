#!/usr/bin/env python3
"""Aether verified-code benchmark -- multi-model comparison.

Scores several candidate directories at once and prints one table: per-model
total plus a column per difficulty tier, followed by a per-task matrix
showing which models verified which task.

Usage:
    python3 eval/harness/compare.py <candidate-dir> [<candidate-dir> ...]
    python3 eval/harness/compare.py --markdown eval/candidates/*

`--markdown` renders the summary table in Markdown, ready to paste into
RESULTS.md. The scorer is reused from run.py -- a task counts as solved only
when the Aether compiler proved its refinement contract (see run.py).

Provider names are read from each candidate dir's `_meta.json` when present
(written by generate.py); otherwise the provider column shows `--`.
"""

import json
import sys
from math import comb
from pathlib import Path

from run import (MISSING, TIER_ORDER, VERIFIED, aether_bin, score_candidate,
                 task_difficulty, task_list, tier_breakdown, z3_available)


def candidate_provider(d: Path) -> str:
    """Provider recorded in a candidate dir's _meta.json, or '--'."""
    meta = d / "_meta.json"
    if meta.exists():
        try:
            return json.loads(meta.read_text()).get("provider", "--")
        except (OSError, json.JSONDecodeError):
            pass
    return "--"


def score_all(dirs: list):
    """Score every candidate dir.

    Returns (tasks, runs) where runs is a list of dicts sorted by score
    descending: {name, provider, results, verified, total}.
    """
    binary = aether_bin()
    tasks = task_list()
    runs = []
    for d in dirs:
        results = score_candidate(binary, d, tasks)
        runs.append({
            "name": d.name,
            "provider": candidate_provider(d),
            "results": results,
            "verified": sum(1 for o in results.values() if o == VERIFIED),
            "total": len(tasks),
        })
    runs.sort(key=lambda r: r["verified"], reverse=True)
    return tasks, runs


def tiers_present(tasks: list) -> list:
    """Difficulty tiers that actually occur, in canonical order."""
    seen = {task_difficulty(t) for t in tasks}
    return [t for t in TIER_ORDER if t in seen]


def print_text_table(tasks: list, runs: list) -> None:
    tiers = tiers_present(tasks)
    name_w = max(len(r["name"]) for r in runs)
    prov_w = max(len(r["provider"]) for r in runs)
    header = f"  {'model'.ljust(name_w)}  {'provider'.ljust(prov_w)}  {'score':>12}"
    for t in tiers:
        header += f"  {t:>11}"
    print(header)
    print("  " + "-" * (len(header) - 2))
    for r in runs:
        pct = 100.0 * r["verified"] / r["total"]
        row = (f"  {r['name'].ljust(name_w)}  {r['provider'].ljust(prov_w)}  "
               f"{r['verified']:>3}/{r['total']:<3} {pct:>3.0f}%")
        bt = tier_breakdown(r["results"], tasks)
        for t in tiers:
            v, n = bt.get(t, [0, 0])
            row += f"  {f'{v}/{n}':>11}"
        print(row)


def print_markdown_table(tasks: list, runs: list) -> None:
    tiers = tiers_present(tasks)
    cols = ["Model", "Provider", "Score"] + tiers
    print("| " + " | ".join(cols) + " |")
    print("|" + "|".join(["---"] * len(cols)) + "|")
    for r in runs:
        pct = 100.0 * r["verified"] / r["total"]
        bt = tier_breakdown(r["results"], tasks)
        cells = [f"`{r['name']}`", r["provider"],
                 f"**{r['verified']}/{r['total']} -- {pct:.0f}%**"]
        for t in tiers:
            v, n = bt.get(t, [0, 0])
            cells.append(f"{v}/{n}")
        print("| " + " | ".join(cells) + " |")


def print_task_matrix(tasks: list, runs: list) -> None:
    """Per-task outcome matrix: one column per model, positional marks."""
    print("\n  per-task outcomes  (col order = models below)")
    for i, r in enumerate(runs):
        print(f"    [{i + 1}] {r['name']}")
    task_w = max(len(t) for t in tasks)
    for task in tasks:
        marks = ""
        for r in runs:
            o = r["results"][task]
            marks += "v" if o == VERIFIED else ("." if o == MISSING else "x")
        flag = "  <- split" if len({r["results"][task] for r in runs}) > 1 else ""
        print(f"  {task.ljust(task_w)}  {marks}{flag}")


def mcnemar_exact(a_results: dict, b_results: dict, tasks: list) -> tuple:
    """Exact-binomial McNemar test on two models' per-task verified/not.

    Returns (b, c, p): b = A solved & B missed, c = B solved & A missed. Both
    models face the same tasks, so a paired test is correct; at ~69 items the
    exact binomial p-value is preferred over the chi-square approximation.
    """
    b = sum(1 for t in tasks
            if a_results[t] == VERIFIED and b_results[t] != VERIFIED)
    c = sum(1 for t in tasks
            if a_results[t] != VERIFIED and b_results[t] == VERIFIED)
    n = b + c
    if n == 0:
        return b, c, 1.0
    tail = sum(comb(n, i) for i in range(min(b, c) + 1))
    return b, c, min(1.0, 2.0 * tail / (2 ** n))


def print_pairwise(tasks: list, runs: list) -> None:
    """Paired McNemar test for every model pair -- is the score gap real?"""
    print("\n  pairwise significance (McNemar exact paired test):")
    for i in range(len(runs)):
        for j in range(i + 1, len(runs)):
            a, b_ = runs[i], runs[j]
            b, c, p = mcnemar_exact(a["results"], b_["results"], tasks)
            verdict = "significant" if p < 0.05 else "NOT significant"
            print(f"    {a['name']} +{b} / {b_['name']} +{c}   "
                  f"p={p:.3f}  ({verdict})")


def print_discrimination(tasks: list, runs: list) -> None:
    """How many tasks actually separate the models in this comparison."""
    n = len(runs)
    none = all_ = disc = 0
    for t in tasks:
        v = sum(1 for r in runs if r["results"][t] == VERIFIED)
        if v == 0:
            none += 1
        elif v == n:
            all_ += 1
        else:
            disc += 1
    print(f"\n  item discrimination ({len(tasks)} tasks, {n} models):")
    print(f"    solved by all: {all_}   solved by none: {none}   "
          f"discriminating: {disc}")
    print("    (only discriminating items separate models; 0%/100% items "
          "mark ceilings/floors)")


def main() -> int:
    argv = [a for a in sys.argv[1:] if a != "--markdown"]
    as_markdown = "--markdown" in sys.argv
    dirs = [Path(a) for a in argv if Path(a).is_dir()]
    if not dirs:
        print(__doc__)
        return 2

    tasks, runs = score_all(dirs)

    if as_markdown:
        print_markdown_table(tasks, runs)
    else:
        z3 = "present" if z3_available() else "absent"
        print(f"Aether verified-code benchmark -- {len(tasks)} tasks, "
              f"{len(runs)} models  (z3: {z3})\n")
        print_text_table(tasks, runs)
        if len(runs) >= 2:
            print_discrimination(tasks, runs)
            print_pairwise(tasks, runs)
        print_task_matrix(tasks, runs)
    return 0


if __name__ == "__main__":
    sys.exit(main())

#!/usr/bin/env python3
"""Aether verified-code benchmark -- spec-writing track.

Most of the benchmark asks a model to write a function body that satisfies a
given contract. This track does the opposite, and it is harder: the model is
shown a complete, *correct* function and must write its `where` **contract**
-- the tightest postcondition that holds for that implementation.

Writing a good contract is harder than writing the body because the contract
must capture the function's *whole* behaviour. A contract is scored on two
axes (the Verina discipline):

  * SOUNDNESS  -- the reference implementation must VERIFY against it;
  * STRENGTH   -- it must REFUTE wrong code. We mutate the reference body and
                  require the model's contract to kill every mutant that the
                  task's own canonical (tight) contract kills.

A task counts as solved only at soundness AND a 100% relative kill-rate.

Usage:
    python3 eval/harness/specwrite.py <out-dir> <provider> [model] \
        [--tasks N-M] [--reasoning EFFORT]

Providers / model defaults / API keys are shared with generate.py. The task
set is read from AETHER_EVAL_TASKS (default: eval/tasks/).
"""

import json
import os
import re
import sys
import time
import urllib.error
from pathlib import Path

from generate import (LOCAL_PROVIDERS, PROVIDERS, REASONING_EFFORTS,
                       call_anthropic, call_openai_compatible,
                       call_openai_responses, pop_flag)
from mutate import mutants, verifies
from run import TASKS_DIR, aether_bin, wilson_ci

SPEC_SYSTEM = (
    "You write Aether refinement contracts. Given a complete, correct Aether "
    "function and a description of what it does, return ONLY the `where` "
    "clause predicate -- a single boolean expression over the function's "
    "parameters and the special variable `result`. It must be the TIGHTEST "
    "correct postcondition: true for this implementation, and false for any "
    "implementation that computes the wrong answer.\n"
    "The contract is checked by Aether's compiler, which proves LINEAR "
    "integer arithmetic. Use only `&&`, `||`, comparisons (`<`, `<=`, `>`, "
    "`>=`, `==`, `!=`) and `+`/`-`. Express piecewise behaviour as a "
    "disjunction of `(condition && result == value)` clauses -- do NOT put "
    "an `if`-expression inside the contract; the prover cannot discharge it.\n"
    "Return only the expression (what would follow the `where` keyword) -- "
    "no `where` keyword, no markdown fences, no prose."
)

# Per-task strength check: all degenerate mutants + a capped sample of the
# operator/literal mutants. Degenerate mutants are the sharpest loose-spec
# signal; capping the rest keeps the scorer fast.
SOFT_MUTANT_CAP = 12


def split_contract(text: str):
    """Split a reference .ae into (head, canonical_contract, tail).

    `head` is everything up to (not including) the `where` line; `tail` is
    the `effects ... { body }` remainder. Returns None if there is no
    `where` clause (contract-free tasks are skipped by this track).
    """
    m = re.search(r"^[ \t]*where (.+)$", text, re.M)
    if not m:
        return None
    return text[:m.start()], m.group(1).strip(), text[m.end():].lstrip("\n")


def nl_description(prompt_md: str) -> str:
    """The natural-language part of a prompt.md, with the signature block
    (which would leak the canonical contract) stripped off."""
    cut = prompt_md.find("## Signature")
    return (prompt_md[:cut] if cut >= 0 else prompt_md).strip()


def with_contract(head: str, contract: str, tail: str) -> str:
    """Reassemble a .ae file with a chosen `where` contract."""
    return f"{head}  where {contract}\n{tail}"


def strip_contract_reply(text: str) -> str:
    """Pull a bare predicate expression out of a model reply.

    Models often ignore the instruction and echo a whole function (or a full
    `where` line). If the reply contains a `where` clause, take the predicate
    between `where` and the following `effects`/body brace; otherwise treat
    the whole reply as the predicate. Either way, collapse to a single line.
    """
    text = "\n".join(ln for ln in text.splitlines()
                     if not ln.strip().startswith("```")).strip()
    m = re.search(r"\bwhere\b(.+?)(?=\n\s*effects\b|\n\s*\{|\Z)", text, re.S)
    payload = m.group(1) if m else text
    return " ".join(payload.split())


def call_model(provider, base, key, model, prompt, reasoning):
    """Single-shot model call, routed like generate.py."""
    if provider == "openai":
        return call_openai_responses(base, key, model, prompt, reasoning,
                                     system=SPEC_SYSTEM)
    if provider == "anthropic":
        return call_anthropic(base, key, model, prompt, system=SPEC_SYSTEM)
    return call_openai_compatible(base, key, model, prompt, system=SPEC_SYSTEM)


def score_contract(binary: str, head: str, model_contract: str,
                   canonical_ref: str, tail: str) -> dict:
    """Score one model-written contract: soundness + relative kill-rate."""
    model_ref = with_contract(head, model_contract, tail)
    sound = verifies(binary, model_ref)

    # Body-mutant sequences are identical for the two contracts (same body,
    # same params), so they zip: every mutant the canonical contract kills,
    # the model's contract must kill too. All degenerate mutants are checked;
    # the operator/literal mutants are capped to keep the scorer fast.
    soft_seen = 0
    relevant = killed = 0
    for (kind, c_text), (_, m_text) in zip(mutants(canonical_ref),
                                           mutants(model_ref)):
        if kind != "degenerate":
            soft_seen += 1
            if soft_seen > SOFT_MUTANT_CAP:
                continue
        if verifies(binary, c_text):
            continue                            # canonical accepts it -- skip
        relevant += 1
        if not verifies(binary, m_text):
            killed += 1
    kill_rate = killed / relevant if relevant else 1.0
    return {"sound": sound, "relevant": relevant, "killed": killed,
            "kill_rate": round(kill_rate, 3),
            "solved": sound and killed == relevant}


def main() -> int:
    argv, task_spec = pop_flag(list(sys.argv), "--tasks")
    argv, reasoning = pop_flag(argv, "--reasoning")
    reasoning = reasoning or "medium"
    if reasoning not in REASONING_EFFORTS:
        print("error: bad --reasoning", file=sys.stderr)
        return 2
    if len(argv) < 3 or argv[2] not in PROVIDERS:
        print(__doc__)
        return 2

    out_dir = Path(argv[1])
    provider = argv[2]
    env_var, base, default_model = PROVIDERS[provider]
    model = argv[3] if len(argv) > 3 else default_model
    if provider in LOCAL_PROVIDERS:
        base = os.environ.get(env_var, base)
        key = "ollama"
    else:
        key = os.environ.get(env_var)
        if not key:
            print(f"No {env_var} set -- the spec-writing track needs it.")
            return 0

    binary = aether_bin()
    out_dir.mkdir(parents=True, exist_ok=True)
    tasks = sorted(p.name for p in TASKS_DIR.iterdir() if p.is_dir())
    if task_spec:
        lo, _, hi = task_spec.partition("-")
        tasks = [t for t in tasks
                 if int(lo) <= int(t.split("_", 1)[0]) <= int(hi or lo)]
    print(f"Spec-writing track -- {provider}/{model}, {len(tasks)} tasks\n")

    results, meta = {}, {}
    for task in tasks:
        tdir = TASKS_DIR / task
        ref = (tdir / "reference.ae").read_text()
        parts = split_contract(ref)
        if parts is None:
            continue                            # contract-free task -- skip
        head, _canonical, tail = parts
        prompt = (
            nl_description((tdir / "prompt.md").read_text()) + "\n\n"
            + "Here is a complete, correct Aether function. Write the "
            "tightest `where` contract that holds for it:\n\n"
            + head + tail
        )
        started = time.monotonic()
        try:
            reply, _usage = call_model(provider, base, key, model, prompt,
                                       reasoning)
        except (urllib.error.URLError, urllib.error.HTTPError, ConnectionError,
                KeyError, TimeoutError) as exc:
            print(f"  !  {task}: request failed ({exc})", file=sys.stderr)
            continue
        contract = strip_contract_reply(reply)
        sc = score_contract(binary, head, contract, ref, tail)
        results[task] = sc
        meta[task] = {"contract": contract,
                      "latency_s": round(time.monotonic() - started, 2)}
        mark = "v" if sc["solved"] else ("~" if sc["sound"] else "x")
        print(f"  {mark} {task.ljust(26)} sound={sc['sound']!s:5} "
              f"kill {sc['killed']}/{sc['relevant']} ({sc['kill_rate']:.0%})")

    n = len(results)
    solved = sum(1 for r in results.values() if r["solved"])
    sound = sum(1 for r in results.values() if r["sound"])
    print(f"\n  spec-writing score: {solved}/{n} contracts "
          f"sound AND fully strong")
    if n:
        lo, hi = wilson_ci(solved, n)
        print(f"  95% CI [{lo:.0f}%, {hi:.0f}%]    "
              f"(sound-but-not-tight: {sound - solved})")

    (out_dir / "_specwrite.json").write_text(json.dumps({
        "provider": provider, "model": model, "tasks": n, "solved": solved,
        "sound": sound, "per_task": {t: {**results[t], **meta[t]}
                                     for t in results},
    }, indent=2) + "\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())

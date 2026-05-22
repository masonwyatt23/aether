#!/usr/bin/env python3
"""Aether verified-code benchmark -- agentic verifier-in-the-loop mode.

Single-shot generation asks a model to write provably-correct code blind.
Real verified-code development is iterative: you write code, the verifier
rejects it with a counterexample, you fix it. This harness runs that loop --
the model submits a function body, receives the exact `aether check`
diagnostics (refutation counterexamples included), and retries up to N times.
The final submission is scored by the same rule as the single-shot benchmark.

It reports a pass@turn curve (pass@1 == single-shot, ... pass@N) so the
*shape* of the gain from verifier feedback is visible, plus turns-to-solve.

Usage:
    python3 eval/harness/agentic.py <out-dir> <provider> [model] \
        [--turns N] [--tasks N-M] [--reasoning EFFORT]

Providers, model defaults, and API keys are shared with generate.py.

Anti-gaming: the only feedback is the verifier's own output -- the reference
solution is never shown; turns are hard-capped; the final body must pass all
contracts (type, effect, refinement) to count, exactly as in single-shot.
"""

import json
import os
import subprocess
import sys
import tempfile
import urllib.error
import urllib.request
from pathlib import Path

from generate import (LOCAL_PROVIDERS, PROVIDERS, REASONING_EFFORTS, SSL_CTX,
                       extract_code, pop_flag)
from run import aether_bin

REPO_ROOT = Path(__file__).resolve().parents[2]
TASKS_DIR = Path(os.environ.get("AETHER_EVAL_TASKS", REPO_ROOT / "eval" / "tasks"))

AGENTIC_SYSTEM = (
    "You are writing Aether, a statically typed language whose compiler "
    "PROVES refinement postconditions (the `where` clause) at compile time. "
    "Given a task and a function signature, return ONLY the complete Aether "
    "function -- the exact signature provided, with the stub body replaced by "
    "a correct implementation. No markdown fences, no prose. If the compiler "
    "rejects your attempt you will be shown its exact diagnostics (including "
    "any refutation counterexample); use them to fix the body and resubmit."
)


def _extract_reply(provider: str, payload: dict) -> str:
    """Pull the assistant text out of one provider's response payload."""
    if provider == "openai":
        text = ""
        for item in payload.get("output", []):
            for chunk in item.get("content", []):
                if chunk.get("type") == "output_text":
                    text += chunk.get("text", "")
        return text
    if provider == "anthropic":
        return "".join(b.get("text", "") for b in payload.get("content", []))
    return payload["choices"][0]["message"]["content"] or ""


def chat(provider: str, base: str, key: str, model: str,
         convo: list, reasoning: str) -> str:
    """One model turn over a multi-message conversation. Returns reply text."""
    if provider == "openai":
        body = {"model": model, "input": convo,
                "reasoning": {"effort": reasoning}, "max_output_tokens": 8192}
        url = base + "/responses"
        headers = {"content-type": "application/json",
                   "authorization": "Bearer " + key}
    elif provider == "anthropic":
        system = "".join(m["content"] for m in convo if m["role"] == "system")
        body = {"model": model, "max_tokens": 2048, "temperature": 0,
                "system": system,
                "messages": [m for m in convo if m["role"] != "system"]}
        url = base + "/messages"
        headers = {"content-type": "application/json", "x-api-key": key,
                   "anthropic-version": "2023-06-01"}
    else:  # xai / gemini / ollama -- OpenAI-compatible chat completions
        body = {"model": model, "temperature": 0, "messages": convo}
        url = base + "/chat/completions"
        headers = {"content-type": "application/json",
                   "authorization": "Bearer " + key}
    req = urllib.request.Request(url, data=json.dumps(body).encode(),
                                 headers=headers, method="POST")
    with urllib.request.urlopen(req, timeout=600, context=SSL_CTX) as resp:
        return _extract_reply(provider, json.loads(resp.read()))


def check(binary: str, source: str) -> tuple:
    """Run `aether check --json` on source text.

    Returns (verified, diagnostics_text) -- the diagnostics are fed back to
    the model verbatim, which is the whole point of the loop.
    """
    fd, path = tempfile.mkstemp(suffix=".ae")
    try:
        with os.fdopen(fd, "w") as f:
            f.write(source)
        proc = subprocess.run([binary, "check", "--json", path],
                              capture_output=True, text=True, timeout=30)
    except (subprocess.TimeoutExpired, FileNotFoundError) as exc:
        return False, f"harness could not run aether: {exc}"
    finally:
        os.unlink(path)
    out = proc.stdout.strip()
    start = out.find("{")
    if start < 0:
        return False, "aether check produced no JSON output"
    try:
        result = json.loads(out[start:])
    except json.JSONDecodeError:
        return False, "aether check output was not parseable"
    verified = int(result.get("errors", 1)) == 0 and int(result.get("warnings", 1)) == 0
    diags = result.get("diagnostics", [])
    text = "\n".join(
        f"- [{d.get('severity')}] line {d.get('line')}: {d.get('message')}"
        for d in diags)
    return verified, text or "(no diagnostics reported)"


def solve_task(provider, base, key, model, reasoning, binary, task, turns):
    """Run the verifier-in-the-loop on one task.

    Returns (solved_at_turn_or_None, final_source).
    """
    tdir = TASKS_DIR / task
    prompt = ((tdir / "prompt.md").read_text() + "\n\n"
              + "Signature to complete (return the whole function):\n\n"
              + (tdir / "signature.ae").read_text())
    convo = [{"role": "system", "content": AGENTIC_SYSTEM},
             {"role": "user", "content": prompt}]
    final = ""
    for turn in range(1, turns + 1):
        reply = chat(provider, base, key, model, convo, reasoning)
        final = extract_code(reply)
        verified, diag = check(binary, final)
        if verified:
            return turn, final
        convo.append({"role": "assistant", "content": reply})
        convo.append({"role": "user", "content":
                      "`aether check` rejected that submission. Diagnostics:\n\n"
                      + diag + "\n\nFix the body and return the complete "
                      "function again."})
    return None, final


def main() -> int:
    argv, task_spec = pop_flag(list(sys.argv), "--tasks")
    argv, turns_s = pop_flag(argv, "--turns")
    argv, reasoning = pop_flag(argv, "--reasoning")
    reasoning = reasoning or "medium"
    turns = int(turns_s) if turns_s else 5
    if reasoning not in REASONING_EFFORTS:
        print("error: --reasoning must be one of " + ", ".join(REASONING_EFFORTS),
              file=sys.stderr)
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
            print(f"No {env_var} set -- agentic mode needs it.")
            return 0

    out_dir.mkdir(parents=True, exist_ok=True)
    tasks = sorted(p.name for p in TASKS_DIR.iterdir()
                   if p.is_dir()
                   and (not task_spec
                        or _in_range(p.name, task_spec)))
    binary = aether_bin()
    print(f"Agentic verifier-in-the-loop -- {provider}/{model}, "
          f"up to {turns} turns, {len(tasks)} tasks\n")

    solved = {}
    for task in tasks:
        try:
            turn, source = solve_task(provider, base, key, model, reasoning,
                                      binary, task, turns)
        except (urllib.error.URLError, urllib.error.HTTPError, ConnectionError,
                KeyError, TimeoutError) as exc:
            print(f"  !  {task}: request failed ({exc})", file=sys.stderr)
            solved[task] = None
            continue
        (out_dir / f"{task}.ae").write_text(source)
        solved[task] = turn
        mark = f"turn {turn}" if turn else "UNSOLVED"
        print(f"  {'v' if turn else 'x'}  {task.ljust(24)} {mark}")

    total = len(tasks)
    print(f"\n  pass@turn curve  ({total} tasks):")
    curve = []
    for k in range(1, turns + 1):
        hit = sum(1 for t in solved.values() if t is not None and t <= k)
        curve.append(hit)
        bar = "#" * round(40 * hit / total) if total else ""
        print(f"    pass@{k}: {hit}/{total} ({100*hit/total:.0f}%)  {bar}")
    final_solved = curve[-1] if curve else 0
    gain = final_solved - curve[0] if curve else 0
    print(f"\n  single-shot: {curve[0]}/{total}   "
          f"agentic (@{turns}): {final_solved}/{total}   "
          f"feedback gain: +{gain}")
    durs = [t for t in solved.values() if t]
    if durs:
        print(f"  mean turns-to-solve: {sum(durs)/len(durs):.1f}")

    (out_dir / "_agentic.json").write_text(json.dumps({
        "provider": provider, "model": model, "turns": turns,
        "reasoning_effort": reasoning if provider == "openai" else None,
        "per_task": solved,
        "pass_at_turn": {str(k + 1): curve[k] for k in range(len(curve))},
        "total": total,
    }, indent=2) + "\n")
    return 0


def _in_range(task: str, spec: str) -> bool:
    lo, _, hi = spec.partition("-")
    num = int(task.split("_", 1)[0])
    return int(lo) <= num <= (int(hi) if hi else int(lo))


if __name__ == "__main__":
    sys.exit(main())

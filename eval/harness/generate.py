#!/usr/bin/env python3
"""Aether verified-code benchmark — solution generator (optional LLM mode).

Sends each task's `prompt.md` + `signature.ae` to an Anthropic model and
writes the model's solution into a candidate directory, ready for
`run.py` to score.

Usage:
    ANTHROPIC_API_KEY=sk-... python3 eval/harness/generate.py <out-dir> [model]

Without an API key it prints setup instructions and exits cleanly — the
benchmark itself (`run.py`) needs no key, only this optional generator does.

Uses only the Python standard library (`urllib`) — no `requests` dependency.
"""

import json
import os
import sys
import urllib.error
import urllib.request
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
TASKS_DIR = REPO_ROOT / "eval" / "tasks"
API_URL = "https://api.anthropic.com/v1/messages"
DEFAULT_MODEL = "claude-opus-4-7"

SYSTEM = (
    "You are writing Aether, a statically typed language whose compiler "
    "PROVES refinement postconditions (the `where` clause) at compile time. "
    "Given a task and a function signature, return ONLY the complete Aether "
    "function — the exact signature provided, with the stub body replaced by "
    "a correct implementation. No markdown fences, no prose, no commentary."
)


def call_anthropic(api_key: str, model: str, prompt: str) -> str:
    """POST a single-turn completion request; return the text response."""
    body = json.dumps(
        {
            "model": model,
            "max_tokens": 1024,
            "system": SYSTEM,
            "messages": [{"role": "user", "content": prompt}],
        }
    ).encode()
    req = urllib.request.Request(
        API_URL,
        data=body,
        headers={
            "content-type": "application/json",
            "x-api-key": api_key,
            "anthropic-version": "2023-06-01",
        },
        method="POST",
    )
    with urllib.request.urlopen(req, timeout=120) as resp:
        payload = json.loads(resp.read())
    # The response content is a list of blocks; concatenate the text ones.
    return "".join(b.get("text", "") for b in payload.get("content", []))


def strip_fences(text: str) -> str:
    """Drop any ```...``` markdown fence the model may have added."""
    lines = [ln for ln in text.splitlines() if not ln.strip().startswith("```")]
    return "\n".join(lines).strip() + "\n"


def main() -> int:
    if len(sys.argv) < 2:
        print(__doc__)
        return 2
    out_dir = Path(sys.argv[1])
    model = sys.argv[2] if len(sys.argv) > 2 else DEFAULT_MODEL

    api_key = os.environ.get("ANTHROPIC_API_KEY")
    if not api_key:
        print("No ANTHROPIC_API_KEY set — the generator is optional.\n")
        print("To generate model solutions and score them:")
        print("  export ANTHROPIC_API_KEY=sk-...")
        print(f"  python3 {sys.argv[0]} candidates/run1")
        print("  python3 eval/harness/run.py candidates/run1\n")
        print("The benchmark's reference baseline needs no key:")
        print("  python3 eval/harness/run.py eval/baseline")
        return 0

    out_dir.mkdir(parents=True, exist_ok=True)
    tasks = sorted(p.name for p in TASKS_DIR.iterdir() if p.is_dir())
    print(f"Generating {len(tasks)} solutions with {model} → {out_dir}\n")

    for task in tasks:
        tdir = TASKS_DIR / task
        prompt_md = (tdir / "prompt.md").read_text()
        signature = (tdir / "signature.ae").read_text()
        prompt = (
            f"{prompt_md}\n\n"
            f"Signature to complete (return the whole function):\n\n{signature}"
        )
        try:
            answer = call_anthropic(api_key, model, prompt)
        except (urllib.error.URLError, urllib.error.HTTPError, TimeoutError) as exc:
            print(f"  ! {task}: request failed ({exc})", file=sys.stderr)
            continue
        (out_dir / f"{task}.ae").write_text(strip_fences(answer))
        print(f"  ✓ {task}")

    print(f"\nDone. Score with:\n  python3 eval/harness/run.py {out_dir}")
    return 0


if __name__ == "__main__":
    sys.exit(main())

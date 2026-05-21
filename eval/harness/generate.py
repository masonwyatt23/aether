#!/usr/bin/env python3
"""Aether verified-code benchmark -- solution generator (optional LLM mode).

Sends each task's `prompt.md` + `signature.ae` to a model and writes the
model's solution into a candidate directory, ready for `run.py` to score.

Usage:
    python3 eval/harness/generate.py <out-dir> <provider> [model]

Providers (each reads the matching API key from the environment):
    openai      OPENAI_API_KEY      e.g. model gpt-5.2
    xai         XAI_API_KEY         e.g. model grok-4.3
    anthropic   ANTHROPIC_API_KEY   e.g. model claude-opus-4-7

With no key set the script prints setup instructions and exits cleanly --
the benchmark itself (`run.py`) needs no key, only this generator does.

Uses only the Python standard library (`urllib`) -- no third-party deps.
"""

import json
import os
import ssl
import sys
import urllib.error
import urllib.request
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
TASKS_DIR = REPO_ROOT / "eval" / "tasks"


def _ssl_context():
    """A verified SSL context, preferring the `certifi` CA bundle (works
    around Python installs without a usable system trust store)."""
    try:
        import certifi
        return ssl.create_default_context(cafile=certifi.where())
    except ImportError:
        return ssl.create_default_context()


SSL_CTX = _ssl_context()

SYSTEM = (
    "You are writing Aether, a statically typed language whose compiler "
    "PROVES refinement postconditions (the `where` clause) at compile time. "
    "Given a task and a function signature, return ONLY the complete Aether "
    "function -- the exact signature provided, with the stub body replaced by "
    "a correct implementation. No markdown fences, no prose, no commentary."
)

# provider -> (env var, base url, default model)
PROVIDERS = {
    "openai": ("OPENAI_API_KEY", "https://api.openai.com/v1", "gpt-5.2"),
    "xai": ("XAI_API_KEY", "https://api.x.ai/v1", "grok-4.3"),
    "anthropic": ("ANTHROPIC_API_KEY", "https://api.anthropic.com/v1", "claude-opus-4-7"),
}


def call_openai_compatible(base, key, model, prompt):
    """OpenAI / xAI chat-completions request (identical wire format)."""
    body = {
        "model": model,
        "messages": [
            {"role": "system", "content": SYSTEM},
            {"role": "user", "content": prompt},
        ],
    }
    req = urllib.request.Request(
        base + "/chat/completions",
        data=json.dumps(body).encode(),
        headers={"content-type": "application/json", "authorization": "Bearer " + key},
        method="POST",
    )
    with urllib.request.urlopen(req, timeout=240, context=SSL_CTX) as resp:
        payload = json.loads(resp.read())
    return payload["choices"][0]["message"]["content"] or ""


def call_anthropic(base, key, model, prompt):
    """Anthropic messages request."""
    body = {
        "model": model,
        "max_tokens": 2048,
        "system": SYSTEM,
        "messages": [{"role": "user", "content": prompt}],
    }
    req = urllib.request.Request(
        base + "/messages",
        data=json.dumps(body).encode(),
        headers={
            "content-type": "application/json",
            "x-api-key": key,
            "anthropic-version": "2023-06-01",
        },
        method="POST",
    )
    with urllib.request.urlopen(req, timeout=240, context=SSL_CTX) as resp:
        payload = json.loads(resp.read())
    return "".join(b.get("text", "") for b in payload.get("content", []))


def strip_fences(text):
    """Drop any triple-backtick markdown fence the model may have added."""
    lines = [ln for ln in text.splitlines() if not ln.strip().startswith("```")]
    return "\n".join(lines).strip() + "\n"


def main():
    if len(sys.argv) < 3 or sys.argv[2] not in PROVIDERS:
        print(__doc__)
        return 2
    out_dir = Path(sys.argv[1])
    provider = sys.argv[2]
    env_var, base, default_model = PROVIDERS[provider]
    model = sys.argv[3] if len(sys.argv) > 3 else default_model

    key = os.environ.get(env_var)
    if not key:
        print("No " + env_var + " set -- the generator is optional.\n")
        print("  export " + env_var + "=...")
        print("  python3 " + sys.argv[0] + " candidates/run1 " + provider + " " + model)
        return 0

    out_dir.mkdir(parents=True, exist_ok=True)
    tasks = sorted(p.name for p in TASKS_DIR.iterdir() if p.is_dir())
    print("Generating " + str(len(tasks)) + " solutions -- "
          + provider + "/" + model + " -> " + str(out_dir) + "\n")

    ok = 0
    for task in tasks:
        tdir = TASKS_DIR / task
        prompt = (
            (tdir / "prompt.md").read_text() + "\n\n"
            + "Signature to complete (return the whole function):\n\n"
            + (tdir / "signature.ae").read_text()
        )
        try:
            if provider == "anthropic":
                answer = call_anthropic(base, key, model, prompt)
            else:
                answer = call_openai_compatible(base, key, model, prompt)
        except (urllib.error.URLError, urllib.error.HTTPError, KeyError, TimeoutError) as exc:
            detail = exc
            if isinstance(exc, urllib.error.HTTPError):
                detail = "HTTP " + str(exc.code) + ": " + exc.read().decode(errors="replace")[:200]
            print("  ! " + task + ": request failed (" + str(detail) + ")", file=sys.stderr)
            continue
        (out_dir / (task + ".ae")).write_text(strip_fences(answer))
        print("  ok  " + task)
        ok += 1

    print("\nGenerated " + str(ok) + "/" + str(len(tasks)) + ". Score with:")
    print("  python3 eval/harness/run.py " + str(out_dir))
    return 0 if ok > 0 else 1


if __name__ == "__main__":
    sys.exit(main())

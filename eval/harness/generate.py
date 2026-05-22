#!/usr/bin/env python3
"""Aether verified-code benchmark -- solution generator (optional LLM mode).

Sends each task's `prompt.md` + `signature.ae` to a model and writes the
model's solution into a candidate directory, ready for `run.py` to score.

Usage:
    python3 eval/harness/generate.py <out-dir> <provider> [model] \
        [--tasks N-M] [--reasoning EFFORT]

Providers:
    openai      OPENAI_API_KEY      e.g. model gpt-5.5          (paid API)
    xai         XAI_API_KEY         e.g. model grok-4.3         (paid API)
    anthropic   ANTHROPIC_API_KEY   e.g. model claude-opus-4-7  (paid API)
    gemini      GEMINI_API_KEY      e.g. model gemini-3-flash   (paid API)
    ollama      (no key)            e.g. model qwen2.5-coder:7b (local, free)

The `ollama` provider talks to a local Ollama daemon over its
OpenAI-compatible endpoint (default http://localhost:11434/v1, override with
OLLAMA_HOST) -- no API key, no cost. Pull a model first: `ollama pull <model>`.

For the paid providers, with no key set the script prints setup instructions
and exits cleanly -- the benchmark itself (`run.py`) needs no key, only this
generator does.

`openai` models are GPT-5.x reasoning models: they are called through the
Responses API (which the others' chat-completions path does not use) because
reasoning models reject a non-default `temperature`. The reasoning level is
pinned with `--reasoning EFFORT` (none|minimal|low|medium|high|xhigh,
default medium) for run-to-run comparability.

`--tasks N-M` (or `--tasks N`) restricts generation to a task-number range,
e.g. `--tasks 61-72` to regenerate just the expert tier.

Note: the `gemini` default model id is a best guess -- verify the current
tag at ai.google.dev/gemini-api/docs/models before relying on it.

Uses only the Python standard library (`urllib`) -- no third-party deps.
"""

import json
import os
import re
import ssl
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
# AETHER_EVAL_TASKS lets the harness target a generated instance set.
TASKS_DIR = Path(os.environ.get("AETHER_EVAL_TASKS", REPO_ROOT / "eval" / "tasks"))


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

# provider -> (env var / base-url override var, default base url, default model)
PROVIDERS = {
    "openai": ("OPENAI_API_KEY", "https://api.openai.com/v1", "gpt-5.5"),
    "xai": ("XAI_API_KEY", "https://api.x.ai/v1", "grok-4.3"),
    "anthropic": ("ANTHROPIC_API_KEY", "https://api.anthropic.com/v1", "claude-opus-4-7"),
    "gemini": ("GEMINI_API_KEY",
               "https://generativelanguage.googleapis.com/v1beta/openai", "gemini-3-flash"),
    "ollama": ("OLLAMA_HOST", "http://localhost:11434/v1", "qwen2.5-coder:7b"),
}

# Reasoning-effort levels accepted by the OpenAI Responses API.
REASONING_EFFORTS = ("none", "minimal", "low", "medium", "high", "xhigh")

# Providers that run locally: no API key, and the env var (if set) overrides
# the base URL rather than supplying a credential.
LOCAL_PROVIDERS = {"ollama"}


def call_openai_compatible(base, key, model, prompt, system=SYSTEM):
    """OpenAI / xAI / Ollama chat-completions request (identical wire format).

    Returns (text, usage). `temperature` is pinned to 0 so a benchmark run is
    reproducible -- local models otherwise drift between runs.
    """
    body = {
        "model": model,
        "temperature": 0,
        "messages": [
            {"role": "system", "content": system},
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
    text = payload["choices"][0]["message"]["content"] or ""
    return text, payload.get("usage", {})


def call_openai_responses(base, key, model, prompt, effort, system=SYSTEM):
    """OpenAI Responses API request for GPT-5.x reasoning models.

    Reasoning models reject a non-default `temperature` on Chat Completions,
    so the benchmark uses the Responses API and pins `reasoning.effort`
    instead -- that is the parameter that governs run-to-run behaviour for
    these models. Returns (text, usage); `usage` carries `output_tokens` and
    `output_tokens_details.reasoning_tokens` for cost auditing.
    """
    body = {
        "model": model,
        "input": [
            {"role": "system", "content": system},
            {"role": "user", "content": prompt},
        ],
        "reasoning": {"effort": effort},
        "max_output_tokens": 8192,
    }
    req = urllib.request.Request(
        base + "/responses",
        data=json.dumps(body).encode(),
        headers={"content-type": "application/json", "authorization": "Bearer " + key},
        method="POST",
    )
    with urllib.request.urlopen(req, timeout=600, context=SSL_CTX) as resp:
        payload = json.loads(resp.read())
    text = ""
    for item in payload.get("output", []):
        for chunk in item.get("content", []):
            if chunk.get("type") == "output_text":
                text += chunk.get("text", "")
    return text, payload.get("usage", {})


def call_anthropic(base, key, model, prompt, system=SYSTEM):
    """Anthropic messages request. Returns (text, usage)."""
    body = {
        "model": model,
        "max_tokens": 2048,
        "temperature": 0,
        "system": system,
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
    text = "".join(b.get("text", "") for b in payload.get("content", []))
    return text, payload.get("usage", {})


def extract_code(text):
    """Pull the Aether source out of a model response.

    Prefers the first fenced code block; otherwise slices from the first
    `fn`/`type` declaration (plus any leading `##` doc comments) to the last
    closing brace. Falls back to the raw stripped text when no declaration is
    found. Local models in particular wrap answers in prose and fences, which
    a plain fence-strip leaves behind as un-parseable lines.
    """
    fence = re.search(r"```[a-zA-Z]*\n(.*?)```", text, re.S)
    body = fence.group(1) if fence else text
    lines = body.splitlines()
    decl = next((i for i, ln in enumerate(lines)
                 if re.match(r"\s*(fn|type)\b", ln)), None)
    if decl is None:
        return body.strip() + "\n"
    start = decl
    while start > 0 and lines[start - 1].strip().startswith("##"):
        start -= 1
    end = next((i for i in range(len(lines) - 1, decl - 1, -1)
                if lines[i].strip() == "}"), len(lines) - 1)
    return "\n".join(lines[start:end + 1]).strip() + "\n"


def pop_flag(argv, name):
    """Remove `--name value` from argv; return (clean_argv, value_or_None)."""
    if name not in argv:
        return argv, None
    i = argv.index(name)
    value = argv[i + 1] if i + 1 < len(argv) else None
    return argv[:i] + argv[i + 2:], value


def task_in_range(task, spec):
    """True if `task`'s `NN_` prefix falls in a `--tasks` spec (`N` or `N-M`)."""
    if not spec:
        return True
    lo, _, hi = spec.partition("-")
    num = int(task.split("_", 1)[0])
    return int(lo) <= num <= (int(hi) if hi else int(lo))


def main():
    argv, task_spec = pop_flag(list(sys.argv), "--tasks")
    argv, reasoning = pop_flag(argv, "--reasoning")
    reasoning = reasoning or "medium"
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
        base = os.environ.get(env_var, base)  # OLLAMA_HOST overrides the URL
        key = "ollama"  # local daemon ignores auth but wants a non-empty bearer
    else:
        key = os.environ.get(env_var)
        if not key:
            print("No " + env_var + " set -- the generator is optional.\n")
            print("  export " + env_var + "=...")
            print("  python3 " + argv[0] + " candidates/run1 " + provider + " " + model)
            return 0

    out_dir.mkdir(parents=True, exist_ok=True)
    tasks = sorted(p.name for p in TASKS_DIR.iterdir()
                   if p.is_dir() and task_in_range(p.name, task_spec))
    if not tasks:
        print("error: no tasks match --tasks " + str(task_spec), file=sys.stderr)
        return 2
    label = provider + "/" + model
    if provider == "openai":
        label += " (reasoning=" + reasoning + ")"
    print("Generating " + str(len(tasks)) + " solutions -- "
          + label + " -> " + str(out_dir) + "\n")

    ok = 0
    meta = {}
    for task in tasks:
        tdir = TASKS_DIR / task
        prompt = (
            (tdir / "prompt.md").read_text() + "\n\n"
            + "Signature to complete (return the whole function):\n\n"
            + (tdir / "signature.ae").read_text()
        )
        started = time.monotonic()
        try:
            if provider == "openai":
                answer, usage = call_openai_responses(base, key, model, prompt, reasoning)
            elif provider == "anthropic":
                answer, usage = call_anthropic(base, key, model, prompt)
            else:
                answer, usage = call_openai_compatible(base, key, model, prompt)
        except (urllib.error.URLError, urllib.error.HTTPError, ConnectionError,
                KeyError, TimeoutError) as exc:
            detail = exc
            if isinstance(exc, urllib.error.HTTPError):
                detail = "HTTP " + str(exc.code) + ": " + exc.read().decode(errors="replace")[:200]
            print("  ! " + task + ": request failed (" + str(detail) + ")", file=sys.stderr)
            continue
        (out_dir / (task + ".ae")).write_text(extract_code(answer))
        latency = round(time.monotonic() - started, 2)
        meta[task] = {"latency_s": latency, "usage": usage}
        print("  ok  " + task + "  (" + str(latency) + "s)")
        ok += 1

    run_meta = {"provider": provider, "model": model, "tasks": meta}
    if provider == "openai":
        run_meta["reasoning_effort"] = reasoning
    (out_dir / "_meta.json").write_text(json.dumps(run_meta, indent=2) + "\n")
    print("\nGenerated " + str(ok) + "/" + str(len(tasks)) + ". Score with:")
    print("  python3 eval/harness/run.py " + str(out_dir))
    return 0 if ok > 0 else 1


if __name__ == "__main__":
    sys.exit(main())

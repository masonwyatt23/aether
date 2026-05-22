#!/usr/bin/env python3
"""Materialize a fresh Aether benchmark task set from procedural templates.

Samples a seed and instantiates every template family (see eval/templates.py)
into a directory of `NNN_family/{prompt.md,signature.ae,reference.ae}` task
folders -- the same on-disk shape as eval/tasks/, so run.py / generate.py /
agentic.py / mutate.py can all target it via the AETHER_EVAL_TASKS env var:

    python3 eval/harness/gen_tasks.py --seed 7 --per-family 10 --out eval/instances/s7
    AETHER_EVAL_TASKS=eval/instances/s7/tasks \
        python3 eval/harness/run.py eval/instances/s7/baseline

The set is written as `DIR/tasks/` (the task folders) plus `DIR/baseline/`
(reference solutions flattened, one `<task>.ae` each) so the baseline
self-test works exactly as for the static eval/tasks + eval/baseline pair.

Because the set is regenerated per seed, no fixed instance can be memorized
-- this is the benchmark's contamination defence. The seed is recorded in
`_manifest.json` so any run is exactly reproducible.

Usage:
    python3 eval/harness/gen_tasks.py --seed S --per-family N --out DIR [--validate]

`--validate` (recommended) runs `aether check` on every generated task and
asserts the reference VERIFIES and the stub does NOT -- a self-test that the
templates produce sound, non-trivial tasks.
"""

import json
import random
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO_ROOT / "eval"))
sys.path.insert(0, str(REPO_ROOT / "eval" / "harness"))

from templates import FAMILIES, generate          # noqa: E402
from run import aether_bin                          # noqa: E402
from mutate import verifies                         # noqa: E402


def build_prompt(inst: dict) -> str:
    """Render an instance's prompt.md."""
    spec = "\n".join("- " + s for s in inst["spec"])
    return (f"# {inst['title']}\n\n{inst['intro']}\n\n"
            f"## Specification\n\n{spec}\n\n"
            f"## Signature (provided -- do not change)\n\n"
            f"```aether\n{inst['signature']}```\n\n"
            f"Replace the stub body with a correct implementation.\n\n"
            f"**Difficulty:** {inst['tier']}\n")


def pop(argv, name, default=None):
    if name in argv:
        i = argv.index(name)
        return argv[:i] + argv[i + 2:], argv[i + 1]
    return argv, default


def main() -> int:
    argv = list(sys.argv[1:])
    validate = "--validate" in argv
    argv = [a for a in argv if a != "--validate"]
    argv, seed = pop(argv, "--seed", "0")
    argv, per_family = pop(argv, "--per-family", "10")
    argv, out = pop(argv, "--out")
    if out is None:
        print(__doc__)
        return 2
    per_family = int(per_family)
    out_dir = Path(out)
    tasks_dir = out_dir / "tasks"
    baseline_dir = out_dir / "baseline"
    tasks_dir.mkdir(parents=True, exist_ok=True)
    baseline_dir.mkdir(parents=True, exist_ok=True)

    binary = aether_bin()
    seq = 0
    seen = set()
    manifest = {"seed": seed, "per_family": per_family,
                "families": sorted(FAMILIES), "tasks": []}
    bad = []
    print(f"Generating from {len(FAMILIES)} families x {per_family} "
          f"instances -- seed {seed}\n")

    for family in sorted(FAMILIES):
        made = 0
        for i in range(per_family * 3):  # over-sample; dedupe drops repeats
            if made >= per_family:
                break
            rng = random.Random(f"{seed}:{family}:{i}")
            inst = generate(family, rng)
            if inst["signature"] in seen:
                continue          # textual duplicate -- skip, keep instances distinct
            seen.add(inst["signature"])
            seq += 1
            made += 1
            name = f"{seq:03d}_{family}"
            tdir = tasks_dir / name
            tdir.mkdir(exist_ok=True)
            (tdir / "prompt.md").write_text(build_prompt(inst))
            (tdir / "signature.ae").write_text(inst["signature"])
            (tdir / "reference.ae").write_text(inst["reference"])
            (baseline_dir / (name + ".ae")).write_text(inst["reference"])
            manifest["tasks"].append({"name": name, "family": family,
                                      "tier": inst["tier"]})
            if validate:
                ref_ok = verifies(binary, inst["reference"])
                stub_ok = verifies(binary, inst["signature"])
                # ADT tasks are contract-free: the stub is non-exhaustive and
                # so does not verify; the only check is the reference.
                if not ref_ok or stub_ok:
                    bad.append((name, ref_ok, stub_ok))
                    mark = "X"
                else:
                    mark = "v"
                print(f"  {mark} {name}")
    (out_dir / "_manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")

    print(f"\nGenerated {seq} tasks into {tasks_dir}")
    if validate:
        if bad:
            print(f"\nVALIDATION FAILED for {len(bad)} task(s):")
            for name, ref_ok, stub_ok in bad:
                why = []
                if not ref_ok:
                    why.append("reference does not verify")
                if stub_ok:
                    why.append("stub verifies (task is trivial)")
                print(f"  {name}: {'; '.join(why)}")
            return 1
        print("validation passed: every reference verifies, every stub fails.")
    print(f"\nBaseline self-test (every reference must verify):\n"
          f"  AETHER_EVAL_TASKS={tasks_dir} python3 eval/harness/run.py {baseline_dir}")
    return 0


if __name__ == "__main__":
    sys.exit(main())

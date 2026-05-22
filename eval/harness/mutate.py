#!/usr/bin/env python3
"""Aether verified-code benchmark -- specification mutation audit.

A `where` contract is only as good as the wrong programs it rejects. This
tool treats every contract as a test oracle: it generates mutants of each
reference solution and checks that `aether check` REFUTES every one. A mutant
that still verifies is a "survivor" -- direct evidence the contract is too
loose to tell a correct solution from a degenerate one.

The headline check is the degenerate-return gauntlet: replacing the whole
body with a constant or a bare parameter. If `0` verifies a task, the
contract cannot distinguish a real solution from nothing.

Operator- and literal-mutant survivors are also reported, but some of those
are *equivalent mutants* (a different body that is still correct) -- those
are expected to verify and need a human eye. A degenerate survivor never is.

It doubles as a standalone **spec-quality tool**: point `--file` at any
Aether function and it reports whether the `where` contract is STRONG
(refutes every wrong mutant) or LOOSE -- a "vacuous" specification a
degenerate body satisfies. Spec vacuity is an open problem in the
verified-code literature; this turns it into a measured, automated check.

Usage:
    python3 eval/harness/mutate.py [task ...]            # audit benchmark tasks
    python3 eval/harness/mutate.py --file path/to.ae     # audit one .ae file
    python3 eval/harness/mutate.py --verbose ...         # print survivor bodies

Exit code 1 if any audited contract has a degenerate survivor.
"""

import json
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

from run import aether_bin

REPO_ROOT = Path(__file__).resolve().parents[2]
TASKS_DIR = Path(os.environ.get("AETHER_EVAL_TASKS", REPO_ROOT / "eval" / "tasks"))

CMP = ["<=", ">=", "==", "!=", "<", ">"]
ARITH = ["+", "-", "*"]
BOOL = ["&&", "||"]


def split_reference(text: str):
    """Return (prefix, body, suffix) of a reference .ae file -- `body` is the
    function body between the `effects {} {` brace and the final `}`."""
    m = re.search(r"effects\s*\{\s*\}\s*\{", text)
    if not m:
        return None
    open_brace = m.end() - 1
    close_brace = text.rstrip().rfind("}")
    if close_brace <= open_brace:
        return None
    return text[:open_brace + 1], text[open_brace + 1:close_brace], text[close_brace:]


def param_names(text: str) -> list:
    """Parameter identifiers from the first `fn name(...)` signature."""
    m = re.search(r"fn\s+\w+\s*\((.*?)\)\s*(->|where|effects)", text, re.S)
    if not m:
        return []
    names = []
    # Split on commas that are not inside a `{...}` refinement predicate.
    for part in re.split(r",(?![^{]*\})", m.group(1)):
        name = part.strip().split(":")[0].strip()
        if re.fullmatch(r"\w+", name):
            names.append(name)
    return names


def mutants(text: str):
    """Yield (kind, mutated_text) pairs for one reference file."""
    parts = split_reference(text)
    if parts is None:
        return
    prefix, body, suffix = parts

    # Degenerate-return mutants -- a constant or a bare parameter. These are
    # never a correct implementation; any survivor is a loose contract.
    for const in ("0", "1", "2"):
        yield "degenerate", prefix + "\n  " + const + "\n" + suffix
    for name in param_names(text):
        yield "degenerate", prefix + "\n  " + name + "\n" + suffix

    # Operator-replacement mutants -- swap one operator occurrence.
    for ops in (CMP, ARITH, BOOL):
        pattern = "|".join(re.escape(o) for o in sorted(ops, key=len, reverse=True))
        for m in re.finditer(pattern, body):
            # Skip the `>` inside a `=>` match arrow.
            if m.group(0) == ">" and m.start() > 0 and body[m.start() - 1] == "=":
                continue
            for repl in ops:
                if repl != m.group(0):
                    yield "operator", prefix + body[:m.start()] + repl + body[m.end():] + suffix

    # Off-by-one / constant mutants on integer literals in the body.
    for m in re.finditer(r"\b\d+\b", body):
        n = int(m.group(0))
        for repl in sorted({0, 1, n + 1, n - 1} - {n}):
            yield "literal", prefix + body[:m.start()] + str(repl) + body[m.end():] + suffix


def verifies(binary: str, text: str) -> bool:
    """True if `text` verifies clean (zero errors, zero warnings)."""
    fd, path = tempfile.mkstemp(suffix=".ae")
    try:
        with os.fdopen(fd, "w") as f:
            f.write(text)
        proc = subprocess.run([binary, "check", "--json", path],
                              capture_output=True, text=True, timeout=30)
    except (subprocess.TimeoutExpired, FileNotFoundError):
        return False
    finally:
        os.unlink(path)
    out = proc.stdout.strip()
    start = out.find("{")
    if start < 0:
        return False
    try:
        result = json.loads(out[start:])
    except json.JSONDecodeError:
        return False
    return result.get("errors", 1) == 0 and result.get("warnings", 1) == 0


def audit_ref(binary: str, ref: str):
    """Mutation-audit one reference .ae source. Returns
    (total, killed, survivors-by-kind), or None if it has no `where` contract."""
    if "where" not in ref:
        return None  # contract-free (ADT exhaustiveness) -- mutation audit N/A
    survivors = {"degenerate": [], "operator": [], "literal": []}
    total = 0
    for kind, mutant in mutants(ref):
        total += 1
        if verifies(binary, mutant):
            survivors[kind].append(mutant)
    killed = total - sum(len(v) for v in survivors.values())
    return total, killed, survivors


def audit_task(binary: str, task: str):
    """Mutation-audit one benchmark task's reference solution."""
    return audit_ref(binary, (TASKS_DIR / task / "reference.ae").read_text())


def audit_one_file(binary: str, path: Path, verbose: bool) -> int:
    """Standalone spec-quality audit of a single Aether file. Prints a
    STRONG/LOOSE verdict; returns an exit code."""
    result = audit_ref(binary, path.read_text())
    if result is None:
        print(f"{path}: no `where` contract -- nothing to audit.")
        return 0
    total, killed, survivors = result
    deg = survivors["degenerate"]
    soft = survivors["operator"] + survivors["literal"]
    score = killed / total if total else 1.0
    print(f"spec-quality audit -- {path}\n")
    print(f"  mutants killed:       {killed}/{total} ({score:.0%})")
    print(f"  degenerate survivors: {len(deg)}")
    print(f"  other survivors:      {len(soft)}  "
          f"(some may be equivalent mutants -- a human eye is needed)")
    if deg:
        print("\n  VERDICT: LOOSE -- a degenerate body satisfies this contract.")
        print("  The `where` clause is too weak to tell a correct solution")
        print("  from a vacuous one. Strengthen it into a tight functional spec.")
        if verbose:
            for m in deg:
                print("\n  degenerate body that still verifies:")
                print("    " + _body_of(m).replace("\n", "\n    "))
        return 1
    print("\n  VERDICT: STRONG -- every degenerate mutant is refuted.")
    return 0


def main() -> int:
    argv = [a for a in sys.argv[1:] if a != "--verbose"]
    verbose = "--verbose" in sys.argv
    binary = aether_bin()

    # Standalone spec-quality mode: audit a single arbitrary .ae file.
    if "--file" in argv:
        i = argv.index("--file")
        if i + 1 >= len(argv) or not Path(argv[i + 1]).exists():
            print("error: --file needs a path to an existing .ae file",
                  file=sys.stderr)
            return 2
        return audit_one_file(binary, Path(argv[i + 1]), verbose)

    tasks = argv or sorted(p.name for p in TASKS_DIR.iterdir() if p.is_dir())

    print(f"Aether spec mutation audit -- {len(tasks)} tasks\n")
    loose, na = [], []
    for task in tasks:
        result = audit_task(binary, task)
        if result is None:
            na.append(task)
            print(f"  -  {task}  (contract-free -- audit N/A)")
            continue
        total, killed, survivors = result
        deg = survivors["degenerate"]
        soft = survivors["operator"] + survivors["literal"]
        score = killed / total if total else 1.0
        flag = "LOOSE" if deg else ("ok" if not soft else "ok*")
        print(f"  {'X' if deg else 'v'}  {task.ljust(24)} "
              f"kill {killed}/{total} ({score:.0%})  "
              f"degenerate-survivors={len(deg)}  other-survivors={len(soft)}  [{flag}]")
        if deg:
            loose.append(task)
        if verbose and (deg or soft):
            for m in deg:
                print("      DEGENERATE SURVIVOR (loose contract!):")
                print("      " + _body_of(m).replace("\n", "\n      "))
            for m in soft[:3]:
                print("      operator/literal survivor (check if equivalent):")
                print("      " + _body_of(m).replace("\n", "\n      "))

    print()
    print(f"  contract-free tasks (audit N/A): {len(na)}")
    if loose:
        print(f"  LOOSE contracts ({len(loose)}): " + ", ".join(loose))
        print("  -> a degenerate body verifies these; strengthen the `where` clause.")
        return 1
    print("  no loose contracts -- every degenerate mutant is refuted.")
    return 0


def _body_of(text: str) -> str:
    parts = split_reference(text)
    return parts[1].strip() if parts else "?"


if __name__ == "__main__":
    sys.exit(main())

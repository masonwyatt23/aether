#!/usr/bin/env python3
"""Procedural task templates for the Aether verified-code benchmark.

Each *family* is a function `(rng) -> instance` that mechanically builds a
fresh task: a natural-language prompt, a stubbed `signature.ae`, and a
verified `reference.ae`. `gen_tasks.py` samples a seed and materializes a
whole task set, so no fixed instance can be memorized -- the benchmark is
regenerated every run, which makes training-data contamination structurally
hard (see eval/README.md).

Two invariants every family must hold, enforced by `gen_tasks.py --validate`:

  * the generated `reference.ae` VERIFIES (`aether check`, 0 errors/warnings);
  * every contract is a TIGHT functional spec -- a degenerate body (a
    constant, a bare parameter) is refuted. Contracts are written as a
    disjunction of `(branch-condition && result == value)` clauses or as a
    relational postcondition, never as a loose one-sided bound.

Pure standard library; no third-party dependencies.
"""

import random

# Disjoint identifier pools -- families draw one name per role so instances
# vary in surface form (a defence against shallow prompt memorisation).
VALUE_NAMES = ["x", "value", "input", "n", "reading", "level", "sample", "pos"]
LOWER_NAMES = ["lo", "floor", "low", "bottom", "start", "minimum"]
UPPER_NAMES = ["hi", "ceiling", "top", "cap", "limit", "maximum"]
DELTA_NAMES = ["step", "delta", "shift", "advance", "offset", "jump"]


def _ints(rng, n, span=(0, 80), gap=1):
    """n strictly-increasing non-negative integers, each at least `gap` apart."""
    out = []
    cur = rng.randint(*span)
    for _ in range(n):
        out.append(cur)
        cur += rng.randint(gap, gap + 14)
    return out


def _assemble(doc, header, contract, body, decreases=None):
    """Build a full `.ae` source file from its parts."""
    where = f"  where {contract}\n" if contract else ""
    dec = f"  decreases {decreases}\n" if decreases else ""
    return (f"## {doc[0]}\n## {doc[1]}\n"
            f"{header}\n{where}{dec}  effects {{}} {{\n{body}\n}}\n")


# --------------------------------------------------------------------------
# Family: clamp -- clamp a value into a fixed inclusive range [lo, hi].
# --------------------------------------------------------------------------
def family_clamp(rng):
    v = rng.choice(VALUE_NAMES)
    lo, hi = _ints(rng, 2, span=(0, 40), gap=4)
    frame = rng.choice([
        ("Range Clamp", f"clamps `{v}` into the inclusive range [{lo}, {hi}]"),
        ("Saturate to Bounds", f"saturates `{v}` so it never leaves [{lo}, {hi}]"),
        ("Bounded Reading", f"caps a sensor reading `{v}` to the valid band [{lo}, {hi}]"),
    ])
    header = f"fn clamp_value({v}: Int) -> Int"
    contract = (f"({v} < {lo} && result == {lo}) || "
                f"({v} > {hi} && result == {hi}) || "
                f"({lo} <= {v} && {v} <= {hi} && result == {v})")
    body = (f"  if {v} < {lo} then {lo}\n"
            f"  else if {v} > {hi} then {hi}\n"
            f"  else {v}")
    doc = (f"Clamp {v} into [{lo}, {hi}].",
           "Contract: result is lo/hi/x exactly, per the branch (compile-time).")
    return dict(
        family="clamp", tier="medium-hard", title=f"Task: {frame[0]}",
        intro=f"Write `clamp_value`, which {frame[1]}.",
        spec=[f"If `{v}` is below `{lo}`, return `{lo}`.",
              f"If `{v}` is above `{hi}`, return `{hi}`.",
              f"Otherwise return `{v}` unchanged."],
        signature=_assemble(doc, header, contract, "  0"),
        reference=_assemble(doc, header, contract, body))


# --------------------------------------------------------------------------
# Family: window_clamp -- clamp into [lo, lo + width] for a symbolic lo.
# --------------------------------------------------------------------------
def family_window(rng):
    v = rng.choice(VALUE_NAMES)
    lo = rng.choice(LOWER_NAMES)
    width = rng.randint(3, 30)
    frame = rng.choice([
        ("Sliding Window", f"clamps `{v}` into the window [{lo}, {lo} + {width}]"),
        ("Offset Band", f"holds `{v}` within {width} units above `{lo}`"),
    ])
    header = f"fn window_clamp({v}: Int, {lo}: Int) -> Int"
    hi = f"{lo} + {width}"
    contract = (f"({v} < {lo} && result == {lo}) || "
                f"({v} > {hi} && result == {hi}) || "
                f"({lo} <= {v} && {v} <= {hi} && result == {v})")
    body = (f"  if {v} < {lo} then {lo}\n"
            f"  else if {v} > {hi} then {hi}\n"
            f"  else {v}")
    doc = (f"Clamp {v} into the window [{lo}, {lo} + {width}].",
           "Contract: a tight functional spec proved at compile time.")
    return dict(
        family="window", tier="medium-hard", title=f"Task: {frame[0]}",
        intro=f"Write `window_clamp`, which {frame[1]}.",
        spec=[f"The window is `[{lo}, {lo} + {width}]`.",
              f"Below the window return `{lo}`; above it return `{lo} + {width}`;",
              f"inside it return `{v}` unchanged."],
        signature=_assemble(doc, header, contract, "  0"),
        reference=_assemble(doc, header, contract, body))


# --------------------------------------------------------------------------
# Family: minmax -- the smaller (or larger) of two integers.
# --------------------------------------------------------------------------
def family_minmax(rng):
    a, b = rng.sample(VALUE_NAMES, 2)
    is_min = rng.random() < 0.5
    if is_min:
        word, op, rel = "smaller", "<=", "<="
        frame = ("Minimum of Two", f"returns the smaller of `{a}` and `{b}`")
    else:
        word, op, rel = "larger", ">=", ">="
        frame = ("Maximum of Two", f"returns the larger of `{a}` and `{b}`")
    header = f"fn pick({a}: Int, {b}: Int) -> Int"
    contract = (f"(result == {a} || result == {b}) && "
                f"result {rel} {a} && result {rel} {b}")
    body = f"  if {a} {op} {b} then {a} else {b}"
    doc = (f"Return the {word} of {a} and {b}.",
           "Contract: result equals one input and is the extreme of both.")
    return dict(
        family="minmax", tier="easy", title=f"Task: {frame[0]}",
        intro=f"Write `pick`, which {frame[1]}.",
        spec=[f"Return whichever of `{a}` and `{b}` is the {word}.",
              "The result must be exactly one of the two inputs."],
        signature=_assemble(doc, header, contract, "  0"),
        reference=_assemble(doc, header, contract, body))


# --------------------------------------------------------------------------
# Family: abs_val -- absolute value.
# --------------------------------------------------------------------------
def family_abs(rng):
    v = rng.choice(VALUE_NAMES)
    frame = rng.choice([
        ("Absolute Value", f"returns the magnitude of `{v}`"),
        ("Distance from Zero", f"returns how far `{v}` is from zero"),
    ])
    header = f"fn magnitude({v}: Int) -> Int"
    contract = f"result >= 0 && (result == {v} || result == 0 - {v})"
    body = f"  if {v} < 0 then 0 - {v} else {v}"
    doc = (f"Return the absolute value of {v}.",
           "Contract: result is non-negative and equals x or -x.")
    return dict(
        family="abs", tier="easy", title=f"Task: {frame[0]}",
        intro=f"Write `magnitude`, which {frame[1]}.",
        spec=[f"If `{v}` is negative, return `0 - {v}`; otherwise return `{v}`.",
              "The result is never negative."],
        signature=_assemble(doc, header, contract, "  0"),
        reference=_assemble(doc, header, contract, body))


# --------------------------------------------------------------------------
# Family: sign_val -- the sign of an integer, in {-1, 0, 1}.
# --------------------------------------------------------------------------
def family_sign(rng):
    v = rng.choice(VALUE_NAMES)
    header = f"fn sign_of({v}: Int) -> Int"
    contract = (f"({v} < 0 && result == 0 - 1) || "
                f"({v} == 0 && result == 0) || "
                f"({v} > 0 && result == 1)")
    body = f"  if {v} < 0 then 0 - 1 else if {v} > 0 then 1 else 0"
    doc = (f"Return the sign of {v}: -1, 0, or 1.",
           "Contract: a three-way functional spec proved at compile time.")
    return dict(
        family="sign", tier="medium", title="Task: Integer Sign",
        intro=f"Write `sign_of`, which returns `-1`, `0`, or `1` for the sign of `{v}`.",
        spec=[f"Negative `{v}` => `-1` (written `0 - 1`).",
              f"Zero `{v}` => `0`.   Positive `{v}` => `1`."],
        signature=_assemble(doc, header, contract, "  0"),
        reference=_assemble(doc, header, contract, body))


# --------------------------------------------------------------------------
# Family: sat_add -- add a step but saturate at a ceiling.
# --------------------------------------------------------------------------
def family_sat_add(rng):
    v = rng.choice(VALUE_NAMES)
    s = rng.choice(DELTA_NAMES)
    cap = rng.randint(20, 70)
    header = (f"fn saturating_add({v}: Int{{q: q >= 0 && q <= {cap}}}, "
              f"{s}: Int{{w: w >= 0}}) -> Int")
    contract = (f"({v} + {s} <= {cap} && result == {v} + {s}) || "
                f"({v} + {s} > {cap} && result == {cap})")
    body = f"  if {v} + {s} > {cap} then {cap} else {v} + {s}"
    doc = (f"Add {s} to {v}, saturating at the ceiling {cap}.",
           f"Precondition: 0 <= {v} <= {cap}, {s} >= 0.")
    return dict(
        family="sat_add", tier="hard", title="Task: Saturating Add",
        intro=f"Write `saturating_add`, which adds `{s}` to `{v}` but never "
              f"exceeds the ceiling `{cap}`.",
        spec=[f"If `{v} + {s}` stays within `{cap}`, return the sum.",
              f"Otherwise return `{cap}`."],
        signature=_assemble(doc, header, contract, "  0"),
        reference=_assemble(doc, header, contract, body))


# --------------------------------------------------------------------------
# Family: sat_sub -- subtract a step but never drop below a floor.
# --------------------------------------------------------------------------
def family_sat_sub(rng):
    v = rng.choice(VALUE_NAMES)
    s = rng.choice(DELTA_NAMES)
    lo, hi = _ints(rng, 2, span=(0, 30), gap=20)
    header = (f"fn saturating_sub({v}: Int{{q: q >= {lo} && q <= {hi}}}, "
              f"{s}: Int{{w: w >= 0}}) -> Int")
    contract = (f"({v} - {s} >= {lo} && result == {v} - {s}) || "
                f"({v} - {s} < {lo} && result == {lo})")
    body = f"  if {v} - {s} < {lo} then {lo} else {v} - {s}"
    doc = (f"Subtract {s} from {v}, flooring the result at {lo}.",
           f"Precondition: {lo} <= {v} <= {hi}, {s} >= 0.")
    return dict(
        family="sat_sub", tier="hard", title="Task: Saturating Subtract",
        intro=f"Write `saturating_sub`, which subtracts `{s}` from `{v}` but "
              f"never drops below the floor `{lo}`.",
        spec=[f"If `{v} - {s}` stays at or above `{lo}`, return the difference.",
              f"Otherwise return `{lo}`."],
        signature=_assemble(doc, header, contract, "  0"),
        reference=_assemble(doc, header, contract, body))


# --------------------------------------------------------------------------
# Family: mirror -- the mirror index within a fixed range [a, b].
# --------------------------------------------------------------------------
def family_mirror(rng):
    v = rng.choice(VALUE_NAMES)
    a, b = _ints(rng, 2, span=(0, 25), gap=6)
    total = a + b
    header = f"fn mirror_index({v}: Int{{q: q >= {a} && q <= {b}}}) -> Int"
    contract = f"result >= {a} && result <= {b} && result + {v} == {total}"
    body = f"  {total} - {v}"
    doc = (f"Return the mirror of index {v} within [{a}, {b}].",
           f"The mirror is equidistant from the far end: result + {v} == {total}.")
    return dict(
        family="mirror", tier="hard", title="Task: Mirror Index",
        intro=f"Write `mirror_index`: given a valid index `{v}` in `[{a}, {b}]`, "
              f"return the index the same distance from the *other* end.",
        spec=[f"The mirror of `{v}` in `[{a}, {b}]` satisfies `result + {v} == {total}`.",
              f"It is itself a valid index in `[{a}, {b}]`.",
              "Trap: a single-range formula is off by the lower bound."],
        signature=_assemble(doc, header, contract, "  0"),
        reference=_assemble(doc, header, contract, body))


# --------------------------------------------------------------------------
# Family: bucket -- map a value into one of N ordered buckets.
# --------------------------------------------------------------------------
def family_bucket(rng):
    v = rng.choice(VALUE_NAMES)
    n = rng.choice([3, 3, 4, 5])  # bucket count -- structural variation
    thr = _ints(rng, n - 1, span=(5, 30), gap=6)
    header = f"fn bucket({v}: Int) -> Int"
    clauses, body_lines = [], []
    clauses.append(f"({v} < {thr[0]} && result == 0)")
    body_lines.append(f"  if {v} < {thr[0]} then 0")
    for i in range(1, n - 1):
        clauses.append(f"({thr[i-1]} <= {v} && {v} < {thr[i]} && result == {i})")
        body_lines.append(f"  else if {v} < {thr[i]} then {i}")
    clauses.append(f"({v} >= {thr[-1]} && result == {n - 1})")
    body_lines.append(f"  else {n - 1}")
    contract = " || ".join(clauses)
    body = "\n".join(body_lines)
    tier = "expert" if n >= 5 else "hard"
    doc = (f"Map {v} into one of {n} ordered buckets 0..{n - 1}.",
           f"Thresholds: {', '.join(map(str, thr))}.")
    return dict(
        family="bucket", tier=tier, title=f"Task: {n}-Way Bucket",
        intro=f"Write `bucket`, which classifies `{v}` into a bucket index "
              f"`0..{n - 1}` by the thresholds `{', '.join(map(str, thr))}`.",
        spec=[f"`{v}` below `{thr[0]}` => bucket `0`."]
             + [f"`{v}` in `[{thr[i-1]}, {thr[i]})` => bucket `{i}`."
                for i in range(1, n - 1)]
             + [f"`{v}` at or above `{thr[-1]}` => bucket `{n - 1}`."],
        signature=_assemble(doc, header, contract, "  0"),
        reference=_assemble(doc, header, contract, body))


# --------------------------------------------------------------------------
# Family: adt_classify -- exhaustive match over a generated algebraic type.
# Contract-free: difficulty is exhaustive destructuring, not a refinement.
# --------------------------------------------------------------------------
_CTORS = ["Alpha", "Beta", "Gamma", "Delta", "Sigma", "Omega", "Tau", "Rho"]


def family_adt(rng):
    n = rng.choice([2, 3, 3, 4])
    ctors = rng.sample(_CTORS, n)
    tname = rng.choice(["Token", "Shape", "Node", "Event", "Cell"])
    decl = f"type {tname} = " + " | ".join(f"{c}(Int)" for c in ctors)
    header = f"{decl}\n\nfn classify(v: {tname}) -> Int"
    arms = ", ".join(f"{c}(a) => {i}" for i, c in enumerate(ctors))
    body = f"  match v with {{ {arms} }}"
    stub = f"  match v with {{ {ctors[0]}(a) => 0 }}"
    doc = (f"Classify a {tname} into a code 0..{n - 1}.",
           "Every constructor must be matched (non-exhaustive => warning).")
    sig = (f"## {doc[0]}\n## {doc[1]}\n{header}\n  effects {{}} {{\n{stub}\n}}\n")
    ref = (f"## {doc[0]}\n## {doc[1]}\n{header}\n  effects {{}} {{\n{body}\n}}\n")
    return dict(
        family="adt", tier="medium-hard", title=f"Task: Classify a {tname}",
        intro=f"The type `{tname}` has {n} constructors. Write `classify`, "
              f"mapping each constructor to its index with an exhaustive `match`.",
        spec=[f"`{c}(_)` => `{i}`." for i, c in enumerate(ctors)]
             + ["A non-exhaustive `match` is a warning, which counts as unverified."],
        signature=sig, reference=ref)


# --------------------------------------------------------------------------
# Family: compose -- a two-function task. The main function's contract is
# discharged only by importing the helper's `where` postcondition at the call
# site (modular contract reasoning). The model must write BOTH bodies.
# --------------------------------------------------------------------------
def family_compose(rng):
    hv = rng.choice(VALUE_NAMES)
    mv = rng.choice([n for n in VALUE_NAMES if n != hv])
    k = rng.randint(2, 12)
    lo = rng.randint(0, 25)
    hname = rng.choice(["shift_by", "bump", "add_step", "raise_by"])
    mname = rng.choice(["pipeline", "run_stage", "process", "apply_shift"])
    h_doc = (f"Helper: add {k} to {hv}.",
             f"Contract: result == {hv} + {k} (proved at compile time).")
    h_header = f"fn {hname}({hv}: Int) -> Int"
    h_contract = f"result == {hv} + {k}"
    m_doc = (f"Main: apply {hname} to a value that is at least {lo}.",
             f"Contract: result >= {lo + k} -- follows from {hname}'s contract.")
    m_header = f"fn {mname}({mv}: Int{{q: q >= {lo}}}) -> Int"
    m_contract = f"result >= {lo + k}"
    sig = (_assemble(h_doc, h_header, h_contract, "  0") + "\n"
           + _assemble(m_doc, m_header, m_contract, "  0"))
    ref = (_assemble(h_doc, h_header, h_contract, f"  {hv} + {k}") + "\n"
           + _assemble(m_doc, m_header, m_contract, f"  {hname}({mv})"))
    return dict(
        family="compose", tier="hard", title="Task: Composed Functions",
        intro=f"Write BOTH functions. `{hname}` adds `{k}`; `{mname}` applies "
              f"it to a value that is at least `{lo}`.",
        spec=[f"`{hname}({hv})` must return `{hv} + {k}`.",
              f"`{mname}({mv})` applies `{hname}` to `{mv}` (which is >= {lo}).",
              f"`{mname}`'s contract `result >= {lo + k}` is provable only "
              f"once `{hname}` carries a correct, tight contract."],
        signature=sig, reference=ref)


# --------------------------------------------------------------------------
# Family: recursive -- a termination-checked recursive function. The signature
# carries a `decreases` measure, so the body must provably terminate AND meet
# a linear functional contract. The recursive call's own `ensures` is the
# inductive hypothesis (sound because `decreases` is enforced).
# --------------------------------------------------------------------------
def family_recursive(rng):
    n = rng.choice(["n", "count", "steps", "rounds", "ticks"])
    acc = rng.choice(["acc", "total", "sum", "carry", "so_far"])
    inc = rng.randint(1, 4)
    fname = rng.choice(["accumulate", "count_down", "step_total", "fold_n"])
    header = f"fn {fname}({n}: Int{{q: q >= 0}}, {acc}: Int) -> Int"
    contract = f"result == {acc} + {n} * {inc}"
    body = (f"  if {n} <= 0 then {acc}\n"
            f"  else {fname}({n} - 1, {acc} + {inc})")
    doc = (f"Recursively add {inc} to {acc}, once per step, for {n} steps.",
           f"Verified: terminates (decreases {n}) and result == {acc} + {n}*{inc}.")
    return dict(
        family="recursive", tier="expert", title="Task: Verified Recursion",
        intro=f"Write `{fname}` recursively: starting from `{acc}`, add `{inc}` "
              f"once for each of the `{n}` steps (`{n} >= 0`).",
        spec=[f"If `{n}` is 0, return `{acc}`.",
              f"Otherwise recurse on `{fname}({n} - 1, {acc} + {inc})`.",
              f"The signature's `decreases {n}` clause means the recursion "
              f"must provably terminate -- each call must shrink `{n}`."],
        signature=_assemble(doc, header, contract, "  0", decreases=n),
        reference=_assemble(doc, header, contract, body, decreases=n))


# --------------------------------------------------------------------------
# Family: compose_chain -- a three-stage composition. Each stage carries a
# tight contract; the final lower bound is provable only if all three hold.
# Exercises modular contract reasoning through two call layers.
# --------------------------------------------------------------------------
def family_compose_chain(rng):
    x = rng.choice(VALUE_NAMES)
    a = rng.randint(2, 9)
    b = rng.randint(2, 9)
    lo = rng.randint(1, 20)
    f1 = rng.choice(["base", "stage_one", "seed", "ground"])
    f2 = rng.choice(["mid", "stage_two", "extend", "refine"])
    f3 = rng.choice(["top", "finish", "pipeline", "deliver"])
    rv = rng.choice(["r", "tmp", "partial"])
    mv = rng.choice(["m", "stage", "interm"])
    d1 = (f"Stage 1: add {a} to {x}.", f"Contract: result == {x} + {a}.")
    h1 = f"fn {f1}({x}: Int) -> Int"
    c1 = f"result == {x} + {a}"
    d2 = (f"Stage 2: apply {f1}, then add {b} more.",
          f"Contract: result == {x} + {a + b} (needs {f1}'s contract).")
    h2 = f"fn {f2}({x}: Int) -> Int"
    c2 = f"result == {x} + {a + b}"
    b2 = f"  let {rv} = {f1}({x})\n  {rv} + {b}"
    d3 = (f"Stage 3: apply {f2} to an input that is at least {lo}.",
          f"Contract: result >= {lo + a + b}.")
    h3 = f"fn {f3}({x}: Int{{q: q >= {lo}}}) -> Int"
    c3 = f"result >= {lo + a + b}"
    b3 = f"  let {mv} = {f2}({x})\n  {mv}"
    sig = (_assemble(d1, h1, c1, "  0") + "\n"
           + _assemble(d2, h2, c2, "  0") + "\n"
           + _assemble(d3, h3, c3, "  0"))
    ref = (_assemble(d1, h1, c1, f"  {x} + {a}") + "\n"
           + _assemble(d2, h2, c2, b2) + "\n"
           + _assemble(d3, h3, c3, b3))
    return dict(
        family="compose_chain", tier="expert",
        title="Task: Three-Stage Composition",
        intro=f"Write all THREE functions. `{f2}` builds on `{f1}`; "
              f"`{f3}` builds on `{f2}`.",
        spec=[f"`{f1}({x})` returns `{x} + {a}`.",
              f"`{f2}({x})` applies `{f1}`, then adds `{b}` more.",
              f"`{f3}({x})` applies `{f2}`; its lower-bound contract holds "
              f"only if every stage's contract does."],
        signature=sig, reference=ref)


# --------------------------------------------------------------------------
# Family: recursive_range -- recursion whose termination measure is `hi - lo`
# (two parameters). The recursion must move `lo` toward `hi`; the opposite
# direction fails the `decreases` check.
# --------------------------------------------------------------------------
def family_recursive_range(rng):
    lo = rng.choice(["lo", "start", "cursor", "i"])
    hi = rng.choice(["hi", "stop", "end", "limit"])
    acc = rng.choice(["acc", "total", "tally", "carry"])
    fname = rng.choice(["count_range", "walk_range", "scan", "span"])
    header = (f"fn {fname}({lo}: Int, {hi}: Int{{h: h >= {lo}}}, "
              f"{acc}: Int) -> Int")
    contract = f"result == {acc} + ({hi} - {lo})"
    body = (f"  if {lo} >= {hi} then {acc}\n"
            f"  else {fname}({lo} + 1, {hi}, {acc} + 1)")
    doc = (f"Recursively count the integers in [{lo}, {hi}) into {acc}.",
           f"Verified: terminates (decreases {hi} - {lo}); "
           f"result == {acc} + ({hi} - {lo}).")
    return dict(
        family="recursive_range", tier="expert",
        title="Task: Recursion over a Range",
        intro=f"Write `{fname}` recursively: count the integers from `{lo}` up "
              f"to (not including) `{hi}`, adding one to `{acc}` for each.",
        spec=[f"If `{lo} >= {hi}` the range is empty -- return `{acc}`.",
              f"Otherwise advance and recurse.",
              f"`decreases {hi} - {lo}` means the recursion must shrink the "
              f"range -- move `{lo}` toward `{hi}`, not away from it."],
        signature=_assemble(doc, header, contract, "  0",
                            decreases=f"{hi} - {lo}"),
        reference=_assemble(doc, header, contract, body,
                            decreases=f"{hi} - {lo}"))


# --------------------------------------------------------------------------
# Family: list_sum -- sum the first K elements of a list whose element type
# carries a refinement (`[Int{v: v >= B}]`). The contract follows from the
# list-element refinement flowing to each indexed access.
# --------------------------------------------------------------------------
def family_list_sum(rng):
    xs = rng.choice(["xs", "items", "values", "data", "scores"])
    k = rng.randint(2, 5)
    b = rng.randint(1, 5)
    fname = rng.choice(["sum_head", "total_first", "head_sum", "lead_total"])
    header = f"fn {fname}({xs}: [Int{{v: v >= {b}}}]) -> Int"
    contract = f"result >= {k * b}"
    terms = " + ".join(f"{xs}[{i}]" for i in range(k))
    doc = (f"Sum the first {k} elements of {xs} (each element is >= {b}).",
           f"Verified: result >= {k * b}, from the element refinement.")
    return dict(
        family="list_sum", tier="hard",
        title="Task: Sum of Bounded List Elements",
        intro=f"Write `{fname}`: return the sum of the first {k} elements of "
              f"`{xs}` -- a list whose every element is at least {b}.",
        spec=[f"`{xs}` has element-refinement type `[Int{{v: v >= {b}}}]`.",
              f"Return `{xs}[0] + ... + {xs}[{k - 1}]`.",
              f"The contract `result >= {k * b}` holds because each of the "
              f"{k} summed elements is `>= {b}`."],
        signature=_assemble(doc, header, contract, "  0"),
        reference=_assemble(doc, header, contract, f"  {terms}"))


# --------------------------------------------------------------------------
# Family: all_positive -- a quantified contract over a refined list. The
# `where` uses `forall_in` over the elements of an `[Int{v: v > 0}]` list;
# the element refinement discharges the bounded quantifier.
# --------------------------------------------------------------------------
def family_all_positive(rng):
    xs = rng.choice(["xs", "items", "values", "data", "elems"])
    k = rng.randint(2, 5)
    fname = rng.choice(["all_positive", "first_k_pos", "head_all_pos",
                        "lead_positive"])
    header = f"fn {fname}({xs}: [Int{{v: v > 0}}]) -> Bool"
    contract = f"result == forall_in(i, 0, {k - 1}, {xs}[i] > 0)"
    doc = (f"Confirm the first {k} elements of {xs} are positive.",
           "Verified: the element refinement discharges the quantifier.")
    return dict(
        family="all_positive", tier="expert",
        title="Task: First-K All Positive",
        intro=f"Write `{fname}`: return whether each of the first {k} "
              f"elements of `{xs}` is greater than 0.",
        spec=[f"`{xs}` has element-refinement type `[Int{{v: v > 0}}]`.",
              f"Return `forall_in(i, 0, {k - 1}, {xs}[i] > 0)` -- a bounded "
              f"universal quantifier over the first {k} elements.",
              "The element refinement guarantees positivity, so the answer "
              "is `true`."],
        signature=_assemble(doc, header, contract, "  false"),
        reference=_assemble(doc, header, contract, "  true"))


FAMILIES = {
    "clamp": family_clamp,
    "list_sum": family_list_sum,
    "all_positive": family_all_positive,
    "compose": family_compose,
    "compose_chain": family_compose_chain,
    "recursive": family_recursive,
    "recursive_range": family_recursive_range,
    "window": family_window,
    "minmax": family_minmax,
    "abs": family_abs,
    "sign": family_sign,
    "sat_add": family_sat_add,
    "sat_sub": family_sat_sub,
    "mirror": family_mirror,
    "bucket": family_bucket,
    "adt": family_adt,
}


def generate(family: str, rng: random.Random) -> dict:
    """Build one task instance from a named family using `rng`."""
    return FAMILIES[family](rng)

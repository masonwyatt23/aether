"""Aether verified-code -- a `verifiers` RL environment.

The model writes Aether code; the Aether compiler -- an **exact**
refinement-type oracle -- scores it; the compiler's diagnostics (including
refutation counterexamples) are fed back as the next message for a retry.

Why this environment is different from every other code RL environment:
its reward is the compiler **proving** the refinement contract. A test-suite
reward or an LLM-judge reward can be reward-hacked (pass the visible tests,
fool the judge); a proof cannot. And because tasks are procedurally generated
from `aether_core`, every run trains on fresh, contamination-free instances.

Entry point: `load_environment()`. Run with `prime eval run aether-verified-code`.
"""

import asyncio
import re

import verifiers as vf
from datasets import Dataset

import aether_core

SYSTEM_PROMPT = (
    "You write Aether, a statically typed language whose compiler PROVES "
    "refinement-type `where` contracts at compile time. Given a task and a "
    "function signature with a fixed `where` contract, return the complete "
    "Aether function(s) -- the exact signature provided, with the stub body "
    "replaced by an implementation the compiler can prove. Put the code in a "
    "single fenced ```aether block; no prose."
)


def _extract_code(text: str) -> str:
    """Pull Aether source from a model reply: the first fenced block, else the
    slice from the first `fn`/`type` declaration to the last closing brace."""
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


class AetherVerifiedCodeEnv(vf.MultiTurnEnv):
    """Multi-turn: submit Aether code -> see `aether check` diagnostics ->
    retry. The verifier feedback is the whole point of the loop."""

    async def setup_state(self, state):
        # Per-rollout fields, threaded to env_response, the stop check, and
        # the reward functions.
        state["best_reward"] = 0.0
        state["best_outcome"] = "PARSE-ERROR"
        state["best_fraction"] = 0.0
        state["solved"] = False
        state["turn_count"] = 0
        # The base `setup_state` mutates in place and returns None -- match
        # that contract exactly (do not return `state`).
        await super().setup_state(state)

    @vf.stop(priority=10)
    async def proof_succeeded(self, state) -> bool:
        """Terminate the rollout as soon as the compiler verifies a submission."""
        return state.get("solved", False)

    async def env_response(self, messages, state, **kwargs):
        """Score the model's latest submission with the exact Aether oracle
        and return the next message -- either the solved confirmation or the
        compiler's diagnostics to retry against."""
        task = state["info"]
        code = _extract_code(messages[-1]["content"] if messages else "")
        # Run the (sync, subprocess-based) verifier off the event loop so
        # concurrent rollouts are not blocked.
        result = await asyncio.to_thread(aether_core.score, task, code)
        state["turn_count"] += 1
        if result["reward"] > state["best_reward"]:
            state["best_reward"] = result["reward"]
            state["best_outcome"] = result["outcome"]
            state["best_fraction"] = result["contract_fraction"]
        if result["solved"]:
            state["solved"] = True
            msg = [vf.UserMessage(content=(
                "`aether check`: VERIFIED — the compiler proved the "
                "contract for all inputs. Solved."))]
            state["final_env_response"] = msg
            return msg
        return [vf.UserMessage(content=(
            f"`aether check` did not verify that submission "
            f"(outcome: {result['outcome']}). Diagnostics:\n\n"
            f"{result['feedback']}\n\n"
            f"Fix the code and return the complete function(s)."))]


# --- reward + metrics (verifiers requests these args by name) ---------------
def aether_reward(state, **kwargs) -> float:
    """Dense reward: the best score across the rollout's turns. The
    high-water mark is monotone, so it cannot be farmed by a model that
    oscillates the diagnostics. Full 1.0 only for a compiler-proved solution."""
    return float(state.get("best_reward", 0.0))


def solved(state, **kwargs) -> float:
    """Metric (unweighted): did the compiler ever prove a submission."""
    return 1.0 if state.get("solved") else 0.0


def turns_used(state, **kwargs) -> float:
    """Metric (unweighted): verifier-feedback rounds consumed."""
    return float(state.get("turn_count", 0))


def contract_fraction(state, **kwargs) -> float:
    """Metric (unweighted): best fraction of contract conjuncts proved."""
    return float(state.get("best_fraction", 0.0))


def load_environment(num_tasks: int = 200,
                     max_turns: int = 5,
                     seed: int = 0,
                     min_tier: str = "easy",
                     max_tier: str = "expert",
                     tiers=None,
                     **kwargs) -> vf.Environment:
    """Build the environment.

    `num_tasks` fresh procedurally-generated tasks are instantiated starting
    at `seed` -- a seed maps to a never-committed instance, so training data
    is contamination-free; use disjoint `seed` ranges for train vs. eval.
    Each task allows up to `max_turns` rounds of verifier feedback.

    Difficulty curriculum: tasks are sampled from the tier window
    `[min_tier, max_tier]` (tiers: easy, medium, medium-hard, hard, expert).
    `tiers` (a list, or a single tier string) instead pins an exact set --
    a training loop can widen the window over time to ramp difficulty.
    """
    sel = None
    if tiers is not None:
        sel = [tiers] if isinstance(tiers, str) else list(tiers)
        ranks = sorted(aether_core.TIER_RANK[t] for t in sel)
        min_tier = aether_core.TIER_ORDER[ranks[0]]
        max_tier = aether_core.TIER_ORDER[ranks[-1]]
    rows, i = [], 0
    while len(rows) < num_tasks and i < num_tasks * 64:
        task = aether_core.new_task(seed + i, min_tier, max_tier)
        i += 1
        if sel is not None and task["tier"] not in sel:
            continue
        rows.append({
            "prompt": [{"role": "user",
                        "content": aether_core.build_prompt(task)}],
            "answer": task["reference"],
            "info": task,
        })
    dataset = Dataset.from_list(rows)
    rubric = vf.Rubric(
        funcs=[aether_reward, solved, turns_used, contract_fraction],
        weights=[1.0, 0.0, 0.0, 0.0])
    return AetherVerifiedCodeEnv(
        dataset=dataset,
        rubric=rubric,
        max_turns=max_turns,
        system_prompt=SYSTEM_PROMPT,
        **kwargs,
    )

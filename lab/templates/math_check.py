"""The template of an experiment's math_check.py (ADR 0118, docs/experiments/MATH-TEMPLATE.md).

Copy it to lab/experiments/EXP-NNNN/math_check.py and replace the toy with the experiment's own
results. `python -m rematch_lab.experiments check` runs it from the experiment folder, in the lab
image, and needs it to exit 0. It shows the three checks a script makes, on a toy: n games, each
won with probability p, and the win rate p̂ = ȳ with its 95% Wald interval.

1. Symbolic: each derivation's two sides, minus each other, simplify to 0 (sympy).
2. Simulation: the proof's model, simulated from a fixed seed with a known truth, gives the
   derived bias, variance and coverage within the simulation's own standard error.
3. Numbers: every number math.md, decision.md and summary.md quote, recomputed from the formulas
   and the committed results.json and power.json, within the stated rounding.

A result with a Lean proof in lab/lean/ names its theorem in `LEAN` (fourth check, `lake build`).
Each check prints one line and the script fails on the first assertion that does not hold.
"""

from __future__ import annotations

import json
import math
from pathlib import Path

import numpy as np
import sympy as sp

HERE = Path(__file__).resolve().parent
# The toy's results; an experiment reads its committed results.json and power.json instead.
TOY_RESULTS = {"n": 400, "wins": 212, "rate": 0.53, "interval": [0.4811, 0.5789]}
# Theorem names in lab/lean/ that prove a result here (none for the toy).
LEAN: dict[str, str] = {}


def load(name: str, default: dict) -> dict:
    path = HERE / name
    return json.loads(path.read_text(encoding="utf-8")) if path.is_file() else default


def symbolic() -> None:
    """Result 1: E[p̂] = p. Result 2: Var[p̂] = p(1 − p)/n."""
    p, n = sp.symbols("p n", positive=True)
    mean_one = sum(v * (p if v == 1 else 1 - p) for v in (0, 1))
    var_one = sum((v - mean_one) ** 2 * (p if v == 1 else 1 - p) for v in (0, 1))
    assert sp.simplify(mean_one - p) == 0, "Result 1: E[y] is not p"
    # The mean of n independent copies: E is linear, and variances add over independent terms.
    assert sp.simplify(n * var_one / n**2 - p * (1 - p) / n) == 0, "Result 2 does not hold"
    print("symbolic: Results 1 and 2 simplify to 0")


def simulation(seed: int = 20261128, reps: int = 20_000) -> None:
    """The model of Results 1 and 2 and the interval's coverage, with a known p."""
    rng = np.random.default_rng(seed)
    p, n = 0.3, 400
    wins = rng.binomial(n, p, size=reps)
    rate = wins / n
    # Result 1: the bias is 0 within the Monte Carlo standard error of the mean.
    se_mean = rate.std(ddof=1) / math.sqrt(reps)
    assert abs(rate.mean() - p) < 4 * se_mean, f"bias {rate.mean() - p:.5f}"
    # Result 2: the variance matches p(1 − p)/n within its own SE (√(2/(reps − 1)) relative).
    want = p * (1 - p) / n
    assert abs(rate.var(ddof=1) / want - 1) < 4 * math.sqrt(2 / (reps - 1)), "variance"
    # The 95% Wald interval covers p about 95% of the time (binomial SE of a proportion).
    half = 1.959964 * np.sqrt(rate * (1 - rate) / n)
    covered = np.mean((rate - half <= p) & (p <= rate + half))
    assert abs(covered - 0.95) < 4 * math.sqrt(0.95 * 0.05 / reps), f"coverage {covered:.4f}"
    print(f"simulation: bias, variance and coverage ({covered:.4f}) match, seed {seed}")


def numbers() -> None:
    """Every quoted number, recomputed from the formulas and the committed results."""
    r = load("results.json", TOY_RESULTS)
    rate = r["wins"] / r["n"]
    assert round(rate, 4) == r["rate"], "the rate"
    half = 1.959964 * math.sqrt(rate * (1 - rate) / r["n"])
    assert [round(rate - half, 4), round(rate + half, 4)] == r["interval"], "the interval"
    print(f"numbers: rate {r['rate']} and interval {r['interval']} recomputed")


if __name__ == "__main__":
    symbolic()
    simulation()
    numbers()
    for result, theorem in LEAN.items():
        print(f"lean: {result} is {theorem} (lab/lean/)")

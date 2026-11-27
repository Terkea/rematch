"""EXP-0029, section 6: the power of the section 9 rule, simulated end to end, and the sizing
decision, before any new month is read.

Reads only EXP-0009's committed results (development data, never a new month) and writes
`power.json` next to this file:

    python experiments/EXP-0029/power.py            # the plan (section 6)
    python experiments/EXP-0029/power.py --sizing FILE   # stage 2: the sizing decision (B4)

The simulation draws, for each replicate, what the run would compute and applies section 9:

- each quantity's split-half correlation at the run's n around an assumed true three-month
  reliability (Fisher z), its lower 95% bound, Spearman-Brown and the 0.01 floor, which set the
  read's minimum, 14 x sqrt(r_L) per observed SD;
- the nine coefficients, correlated (equicorrelation rho_beta between every pair), each with a
  true effect, the expected residual luck bias of the design and the planned standard error;
- Holm at 0.05 across the eight keys, the control's own z-test at 0.05, the simultaneous
  interval at 1 - 0.05/8 for "below the minimum effect", and the three outcomes.

The seed is `rematch_lab.experiments.seed("EXP-0029", "power")`, computed here the same way so
the script needs only the standard library, numpy and scipy.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
from pathlib import Path
from typing import Any

import numpy as np
from scipy.stats import norm

HERE = Path(__file__).resolve().parent
EXP0009 = HERE.parent / "EXP-0009" / "results.json"

KEYS = ("W4.missed", "W4.allowed", "W5", "W6", "W7.traps", "W7.prep", "W9a", "W9b")
QUANTITIES = (*KEYS, "accuracy")
CONTROL = "accuracy"

DELTA = 14.0  # Elo per true SD of the improvement over the outcome (section 5)
ALPHA = 0.05
SIMULTANEOUS = 1 - ALPHA / len(KEYS)  # keys only; the control is tested on its own (M1)
TARGET = 0.80
FLOOR = 0.01
Z_RELIABILITY = norm.ppf(0.975)  # r_L: the lower end of the 95% interval

# EXP-0009: its confirmatory slice, its games and its runtime (decision.md).
SLICE_0009 = 0.3
GAMES_READ_0009 = 877_072
PLAYERS_OBSERVED_0009 = 17_138
GAMES_PER_MONTH_0009 = GAMES_READ_0009 / PLAYERS_OBSERVED_0009 / 2  # 25.6 read a month
CPU_HOURS_PER_MILLION = 1.5 / (GAMES_READ_0009 / 1e6)  # observe + Python, one run
WALL_HOURS_PER_MILLION = (48 / 60) / (GAMES_READ_0009 / 1e6)  # one run, 8 workers
WITH_OUTCOME_SHARE_0009 = 16_204 / 17_136

# This design (section 7).
W_GRID = (0.1, 0.2, 0.3, 0.4, 0.5, 0.6)  # [0.3, 0.3 + w) inside [0.3, 0.9)
CAPS_PER_WINDOW = ((144, 120), (96, 80), (72, 60))  # (read, kept); the fallback order (B4)
OBSERVE_GAMES_BUDGET = 3_700_000
BUDGET_CAP = 1.5  # the test is at 150% of the budget: 5,550,000 games
CPU_HOURS_CAP = 25.0
WALL_HOURS_CAP = 24.0
STREAMING_CPU_HOURS = 7.0  # nine dumps at about 0.75 CPU-hours each (EXP-0009)  # from the sizing commit to results.json (S2)

# The planned nuisance inputs (each replaced by the sizing step's measurement).
PLAN = {"density": 0.7, "k": 3.0, "april_games": 20, "phi": 0.97, "rho_beta": 0.3,
        "dilution": 1 / math.sqrt(3), "truth": "point"}
SIMS = 20_000
CONTROL_EFFECTS = (5.0, 7.0, 10.0)  # Elo per true SD over the outcome (section 4)
CONTROL_PLAUSIBLE = 7.0
OUTCOME2_MIN_POWER = 0.80  # a key enters outcome 2's list if it reads "below" this often at 0


def seed(name: str) -> int:
    digest = hashlib.sha256(f"EXP-0029/{name}".encode()).digest()
    return int.from_bytes(digest[:4], "big")


def spearman_brown(rho: Any, k: float = 2.0) -> Any:
    return k * rho / (1 + (k - 1) * rho)


def half_correlation(r: float) -> float:
    return r / (2 - r)


def reliability_lower(r: float, n: float) -> float:
    rho = half_correlation(r)
    z = math.atanh(rho) - Z_RELIABILITY / math.sqrt(n - 3)
    return max(FLOOR, spearman_brown(math.tanh(z)))


# ---- the designs -------------------------------------------------------------------------------


def luck_share(design: str, games: int, phi: float) -> float:
    """The share of the luck held at F2's end that the outcome carries (an AR(1) memory)."""
    if design == "exp0009":
        return phi ** games - 1
    if design == "A":  # gap April, outcome R_end(June) - R_end(April)
        return phi ** (3 * games) - phi ** games
    if design == "B":  # gap April and May, outcome R_end(June) - R_end(May)
        return phi ** (3 * games) - phi ** (2 * games)
    raise ValueError(design)


DESIGNS = {
    # outcome months, and the error ratio against EXP-0009's one-month outcome
    "A": {"outcome_months": 2, "sd_ratio": None},  # the upper quartile, sd_ratio_of
    "B": {"outcome_months": 1, "sd_ratio": 1.0},
}


def sd_ratio_of(results: dict[str, Any]) -> dict[str, Any]:
    """EXP-0009's level-family HC3 SE (two months) over its primary's (one month), per quantity,
    and the upper quartile as the planned ratio for a two-month outcome."""
    ratios = {q: results["level"][q]["se"] / results["primary"][q]["se"] for q in QUANTITIES}
    xs = sorted(ratios.values())
    pos = 0.75 * (len(xs) - 1)
    lo = int(math.floor(pos))
    uq = xs[lo] + (xs[min(lo + 1, len(xs) - 1)] - xs[lo]) * (pos - lo)
    return {"per_quantity": ratios, "median": xs[len(xs) // 2], "planned": uq}


def luck_shift(results: dict[str, Any]) -> dict[str, float]:
    """EXP-0009's primary minus its adjusted estimate per observed SD: what September's own
    rating change took out, read as the luck bias of EXP-0009's baseline (it also takes out
    September's catch-up, so it is an estimate of the bias net of that, development data)."""
    return {q: results["primary"][q]["estimate"] - results["adjusted"][q]["estimate"]
            for q in QUANTITIES}


def biases(results: dict[str, Any], design: str, games: int, phi: float,
           dilution: float) -> dict[str, float]:
    """The expected residual luck bias per observed SD: EXP-0009's shift, scaled by the memory
    ratio of the design against EXP-0009's and by the window's dilution (only about the last
    month of a three-month window's games is still in F2's end rating, a third of the covariance
    over 1/sqrt(3) of the SD: 1/sqrt(3))."""
    ratio = luck_share(design, games, phi) / luck_share("exp0009", games, phi)
    return {q: s * ratio * dilution for q, s in luck_shift(results).items()}


def inputs(results: dict[str, Any], design: str, w: float, density: float, k: float,
           truth: str, sd_ratio: float | None = None) -> dict[str, dict[str, float]]:
    """Per quantity: n, the planned SE, and the true three-month reliability assumed."""
    ratio = sd_ratio if sd_ratio is not None else (
        DESIGNS[design]["sd_ratio"] or sd_ratio_of(results)["planned"])
    out = {}
    for q in QUANTITIES:
        f = results["primary"][q]
        n0, r0 = float(f["n"]), float(f["reliability"])
        base = r0 if truth == "point" else reliability_lower(r0, n0)
        n = n0 / SLICE_0009 * w * density
        out[q] = {
            "n": n,
            "se": float(f["se"]) * math.sqrt(n0 / n) * ratio,
            "reliability_true": spearman_brown(base, k),
            "reliability_0009": r0,
        }
    return out


# ---- the rule, simulated -----------------------------------------------------------------------


def simulate(inp: dict[str, dict[str, float]], effects: dict[str, float],
             bias: dict[str, float], rho_beta: float, keys2: tuple[str, ...],
             rng: np.random.Generator, sims: int = SIMS) -> dict[str, Any]:
    """Section 9 on `sims` replicates. `effects` are Elo per true SD over the outcome; the
    coefficient per observed SD is effect x sqrt(true reliability) + bias."""
    qs = list(QUANTITIES)
    m = len(qs)
    corr = np.full((m, m), rho_beta) + (1 - rho_beta) * np.eye(m)
    L = np.linalg.cholesky(corr)
    se = np.array([inp[q]["se"] for q in qs])
    r_true = np.array([inp[q]["reliability_true"] for q in qs])
    n = np.array([inp[q]["n"] for q in qs])
    mu = np.array([effects.get(q, 0.0) * math.sqrt(inp[q]["reliability_true"]) + bias.get(q, 0.0)
                   for q in qs])
    beta = mu + (rng.standard_normal((sims, m)) @ L.T) * se
    # The run's own reliability read: Fisher z at its n, the lower bound, Spearman-Brown, floor.
    rho_true = r_true / (2 - r_true)
    zhat = np.arctanh(rho_true) + rng.standard_normal((sims, m)) / np.sqrt(n - 3)
    rho_low = np.tanh(zhat - Z_RELIABILITY / np.sqrt(n - 3))
    r_low = np.maximum(FLOOR, spearman_brown(rho_low))
    minimum = DELTA * np.sqrt(r_low)
    z_sim = norm.ppf(1 - (1 - SIMULTANEOUS) / 2)
    below = (np.abs(beta) + z_sim * se) < minimum
    p = 2 * norm.sf(np.abs(beta) / se)
    ki = [qs.index(k) for k in KEYS]
    pk = p[:, ki]
    order = np.argsort(pk, axis=1)
    sorted_p = np.take_along_axis(pk, order, axis=1)
    thresholds = ALPHA / (len(KEYS) - np.arange(len(KEYS)))
    passed = np.cumprod(sorted_p <= thresholds, axis=1).astype(bool)
    reject = np.zeros_like(passed)
    np.put_along_axis(reject, order, passed, axis=1)
    climbing = reject & (beta[:, ki] > 0)
    ci = qs.index(CONTROL)
    control = (p[:, ci] < ALPHA) & (beta[:, ci] > 0)
    out1 = climbing.any(axis=1)
    k2 = [qs.index(k) for k in keys2]
    all_below = below[:, k2].all(axis=1) if k2 else np.ones(sims, dtype=bool)
    out2 = ~out1 & all_below & control
    return {
        "outcome1": float(out1.mean()),
        "outcome2": float(out2.mean()),
        "outcome3": float(1 - out1.mean() - out2.mean()),
        "control_holds": float(control.mean()),
        "per_key": {k: {"climbing": float(climbing[:, j].mean()),
                        "below": float(below[:, qs.index(k)].mean())}
                    for j, k in enumerate(KEYS)},
    }


def outcome2_keys(inp: dict[str, dict[str, float]], bias: dict[str, float],
                  rho_beta: float, rng: np.random.Generator) -> tuple[str, ...]:
    """The keys the simulated rule reads "below the minimum effect" at least 80% of the time
    when their true effect is 0 (with the design's bias): outcome 2's list (B1)."""
    sim = simulate(inp, {}, bias, rho_beta, (), rng)
    return tuple(k for k in KEYS if sim["per_key"][k]["below"] >= OUTCOME2_MIN_POWER)


def single_key_power(inp: dict[str, dict[str, float]], bias: dict[str, float],
                     rho_beta: float, rng: np.random.Generator) -> dict[str, float]:
    """For each key alone at the minimum (14 per true SD), the chance of outcome 1 for it."""
    out = {}
    for k in KEYS:
        sim = simulate(inp, {k: DELTA}, bias, rho_beta, (), rng, sims=SIMS // 4)
        out[k] = sim["per_key"][k]["climbing"]
    return out


# ---- the sizing decision (B4) ------------------------------------------------------------------


def games_at(w: float, density: float, k: float, cap: tuple[int, int]) -> float:
    players = PLAYERS_OBSERVED_0009 / SLICE_0009 * w * density
    return players * 2 * min(cap[0], k * GAMES_PER_MONTH_0009)


def costs(games: float, streaming_cpu_hours: float) -> dict[str, float]:
    return {
        "observe_games": games,
        "cpu_hours": streaming_cpu_hours + 2 * games / 1e6 * CPU_HOURS_PER_MILLION,
        "wall_hours": 2 * games / 1e6 * WALL_HOURS_PER_MILLION + 8.0,  # + 8 h: code and checks
    }


def decide_size(results: dict[str, Any], measured: dict[str, float], design: str = "A"
                ) -> dict[str, Any]:
    """The registered sizing decision: the smallest w (and the largest per-window cap, in the
    fallback order) at which (a) the control holds with probability >= 0.80 at 7 Elo per true SD
    after the expected bias, (b) outcome 2's list (from the simulation) has at least four keys,
    each of which alone at 14 Elo per true SD reads "precedes climbing" with probability >= 0.80,
    and (c) the games, CPU-hours and wall time stay within their caps. `measured` holds density,
    k (the harmonic-mean ratio, S4), april_games (median rapid games in the gap month), sd_ratio
    and streaming_cpu_hours. None found: the kill rule."""
    rng = np.random.default_rng(seed("sizing"))
    bias = biases(results, design, int(measured["april_games"]), PLAN["phi"], PLAN["dilution"])
    tried = []
    for cap in CAPS_PER_WINDOW:
        k = measured["k"] * cap[1] / CAPS_PER_WINDOW[0][1] if cap != CAPS_PER_WINDOW[0] \
            else measured["k"]
        for w in W_GRID:
            inp = inputs(results, design, w, measured["density"], k, PLAN["truth"],
                         measured.get("sd_ratio"))
            keys2 = outcome2_keys(inp, bias, PLAN["rho_beta"], rng)
            ctrl = simulate(inp, {CONTROL: CONTROL_PLAUSIBLE}, bias, PLAN["rho_beta"], (),
                            rng)["control_holds"]
            single = single_key_power(inp, bias, PLAN["rho_beta"], rng)
            single_min = min((single[q] for q in keys2), default=0.0)
            c = costs(games_at(w, measured["density"], k, cap), measured["streaming_cpu_hours"])
            ok = (ctrl >= TARGET and len(keys2) >= 4 and single_min >= TARGET
                  and c["observe_games"] <= OBSERVE_GAMES_BUDGET * BUDGET_CAP
                  and c["cpu_hours"] <= CPU_HOURS_CAP and c["wall_hours"] <= WALL_HOURS_CAP)
            tried.append({"cap": list(cap), "w": w, "control_holds": ctrl,
                          "single_key_min_power": single_min,
                          "outcome2_keys": list(keys2), **c, "ok": ok})
            if ok:
                return {"run": True, "w": w, "cap": list(cap), "outcome2_keys": list(keys2),
                        "tried": tried}
    return {"run": False, "reason": "no w and cap reach the target within the caps",
            "tried": tried}


# ---- the plan ----------------------------------------------------------------------------------


def null_effects(results: dict[str, Any], control: float) -> dict[str, float]:
    return {CONTROL: control}


def dev_effects(results: dict[str, Any], control: float) -> dict[str, float]:
    """The keys at EXP-0009's adjusted estimate per true SD (development data, an optimistic
    reading of a lasting link), the control at `control`."""
    eff = {k: results["adjusted"][k]["estimate"] / math.sqrt(results["primary"][k]["reliability"])
           for k in KEYS}
    eff[CONTROL] = control
    return eff


def plan(results: dict[str, Any]) -> dict[str, Any]:
    rng = np.random.default_rng(seed("power"))
    out: dict[str, Any] = {}
    for design in ("A", "B"):
        scale = DESIGNS[design]["outcome_months"] / DESIGNS["A"]["outcome_months"]
        d: dict[str, Any] = {}
        for games in (20, 40):
            bias = biases(results, design, games, PLAN["phi"], PLAN["dilution"])
            for w in (0.5, 0.6):
                inp = inputs(results, design, w, PLAN["density"], PLAN["k"], PLAN["truth"])
                keys2 = outcome2_keys(inp, bias, PLAN["rho_beta"], rng)
                cell: dict[str, Any] = {"bias": bias, "outcome2_keys": list(keys2),
                                        "inputs": inp}
                for c in CONTROL_EFFECTS:
                    ce = c * scale  # a lasting trend: Elo over the outcome scales with its length
                    cell[f"null_control{c:g}"] = simulate(
                        inp, null_effects(results, ce), bias, PLAN["rho_beta"], keys2, rng)
                    cell[f"dev_control{c:g}"] = simulate(
                        inp, {k: v * scale for k, v in dev_effects(results, c).items()},
                        bias, PLAN["rho_beta"], keys2, rng)
                cell["single_key_at_minimum"] = single_key_power(inp, bias, PLAN["rho_beta"], rng)
                cell["costs"] = costs(games_at(w, PLAN["density"], PLAN["k"],
                                               CAPS_PER_WINDOW[0]), STREAMING_CPU_HOURS)
                d[f"april{games}_w{w:g}"] = cell
        out[design] = d
    # Sensitivity of design A at w = 0.6, 20 games: the truth at the lower bound; rho_beta.
    sens = {}
    bias = biases(results, "A", 20, PLAN["phi"], PLAN["dilution"])
    for truth in ("point", "lower"):
        for rb in (0.0, 0.3, 0.6):
            inp = inputs(results, "A", 0.6, PLAN["density"], PLAN["k"], truth)
            keys2 = outcome2_keys(inp, bias, rb, rng)
            s = simulate(inp, null_effects(results, CONTROL_PLAUSIBLE), bias, rb, keys2, rng)
            sens[f"truth={truth},rho_beta={rb:g}"] = {"outcome2_keys": list(keys2), **s}
    # The bias-free check: design A, w = 0.6, no residual bias (what stage 1 assumed).
    inp = inputs(results, "A", 0.6, PLAN["density"], PLAN["k"], "point")
    keys2 = outcome2_keys(inp, {}, PLAN["rho_beta"], rng)
    no_bias = simulate(inp, null_effects(results, CONTROL_PLAUSIBLE), {}, PLAN["rho_beta"],
                       keys2, rng)
    # The gap month's activity decides the control (B3): design A at w = 0.5 and 0.6.
    sweep = {}
    for w in (0.3, 0.4, 0.5, 0.6):
        inp = inputs(results, "A", w, PLAN["density"], PLAN["k"], "point")
        for games in (20, 25, 30, 35, 40, 60):
            bias = biases(results, "A", games, PLAN["phi"], PLAN["dilution"])
            keys2 = outcome2_keys(inp, bias, PLAN["rho_beta"], rng)
            row = {"bias_control": bias[CONTROL], "outcome2_keys": list(keys2)}
            for c in CONTROL_EFFECTS:
                sim = simulate(inp, null_effects(results, c), bias, PLAN["rho_beta"], keys2, rng)
                row[f"control{c:g}"] = {k: sim[k] for k in
                                        ("outcome1", "outcome2", "outcome3", "control_holds")}
            sweep[f"w{w:g}_april{games}"] = row
    return {"designs": out, "sensitivity_A_w0.6_april20": sens, "april_sweep_A": sweep,
            "no_bias_A_w0.6": {"outcome2_keys": list(keys2), **no_bias}}


def memory_table() -> list[dict[str, Any]]:
    rows = []
    for phi in (0.97, 0.98):
        for g in (20, 40, 60):
            def mean_share(start: int, games: int) -> float:
                return sum(phi ** t for t in range(start + 1, start + games + 1)) / games

            rows.append({
                "phi": phi, "rapid_games_per_month": g,
                "exp0009": luck_share("exp0009", g, phi),
                "gap_end": luck_share("A", g, phi),
                "gap2_end": luck_share("B", g, phi),
                "mean_after": mean_share(g, 2 * g) - mean_share(0, g),
                "mean_next": mean_share(0, g) - 1,
                "left_after_gap": phi ** g,
            })
    return rows


def stage2(results: dict[str, Any]) -> dict[str, Any]:
    """The players the median eval-only key needs for 0.80 in both closed-form powers (at the
    point reliability, k = 3, design A), times our own analysis at 1 million nodes."""
    inp = inputs(results, "A", 1.0, PLAN["density"], PLAN["k"], "point")
    z = norm.ppf(1 - (1 - SIMULTANEOUS) / 2) + norm.ppf(1 - (1 - TARGET) / 2)
    need = sorted((z * f["se"] * math.sqrt(f["n"]) / (DELTA * math.sqrt(f["reliability_true"])))
                  ** 2 for q, f in inp.items() if q in KEYS)
    n2 = (need[3] + need[4]) / 2
    return {"players_for_median_key": n2, "games_per_player": 240,
            "cpu_hours": n2 * 240 * 39.1 / 3600, "max_cpu_hours": 20.0, "runs": False}


def round_floats(x: Any) -> Any:
    if isinstance(x, float):
        return round(x, 4)
    if isinstance(x, dict):
        return {k: round_floats(v) for k, v in x.items()}
    if isinstance(x, list):
        return [round_floats(v) for v in x]
    return x


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--sizing", type=Path, help="the measured nuisance inputs (stage 2)")
    args = parser.parse_args()
    results = json.loads(EXP0009.read_text(encoding="utf-8"))
    if args.sizing:
        measured = json.loads(args.sizing.read_text(encoding="utf-8"))
        print(json.dumps(round_floats(decide_size(results, measured)), indent=2, sort_keys=True))
        return
    out = {
        "experiment": "EXP-0029",
        "inputs": "EXP-0009 results.json (development data)",
        "plan": PLAN,
        "seeds": ["power", "sizing"],
        "sd_ratio_from_exp0009": sd_ratio_of(results),
        "luck_shift_0009": luck_shift(results),
        "memory": memory_table(),
        "simulation": plan(results),
        "sizing_at_plan": decide_size(results, {
            "density": PLAN["density"], "k": PLAN["k"], "april_games": PLAN["april_games"],
            "streaming_cpu_hours": STREAMING_CPU_HOURS}),
        "stage2": stage2(results),
    }
    (HERE / "power.json").write_text(
        json.dumps(round_floats(out), indent=2, sort_keys=True, allow_nan=False) + "\n",
        encoding="utf-8")


if __name__ == "__main__":
    main()

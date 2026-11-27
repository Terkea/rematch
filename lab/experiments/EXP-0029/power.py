"""EXP-0029, section 6: the power of the section 9 rule, simulated end to end, and the sizing
decision, before any new month is read.

Reads EXP-0009's committed results and `dev/exp09_activity.json` (activity measured on EXP-0009's
own players files, development data; never a month of EXP-0029) and writes `power.json`:

    python experiments/EXP-0029/power.py                  # the plan (section 6)
    python experiments/EXP-0029/power.py --sizing FILE    # stage 2: the sizing decision

The simulation draws, for each replicate, what the run would compute and applies section 9:

- each quantity's split-half correlation at the run's n around an assumed true three-month
  reliability (Fisher z), its lower 95% bound, Spearman-Brown and the 0.01 floor, which set the
  read's minimum, 14 x sqrt(r_L) per observed SD;
- the nine coefficients, correlated (equicorrelation rho_beta between every pair), each with a
  true effect, the expected residual luck bias of the design and the planned standard error;
- Holm at 0.05 across the eight keys, the control's own z-test at 0.05, the simultaneous
  interval at 1 - 0.05/8 for "below the minimum effect", and the three outcomes, outcome 2
  needing the five registered keys.

The seeds are `rematch_lab.experiments.seed("EXP-0029", "power")` and `(..., "sizing")`,
computed here the same way so the script needs only the standard library, numpy and scipy.
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
ACTIVITY = HERE / "dev" / "exp09_activity.json"

KEYS = ("W4.missed", "W4.allowed", "W5", "W6", "W7.traps", "W7.prep", "W9a", "W9b")
QUANTITIES = (*KEYS, "accuracy")
CONTROL = "accuracy"
# Outcome 2's list, fixed at the round 2 commit (section 9) and never recomputed (N4).
OUTCOME2_KEYS = ("W4.allowed", "W5", "W6", "W7.traps", "W9a")

DELTA = 14.0  # Elo per true SD of the improvement over the outcome (section 5)
ALPHA = 0.05
SIMULTANEOUS = 1 - ALPHA / len(KEYS)  # keys only; the control is tested on its own (M1)
TARGET = 0.80
FLOOR = 0.01
Z_RELIABILITY = norm.ppf(0.975)  # r_L: the lower end of the 95% interval

# EXP-0009: its confirmatory slice and its runtime (decision.md).
SLICE_0009 = 0.3
GAMES_READ_0009 = 877_072
PLAYERS_OBSERVED_0009 = 17_138
CPU_HOURS_PER_MILLION = 1.5 / (GAMES_READ_0009 / 1e6)  # observe + Python, one run
WALL_HOURS_PER_MILLION = (48 / 60) / (GAMES_READ_0009 / 1e6)  # one run, 8 workers

# This design (section 7).
W_GRID = (0.1, 0.2, 0.3, 0.4, 0.5, 0.6)  # [0.3, 0.3 + w) inside [0.3, 0.9)
CAPS_PER_WINDOW = ((144, 120), (96, 80), (72, 60))  # (read, kept); the fallback order
OBSERVE_GAMES_BUDGET = 3_700_000
BUDGET_CAP = 1.5  # the test is at 150% of the budget: 5,550,000 games
CPU_HOURS_CAP = 25.0
WALL_HOURS_CAP = 24.0  # from the sizing commit to results.json (S2)
STREAMING_CPU_HOURS = 7.0  # nine dumps at about 0.75 CPU-hours each (EXP-0009)
PHI = (0.97, 0.98)  # the gate uses the worse of the two (N1)

PLAN = {"density": 0.7, "rho_beta": 0.3, "dilution": 1 / math.sqrt(3), "truth": "point"}
SIMS = 20_000
CONTROL_EFFECTS = (5.0, 7.0, 10.0)  # Elo per true SD over the outcome (section 4)
CONTROL_PLAUSIBLE = 7.0
BIAS_MULTIPLIERS = (1.0, 1.5, 2.0)  # the bias model wrong by these factors (sensitivity)


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


# ---- the inputs --------------------------------------------------------------------------------


def luck_share(design: str, games: int, phi: float) -> float:
    """The share of the luck held at F2's end that an outcome carries (an AR(1) memory) for a
    player with `games` rapid games a month: EXP-0009's one-month outcome, this design (A: gap
    April, outcome to June) or a two-month gap (B: the outcome June alone)."""
    if design == "exp0009":
        return phi ** games - 1
    if design == "A":
        return phi ** (3 * games) - phi ** games
    if design == "B":
        return phi ** (3 * games) - phi ** (2 * games)
    raise ValueError(design)


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
    """EXP-0009's primary minus its adjusted estimate per observed SD, read as the luck bias of
    its baseline (it also removes September's catch-up: an estimate, development data)."""
    return {q: results["primary"][q]["estimate"] - results["adjusted"][q]["estimate"]
            for q in QUANTITIES}


def memory_ratio(activity: dict[str, Any], april_share: dict[str, float]) -> dict[str, float]:
    """Per phi, the mean per-player luck share of this design's outcome (`april_share`, over
    players with an outcome) over EXP-0009's, measured on its own players' October games (N1,
    N5): a mean of per-player shares, so the low-games tail counts by its weight."""
    return {f"{phi:g}": april_share[f"{phi:g}"] / activity["g0"][f"E_share_exp0009_phi{phi:g}"]
            for phi in PHI}


def worse_ratio(ratios: dict[str, float]) -> float:
    return max(ratios.values())


def biases(results: dict[str, Any], ratio: float, dilution: float,
           mult: float = 1.0) -> dict[str, float]:
    """The expected residual luck bias per observed SD: EXP-0009's shift x the memory ratio x
    the window's dilution (1/sqrt(3): about the last month of a three-month window is still in
    F2's end rating) x `mult`."""
    return {q: s * ratio * dilution * mult for q, s in luck_shift(results).items()}


def k_of(activity: dict[str, Any], e_inv_kept: float) -> float:
    """Spearman-Brown's k: EXP-0009's E[1/m] at its 40 a month over this design's E[1/m] at
    the kept cap, both measured (S4, N3)."""
    return activity["window_proxy"]["E_inv_m_exp0009_cap40"] / e_inv_kept


def inputs(results: dict[str, Any], w: float, density: float, k: float, truth: str,
           sd_ratio: float) -> dict[str, dict[str, float]]:
    """Per quantity: n, the planned SE, and the true three-month reliability assumed."""
    out = {}
    for q in QUANTITIES:
        f = results["primary"][q]
        n0, r0 = float(f["n"]), float(f["reliability"])
        base = r0 if truth == "point" else reliability_lower(r0, n0)
        n = n0 / SLICE_0009 * w * density
        out[q] = {
            "n": n,
            "se": float(f["se"]) * math.sqrt(n0 / n) * sd_ratio,
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


def single_key_power(inp: dict[str, dict[str, float]], bias: dict[str, float],
                     rho_beta: float, rng: np.random.Generator) -> dict[str, float]:
    """For each key alone at the minimum (14 per true SD), the chance of outcome 1 for it."""
    out = {}
    for k in KEYS:
        sim = simulate(inp, {k: DELTA}, bias, rho_beta, (), rng, sims=SIMS // 4)
        out[k] = sim["per_key"][k]["climbing"]
    return out


# ---- the sizing decision -----------------------------------------------------------------------


def costs(games: float, streaming_cpu_hours: float) -> dict[str, float]:
    return {
        "observe_games": games,
        "cpu_hours": streaming_cpu_hours + 2 * games / 1e6 * CPU_HOURS_PER_MILLION,
        "wall_hours": 2 * games / 1e6 * WALL_HOURS_PER_MILLION + 8.0,  # + 8 h: checks, write-up
    }


def players_at(w: float, density: float) -> float:
    return PLAYERS_OBSERVED_0009 / SLICE_0009 * w * density


def target(inp: dict[str, dict[str, float]], bias: dict[str, float],
           rng: np.random.Generator) -> dict[str, Any]:
    """The target at one size: each of the five registered keys reads "below" >= 0.80 at 0 and,
    alone at the minimum, "precedes climbing" >= 0.80; the control holds >= 0.80 at 7 Elo per
    true SD; all after the bias (N4)."""
    null = simulate(inp, {}, bias, PLAN["rho_beta"], OUTCOME2_KEYS, rng)
    below = {k: null["per_key"][k]["below"] for k in OUTCOME2_KEYS}
    single = single_key_power(inp, bias, PLAN["rho_beta"], rng)
    ctrl = simulate(inp, {CONTROL: CONTROL_PLAUSIBLE}, bias, PLAN["rho_beta"], (),
                    rng)["control_holds"]
    ok = (min(below.values()) >= TARGET and min(single[k] for k in OUTCOME2_KEYS) >= TARGET
          and ctrl >= TARGET)
    return {"ok": ok, "below": below, "single": single, "control_holds": ctrl}


def decide_size(results: dict[str, Any], measured: dict[str, Any]) -> dict[str, Any]:
    """The registered sizing decision (N2, N3, N4). `measured`, from the players files only:
    `density` (eligible players per unit of hash over EXP-0009's 57,127), `caps` (for each
    "read/kept" cap, E[min(m, read)] and E[1/min(m, kept)] per window over the eligible),
    `april_share` (for phi 0.97 and 0.98, the mean over players with an outcome of
    phi^g_A (phi^(2 g_J) - 1), g_A April's and g_J June's rapid games), `sd_ratio` (the SD of y
    over EXP-0009's 53.8) and `streaming_cpu_hours` (from the logs).

    For each cap in the fallback order, the largest w of the grid whose games, CPU-hours and
    wall time fit the caps is the size, and the target is tested there, at the worse of the two
    phi. It passes: that is the design. It fails: the kill rule. A smaller cap is tried only
    when no w fits the caps, since it lowers the reliability."""
    activity = json.loads(ACTIVITY.read_text(encoding="utf-8"))
    rng = np.random.default_rng(seed("sizing"))
    ratios = memory_ratio(activity, measured["april_share"])
    bias = biases(results, worse_ratio(ratios), PLAN["dilution"])
    tried = []
    for read, kept in CAPS_PER_WINDOW:
        cap = measured["caps"][f"{read}/{kept}"]
        k = k_of(activity, cap["E_inv_min_kept"])
        for w in sorted(W_GRID, reverse=True):
            games = players_at(w, measured["density"]) * 2 * cap["E_min_read"]
            c = costs(games, measured["streaming_cpu_hours"])
            fits = (c["observe_games"] <= OBSERVE_GAMES_BUDGET * BUDGET_CAP
                    and c["cpu_hours"] <= CPU_HOURS_CAP and c["wall_hours"] <= WALL_HOURS_CAP)
            row = {"cap": [read, kept], "w": w, "k": k, **c, "fits": fits}
            if not fits:
                tried.append(row)
                continue
            inp = inputs(results, w, measured["density"], k, PLAN["truth"], measured["sd_ratio"])
            t = target(inp, bias, rng)
            tried.append({**row, **t})
            out = {"run": t["ok"], "w": w, "cap": [read, kept], "k": k,
                   "memory_ratio": ratios, "bias": bias, "tried": tried}
            if not t["ok"]:
                out["reason"] = "the target fails at the largest size within the caps"
            return out
    return {"run": False, "reason": "no size fits the caps", "memory_ratio": ratios,
            "tried": tried}


# ---- the plan ----------------------------------------------------------------------------------


def dev_effects(results: dict[str, Any], control: float) -> dict[str, float]:
    """The keys at EXP-0009's adjusted estimate per true SD (development data, an optimistic
    reading: it still holds part of the catch-up), the control at `control`."""
    eff = {k: results["adjusted"][k]["estimate"] / math.sqrt(results["primary"][k]["reliability"])
           for k in KEYS}
    eff[CONTROL] = control
    return eff


def outcomes(inp: dict[str, dict[str, float]], effects: dict[str, float],
             bias: dict[str, float], rng: np.random.Generator) -> dict[str, float]:
    s = simulate(inp, effects, bias, PLAN["rho_beta"], OUTCOME2_KEYS, rng)
    return {k: s[k] for k in ("outcome1", "outcome2", "outcome3", "control_holds")}


def plan_measured(results: dict[str, Any], activity: dict[str, Any]) -> dict[str, Any]:
    """The sizing inputs as EXP-0009's own cohort gives them (development data): its October
    games standing in for April and for each of May and June, its August to October analysed
    games for a window, and the planned density and error ratio."""
    return {
        "density": PLAN["density"],
        "caps": activity["window_proxy"]["caps"],
        "april_share": {f"{phi:g}": activity["g0"][f"E_share_A_proxy_phi{phi:g}"] for phi in PHI},
        "sd_ratio": sd_ratio_of(results)["planned"],
        "streaming_cpu_hours": STREAMING_CPU_HOURS,
    }


def plan(results: dict[str, Any], activity: dict[str, Any]) -> dict[str, Any]:
    rng = np.random.default_rng(seed("power"))
    measured = plan_measured(results, activity)
    sizing = decide_size(results, measured)
    w, (read, kept), k = sizing["w"], sizing["cap"], sizing["k"]
    ratio = worse_ratio(sizing["memory_ratio"])
    cells: dict[str, Any] = {}
    for truth in ("point", "lower"):
        inp = inputs(results, w, measured["density"], k, truth, measured["sd_ratio"])
        for mult in BIAS_MULTIPLIERS:
            bias = biases(results, ratio, PLAN["dilution"], mult)
            for c in CONTROL_EFFECTS:
                cells[f"truth={truth},bias_x{mult:g},null_control{c:g}"] = outcomes(
                    inp, {CONTROL: c}, bias, rng)
            cells[f"truth={truth},bias_x{mult:g},dev_control7"] = outcomes(
                inp, dev_effects(results, CONTROL_PLAUSIBLE), bias, rng)
    # The memory ratio at which the gate closes, at this size.
    inp = inputs(results, w, measured["density"], k, "point", measured["sd_ratio"])
    gate_by_ratio = {}
    for r in (0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.8, 1.0):
        t = target(inp, biases(results, r, PLAN["dilution"]), rng)
        gate_by_ratio[f"{r:g}"] = {"ok": t["ok"], "control_holds": t["control_holds"]}
    games = players_at(w, measured["density"]) * 2 * measured["caps"][f"{read}/{kept}"]["E_min_read"]
    return {"measured": measured, "sizing": sizing, "outcomes_at_w": cells,
            "gate_by_memory_ratio": gate_by_ratio,
            "costs_at_w": costs(games, STREAMING_CPU_HOURS),
            "players_at_w": players_at(w, measured["density"])}


def memory_table() -> list[dict[str, Any]]:
    rows = []
    for phi in PHI:
        for g in (20, 40, 60, 120):
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


def stage2(results: dict[str, Any], k: float) -> dict[str, Any]:
    """The players the median eval-only key needs for 0.80 in both closed-form powers (point
    reliability, the planned k), times our own analysis at 1 million nodes."""
    inp = inputs(results, 1.0, PLAN["density"], k, "point", sd_ratio_of(results)["planned"])
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
    activity = json.loads(ACTIVITY.read_text(encoding="utf-8"))
    p = plan(results, activity)
    out = {
        "experiment": "EXP-0029",
        "inputs": "EXP-0009 results.json and dev/exp09_activity.json (development data)",
        "plan": PLAN,
        "outcome2_keys": list(OUTCOME2_KEYS),
        "seeds": ["power", "sizing"],
        "sd_ratio_from_exp0009": sd_ratio_of(results),
        "luck_shift_0009": luck_shift(results),
        "memory": memory_table(),
        "simulation": p,
        "stage2": stage2(results, p["sizing"]["k"]),
    }
    (HERE / "power.json").write_text(
        json.dumps(round_floats(out), indent=2, sort_keys=True, allow_nan=False) + "\n",
        encoding="utf-8")


if __name__ == "__main__":
    main()

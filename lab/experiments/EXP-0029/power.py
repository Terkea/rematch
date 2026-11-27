"""EXP-0029, section 6: the planning power analysis, before any new month is read.

Reads only EXP-0009's committed results (development data, never a new month) and writes
`power.json` next to this file:

    python experiments/EXP-0029/power.py

What it does, in the order of section 6 of the pre-registration:

1. For each of EXP-0009's nine quantities, the split-half reliability of the month-to-month
   change at the lower end of its 95% interval: the half correlation rho = r / (2 - r) behind
   EXP-0009's Spearman-Brown value r, its Fisher-z interval at the players the model used, and
   Spearman-Brown again, floored at 0.01 as EXP-0009's analysis does.
2. Spearman-Brown for k times the games (k = 3 for three-month windows), which gives the
   reliability this design is planned at, and the minimum effect per observed SD the read will
   use: 14 x sqrt(reliability).
3. The standard error at n players: EXP-0009's confirmatory HC3 standard error, scaled by
   sqrt(n_0009 / n) and by how much wider a two-month outcome's error is than a one-month one's
   (planned at the upper quartile of EXP-0009's own level-against-primary ratios, with 1.0 to
   2.0 as the sensitivity).
4. The players at hash width w: EXP-0009's players per unit of hash for that quantity, times the
   density of the three-month eligibility rule against EXP-0009's (planned at 0.7, an
   assumption; the counts from the players files set it before any game is observed).
5. The two powers EXP-0009 registered (Bonferroni at 1 - 0.05/9 standing in for Holm), the
   smallest w of the grid that reaches the target, the games to observe, and stage 2's price.
6. The rating-memory table behind the choice of baseline (section 4): how much of the exposure
   games' luck, and of the rating's catch-up, each candidate outcome keeps.

Nothing here is random, so there are no seeds.
"""

from __future__ import annotations

import json
import math
from pathlib import Path
from typing import Any

from scipy.stats import norm

HERE = Path(__file__).resolve().parent
EXP0009 = HERE.parent / "EXP-0009" / "results.json"

KEYS = ("W4.missed", "W4.allowed", "W5", "W6", "W7.traps", "W7.prep", "W9a", "W9b")
QUANTITIES = (*KEYS, "accuracy")
CONTROL = "accuracy"

DELTA = 14.0  # Elo per true SD (EXP-0009 section 5; ADR 0101's 0.02 points a game)
ALPHA = 0.05
SIMULTANEOUS = 1 - ALPHA / len(QUANTITIES)
TARGET = 0.80
KEYS_NEEDED = 6  # outcome 2 needs six of the eight keys "below the minimum effect"
FLOOR = 0.01
RELIABILITY_LEVEL = 0.95  # the lower end of the 95% interval

# EXP-0009's confirmatory slice and its mean games observed per player and month.
SLICE_0009 = 0.3
GAMES_READ_0009 = 877_072
PLAYERS_OBSERVED_0009 = 17_138
GAMES_PER_MONTH_0009 = GAMES_READ_0009 / PLAYERS_OBSERVED_0009 / 2  # 25.6
WITH_OUTCOME_SHARE_0009 = 16_204 / 17_136

# This design (section 7).
MONTHS_PER_WINDOW = 3
OBSERVE_PER_WINDOW = 144  # read by game hash
KEEP_PER_WINDOW = 120  # kept, 3 x EXP-0009's 40
W_GRID = (0.1, 0.2, 0.3, 0.4, 0.5, 0.6)  # inside [0.3, 0.9): EXP-0009's slices are excluded
PLAN = {"k": 3.0, "density": 0.7}  # sd_ratio: from EXP-0009's two-month model, below
SENSITIVITY = {
    "k": (2.0, 3.0),
    "density": (0.5, 0.7, 1.0),
    "sd_ratio": (1.0, 1.25, 1.5, 2.0),
}
OBSERVE_GAMES_PER_SECOND = 360.0  # per process, EXP-0008's October extract
CONTROL_TRUE_EFFECTS = (5.0, 7.0, 10.0, 14.0)  # Elo per true SD, the control's sensitivity

# Stage 2 (EXP-0009 section 6): our own analysis at 1 million nodes, EXP-0008's rate.
STAGE2_CPU_SECONDS_PER_GAME = 39.1
STAGE2_MAX_CPU_HOURS = 20.0

# The rating's memory: the share of a rating's error left after one more game (EXP-0015's
# Glicko-2 simulation measured a game-to-game correlation of 0.97; an established Lichess
# account at the RD floor forgets more slowly, about 0.983).
PHI = (0.97, 0.98)
GAMES_PER_MONTH = (20, 40, 60)


def spearman_brown(rho: float, k: float = 2.0) -> float:
    return k * rho / (1 + (k - 1) * rho)


def half_correlation(r: float) -> float:
    """The rho behind a two-half Spearman-Brown value r = 2 rho / (1 + rho)."""
    return r / (2 - r)


def reliability_lower(r: float, n: int) -> float:
    """r at the lower end of its 95% interval: Fisher z on the half correlation, then
    Spearman-Brown, floored as EXP-0009 floors it."""
    rho = half_correlation(r)
    z = math.atanh(rho) - norm.ppf(1 - (1 - RELIABILITY_LEVEL) / 2) / math.sqrt(n - 3)
    return max(FLOOR, spearman_brown(math.tanh(z)))


def powers(d: float, se: float) -> dict[str, float]:
    """EXP-0009's two registered powers at a minimum d per observed SD and a standard error se."""
    z = norm.ppf(1 - (1 - SIMULTANEOUS) / 2)
    climb = float(norm.cdf(d / se - z))
    small = float(max(0.0, 2 * norm.cdf(d / se - z) - 1))
    return {"power_climbing": climb, "power_small": small}


def sd_ratio_of(results: dict[str, Any]) -> dict[str, Any]:
    """How much wider a two-month outcome's standard error is than a one-month one's, from
    EXP-0009's own level family (R_end(P3) - R_end(P1), two months) against its primary (one
    month), on the same players: the ratio of the HC3 standard errors per quantity, and their
    upper quartile as the planned value. The level family has fewer covariates and a level, not a
    change, as its exposure, so the ratio is a rough guide; the blinded sizing step of section 6
    replaces it with the outcome's own SD before any game is observed."""
    ratios = {q: results["level"][q]["se"] / results["primary"][q]["se"] for q in QUANTITIES}
    xs = sorted(ratios.values())
    pos = 0.75 * (len(xs) - 1)
    lo = int(math.floor(pos))
    upper_quartile = xs[lo] + (xs[min(lo + 1, len(xs) - 1)] - xs[lo]) * (pos - lo)
    return {"per_quantity": ratios, "median": xs[len(xs) // 2], "planned": upper_quartile}


def planning_inputs(results: dict[str, Any], k: float) -> dict[str, dict[str, float]]:
    out = {}
    for q in QUANTITIES:
        f = results["primary"][q]
        n, r = int(f["n"]), float(f["reliability"])
        r_low = reliability_lower(r, n)
        r_k = spearman_brown(r_low, k)
        out[q] = {
            "n_0009": n,
            "se_0009": float(f["se"]),
            "reliability_0009": r,
            "reliability_0009_lower95": r_low,
            "reliability_planned": r_k,
            "minimum_per_observed_sd": DELTA * math.sqrt(r_k),
        }
    return out


def at_width(inputs: dict[str, dict[str, float]], w: float, density: float,
             sd_ratio: float) -> dict[str, Any]:
    per = {}
    for q, f in inputs.items():
        n = f["n_0009"] / SLICE_0009 * w * density
        se = f["se_0009"] * math.sqrt(f["n_0009"] / n) * sd_ratio
        per[q] = {"n": n, "se": se, **powers(f["minimum_per_observed_sd"], se)}
    keys_ok = sum(1 for q in KEYS
                  if min(per[q]["power_climbing"], per[q]["power_small"]) >= TARGET)
    control_ok = per[CONTROL]["power_climbing"] >= TARGET
    return {
        "per_quantity": per,
        "keys_at_target": keys_ok,
        "control_at_target": control_ok,
        "reaches_target": keys_ok >= KEYS_NEEDED and control_ok,
    }


def games_per_player(k: float) -> float:
    """Planned games read for observation per player and window: k times EXP-0009's mean per
    month, within the 144 read by hash."""
    return min(OBSERVE_PER_WINDOW, k * GAMES_PER_MONTH_0009)


def observe_games(w: float, density: float, k: float) -> float:
    players = PLAYERS_OBSERVED_0009 / SLICE_0009 * w * density
    return players * 2 * games_per_player(k)


def grid(inputs: dict[str, dict[str, float]], density: float, sd_ratio: float,
         k: float) -> dict[str, Any]:
    g = {}
    chosen = None
    for w in W_GRID:
        cell = at_width(inputs, w, density, sd_ratio)
        cell["observe_games"] = observe_games(w, density, k)
        g[f"{w:g}"] = cell
        if chosen is None and cell["reaches_target"]:
            chosen = w
    return {"grid": g, "w": chosen}


def control_sensitivity(inputs: dict[str, dict[str, float]], w: float, density: float,
                        sd_ratio: float) -> dict[str, float]:
    """The control's chance of 'precedes climbing' at smaller true effects than the minimum."""
    f = inputs[CONTROL]
    n = f["n_0009"] / SLICE_0009 * w * density
    se = f["se_0009"] * math.sqrt(f["n_0009"] / n) * sd_ratio
    return {f"{e:g}": powers(e * math.sqrt(f["reliability_planned"]), se)["power_climbing"]
            for e in CONTROL_TRUE_EFFECTS}


def stage2(inputs: dict[str, dict[str, float]], sd_ratio: float) -> dict[str, Any]:
    """The players the median eval-only key needs for 0.80 in both powers, as the stand-in for
    the nine enriched keys (whose reliability nobody has measured), times our own analysis."""
    z = norm.ppf(1 - (1 - SIMULTANEOUS) / 2) + norm.ppf(1 - (1 - TARGET) / 2)
    need = sorted(
        (z * f["se_0009"] * sd_ratio * math.sqrt(f["n_0009"]) / f["minimum_per_observed_sd"]) ** 2
        for q, f in inputs.items() if q in KEYS
    )
    n2 = (need[3] + need[4]) / 2
    games_cap = 2 * KEEP_PER_WINDOW
    games_mean = 2 * games_per_player(PLAN["k"])
    return {
        "players_for_median_key": n2,
        "games_per_player_cap": games_cap,
        "games_per_player_mean": games_mean,
        "cpu_hours_at_cap": n2 * games_cap * STAGE2_CPU_SECONDS_PER_GAME / 3600,
        "cpu_hours_at_mean": n2 * games_mean * STAGE2_CPU_SECONDS_PER_GAME / 3600,
        "max_cpu_hours": STAGE2_MAX_CPU_HOURS,
        "runs": False,
    }


def memory_table() -> list[dict[str, Any]]:
    """Under a rating that keeps a share phi of its error per game (an AR(1) model of the
    rating's memory), the share of the exposure games' luck, counted at the end of the second
    feature window, that each candidate outcome carries, with its sign. Under the same model the
    rating's catch-up to a skill change made in the feature window is forgotten at the same rate,
    so each row's catch-up share is the same number: a baseline taken later shrinks the bias and
    the catch-up alike. Only a change in skill that continues after the feature window (a trend
    the player is still on) is not shrunk.

    - exp0009: R_end(next month) - R_end(F2), EXP-0009's primary;
    - gap_end: R_end(O) - R_end(G), one gap month and a two-month outcome (this design);
    - mean_after: mean rating over O - mean rating over G (month-mean ratings after the gap);
    - mean_next: mean rating over the next month - R_end(F2) (month-mean outcome, no gap).
    """
    rows = []
    for phi in PHI:
        for g in GAMES_PER_MONTH:
            def mean_share(start: int, games: int) -> float:
                return sum(phi ** t for t in range(start + 1, start + games + 1)) / games

            rows.append({
                "phi": phi,
                "rapid_games_per_month": g,
                "exp0009": phi ** g - 1,
                "gap_end": phi ** (3 * g) - phi ** g,
                "mean_after": mean_share(g, 2 * g) - mean_share(0, g),
                "mean_next": mean_share(0, g) - 1,
                "left_after_gap": phi ** g,
            })
    return rows


def round_floats(x: Any) -> Any:
    if isinstance(x, float):
        return round(x, 6)
    if isinstance(x, dict):
        return {k: round_floats(v) for k, v in x.items()}
    if isinstance(x, list):
        return [round_floats(v) for v in x]
    return x


def main() -> None:
    results = json.loads(EXP0009.read_text(encoding="utf-8"))
    ratio = sd_ratio_of(results)
    PLAN["sd_ratio"] = round(ratio["planned"], 6)
    plan_inputs = planning_inputs(results, PLAN["k"])
    plan = grid(plan_inputs, PLAN["density"], PLAN["sd_ratio"], PLAN["k"])
    w = plan["w"]
    sens = {}
    for k in SENSITIVITY["k"]:
        inputs = planning_inputs(results, k)
        for density in SENSITIVITY["density"]:
            for sd_ratio in SENSITIVITY["sd_ratio"]:
                cell = grid(inputs, density, sd_ratio, k)
                sens[f"k={k:g},density={density:g},sd_ratio={sd_ratio:g}"] = {
                    "w": cell["w"],
                    "observe_games_at_w": (cell["grid"][f"{cell['w']:g}"]["observe_games"]
                                           if cell["w"] is not None else None),
                    "keys_at_target_at_0.6": cell["grid"]["0.6"]["keys_at_target"],
                    "control_at_target_at_0.6": cell["grid"]["0.6"]["control_at_target"],
                }
    games = plan["grid"][f"{w:g}"]["observe_games"] if w is not None else None
    out = {
        "experiment": "EXP-0029",
        "inputs": "EXP-0009 results.json (development data)",
        "plan": PLAN,
        "sd_ratio_from_exp0009": ratio,
        "planning_inputs": plan_inputs,
        "w_grid": plan["grid"],
        "w": w,
        "reaches_target": w is not None,
        "observe_games_at_w": games,
        "observe_cpu_hours_at_w": games / OBSERVE_GAMES_PER_SECOND / 3600 if games else None,
        "players_at_w": (PLAYERS_OBSERVED_0009 / SLICE_0009 * w * PLAN["density"]
                         if w is not None else None),
        "players_with_outcome_at_w": (PLAYERS_OBSERVED_0009 / SLICE_0009 * w * PLAN["density"]
                                      * WITH_OUTCOME_SHARE_0009 if w is not None else None),
        "games_per_player_window": games_per_player(PLAN["k"]),
        "control_sensitivity_at_w": (control_sensitivity(plan_inputs, w, PLAN["density"],
                                                         PLAN["sd_ratio"])
                                     if w is not None else None),
        "sensitivity": sens,
        "stage2": stage2(plan_inputs, PLAN["sd_ratio"]),
        "memory": memory_table(),
    }
    (HERE / "power.json").write_text(
        json.dumps(round_floats(out), indent=2, sort_keys=True, allow_nan=False) + "\n",
        encoding="utf-8",
    )
    print(f"w = {w}, observe_games = {games}, stage 2 = {out['stage2']['cpu_hours_at_cap']:.0f} CPU-h")


if __name__ == "__main__":
    main()

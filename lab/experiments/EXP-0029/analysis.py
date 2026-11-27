"""EXP-0029: does a fall in a weakness over three months come before further climbing, once the
rating has had a month to catch up? (preregistration.md)

EXP-0009's follow-up. The cohort is anonymised Lichess players followed over nine monthly dumps
by a salted hash of their name alone (`lichess-db cohort`, ADR 0114), with EXP-0009's salt and
only the hash range [0.3, 0.9), so no player EXP-0009 could follow is here:

- H = 2025-09 (players file): the activity rule and the history ratings;
- F1 = 2025-10..12 and F2 = 2026-01..03 (players and games files): what Rematch measures;
- G = 2026-04 and 2026-06 (players files): the outcome, R_end(June) - R_end(April).

Every measured quantity comes from the production Rust code through `rematch_lab.observe`
(`cli lab observe`), as in EXP-0009, whose definitions this file imports where they are the same
(the evaluations, the game key, a month's measures, the orientation and the bands). Python here
selects, counts, fits the registered regressions and decides; it computes no evaluation,
judgement, accuracy or weakness event.

    python analysis.py sizing --streaming-cpu-hours H   # section 6: power.json's `sizing`
    python analysis.py --out DIR                        # the registered run: DIR/results.json

The sizing step reads the players files only (counts, April's and June's activity and the
outcome's spread, never a link to any quantity) and calls `power.py`'s `decide_size`. The run
reads the size it chose from `power.json` and observes the slice [0.3, 0.3 + w).
"""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import math
import os
import statistics
import sys
from collections import defaultdict
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
from typing import Any

import numpy as np
import statsmodels.api as sm
from scipy.stats import norm
from statsmodels.stats.multitest import multipletests

import rematch_lab
from rematch_lab.experiments import Budget, external_input, write_results
from rematch_lab.models import ObserveGame, ObserveRequest

EXP = "EXP-0029"
HERE = Path(__file__).resolve().parent


def _load(name: str, path: Path) -> Any:
    spec = importlib.util.spec_from_file_location(name, path)
    mod = importlib.util.module_from_spec(spec)  # type: ignore[arg-type]
    spec.loader.exec_module(mod)  # type: ignore[union-attr]
    return mod


# EXP-0009's frozen analysis (its sha256 is in its own section 10 and in this inputs.sha256):
# the same evaluations, game key, month measures, orientation and bands.
E9 = _load("exp0009_analysis", HERE.parent / "EXP-0009" / "analysis.py")
POWER = _load("exp0029_power", HERE / "power.py")

# Section 7: the cohort. EXP-0009's salt, public by design (ADR 0114).
SALT = hashlib.sha256(b"EXP-0009/cohort/salt").hexdigest()
assert E9.SALT == SALT
DATA = "exp29-cohort"
H_MONTH = "2025-09"
WINDOWS = {"F1": ("2025-10", "2025-11", "2025-12"), "F2": ("2026-01", "2026-02", "2026-03")}
G_MONTH = "2026-04"
J_MONTH = "2026-06"
MONTHS = (H_MONTH, *WINDOWS["F1"], *WINDOWS["F2"], G_MONTH, J_MONTH)
RANGE = (0.3, 0.9)  # EXP-0009 drew from [0, 0.3) and [0.9, 1)
MIN_H_RAPID = 20
MIN_WINDOW_EVAL = 30  # analysed rapid games a window, from the players files and after observing
MIN_OUTCOME_RAPID = 5  # in April and in June
MAX_RATING = 2000  # R_end(F1) below this
ENDS = 5
APRIL_SENSITIVITY = 30  # the primary on players with at least this many rapid games in April

# Sections 4, 5, 8 and 9.
KEYS = E9.KEYS
QUANTITIES = E9.QUANTITIES
EXPLORATORY = E9.EXPLORATORY
CONTROL = E9.POSITIVE_CONTROL
OUTCOME2_KEYS = POWER.OUTCOME2_KEYS
DELTA = 14.0
ALPHA = 0.05
LEVELS = (0.80, 0.95, 0.975)
SIMULTANEOUS = 1 - ALPHA / len(KEYS)  # 99.375%: the control is not in the keys' family (M1)
Z_RELIABILITY = norm.ppf(0.975)
MIN_PLAYERS = 30
RELIABILITY_FLOOR = 0.01
RATING_CHANGE_SD_0009 = 53.789254
WITH_OUTCOME_0009 = 16_204  # EXP-0009's confirmatory players with an outcome

# Section 0: the registered run's budget, in counted units.
BUDGET: dict[str, float] = {"observe_games": 3_700_000, "engine_nodes": 1_000_000}
CAP = 1.5

WORKERS = int(os.environ.get("EXP29_WORKERS", "6"))  # speed only; never a result
BATCH = 1000

# Every family of section 8; only the primary decides.
FAMILIES = ("primary", "f1level", "ivw", "adjusted", "april30", "nogap", "contemporaneous",
            "level")


def log(msg: str) -> None:
    print(f"{EXP}: {msg}", file=sys.stderr, flush=True)


# ---- the cohort -------------------------------------------------------------------------------


def path_of(month: str, name: str) -> Path:
    return external_input(f"{DATA}/{month}/{name}")


def in_range(h: str, lo: float, hi: float) -> bool:
    return lo <= E9.fraction(h) < hi


def read_players(month: str, lo: float, hi: float,
                 keep: set[str] | None = None) -> dict[str, dict[str, Any]]:
    out = {}
    with path_of(month, "players.jsonl").open(encoding="utf-8") as f:
        for line in f:
            h = line[6:38]
            if keep is not None and h not in keep:
                continue
            p = json.loads(line)
            if p["h"] == h and in_range(h, lo, hi):
                out[h] = p
    return out


def r_start(p: dict[str, Any]) -> float:
    return E9.mean([float(x[2]) for x in p["first"][:ENDS]])


def r_end(p: dict[str, Any]) -> float:
    return E9.mean([float(x[3]) for x in p["last"][-ENDS:]])


def window_end(months: list[dict[str, Any] | None]) -> float:
    """The mean rating after the window's last five rapid games by time: each month's own last
    five, merged, the latest five kept (section 4)."""
    games = sorted(g for p in months if p for g in p["last"])
    return E9.mean([float(g[3]) for g in games[-ENDS:]])


def total(months: list[dict[str, Any] | None], field: str) -> int:
    return sum(p[field] for p in months if p)


def eligible(lo: float, hi: float) -> tuple[dict[str, dict[str, Any]], dict[str, Any]]:
    """Section 7's rules on the players files, in order, with the count each step leaves and the
    flow from F1 into F2 by F1 rating band and F1 activity (S6)."""
    hp = read_players(H_MONTH, lo, hi)
    history = {h for h, p in hp.items() if p["rapid_games"] >= MIN_H_RAPID}
    files = {H_MONTH: {h: hp[h] for h in history}}
    for m in MONTHS[1:]:
        files[m] = read_players(m, lo, hi, history)
    flow: dict[str, Any] = {"history": len(history)}

    def win(h: str, w: str) -> list[dict[str, Any] | None]:
        return [files[m].get(h) for m in WINDOWS[w]]

    f1 = sorted(h for h in history if total(win(h, "F1"), "eval_games") >= MIN_WINDOW_EVAL
                and window_end(win(h, "F1")) < MAX_RATING)
    flow["f1_eligible"] = len(f1)
    f2 = [h for h in f1 if total(win(h, "F2"), "eval_games") >= MIN_WINDOW_EVAL]
    flow["f1_and_f2"] = len(f2)
    f2set = set(f2)
    into_f2: dict[str, list[int]] = defaultdict(lambda: [0, 0])
    games = sorted(total(win(h, "F1"), "rapid_games") for h in f1) or [0]
    cuts = (games[len(games) // 3], games[2 * len(games) // 3])
    for h in f1:
        b = E9.band(window_end(win(h, "F1")))
        g = total(win(h, "F1"), "rapid_games")
        t = "low" if g < cuts[0] else "mid" if g < cuts[1] else "high"
        for key in (f"band:{b}", f"activity:{t}"):
            into_f2[key][0] += 1
            into_f2[key][1] += h in f2set
    flow["into_f2"] = {k: {"f1": v[0], "f2": v[1]} for k, v in sorted(into_f2.items())}
    cohort = {h: {m: files[m].get(h) for m in MONTHS} for h in f1}
    return cohort, {"flow": flow, "f2": f2}


def has_outcome(months: dict[str, Any]) -> bool:
    return all((months[m] or {}).get("rapid_games", 0) >= MIN_OUTCOME_RAPID
               for m in (G_MONTH, J_MONTH))


# ---- the sizing step (section 6) ----------------------------------------------------------------


def sizing_measures(streaming_cpu_hours: float) -> dict[str, Any]:
    """The sizing step's measurements, from the players files only, over [0.3, 0.9)."""
    cohort, info = eligible(*RANGE)
    f2 = info["f2"]
    width = RANGE[1] - RANGE[0]
    with_outcome = [h for h in f2 if has_outcome(cohort[h])]
    # power.py scales EXP-0009's players with an outcome, so the density counts those (S2a-4);
    # the games are projected from the players observed, before the outcome rule.
    density = len(with_outcome) / width / (WITH_OUTCOME_0009 / POWER.SLICE_0009)
    density_observed = len(f2) / width / (POWER.PLAYERS_OBSERVED_0009 / POWER.SLICE_0009)
    ms = [total([cohort[h][m] for m in WINDOWS[w]], "eval_games") for h in f2 for w in WINDOWS]
    caps = {f"{r}/{k}": {"E_min_read": statistics.fmean(min(m, r) for m in ms),
                         "E_inv_min_kept": statistics.fmean(1 / min(m, k) for m in ms)}
            for r, k in POWER.CAPS_PER_WINDOW}
    out = [h for h in f2 if has_outcome(cohort[h])]
    share = {}
    for phi in POWER.PHI:
        share[f"{phi:g}"] = statistics.fmean(
            phi ** cohort[h][G_MONTH]["rapid_games"]
            * (phi ** (2 * cohort[h][J_MONTH]["rapid_games"]) - 1) for h in out)
    ys = [r_end(cohort[h][J_MONTH]) - r_end(cohort[h][G_MONTH]) for h in out]
    return {
        "density": density,
        "density_observed": density_observed,
        "caps": caps,
        "april_share": share,
        "sd_ratio": statistics.stdev(ys) / RATING_CHANGE_SD_0009,
        "streaming_cpu_hours": streaming_cpu_hours,
        "counts": {**info["flow"], "with_outcome": len(out)},
    }


def cmd_sizing(streaming_cpu_hours: float) -> dict[str, Any]:
    measured = sizing_measures(streaming_cpu_hours)
    results0009 = json.loads(POWER.EXP0009.read_text(encoding="utf-8"))
    decision = POWER.decide_size(results0009, {k: v for k, v in measured.items() if k != "counts"})
    power_path = HERE / "power.json"
    power = json.loads(power_path.read_text(encoding="utf-8"))
    power["sizing"] = POWER.round_floats({"measured": measured, "decision": decision,
                                          "algorithm_id": rematch_lab.algorithm()["id"]})
    power_path.write_text(json.dumps(power, indent=2, sort_keys=True, allow_nan=False) + "\n",
                          encoding="utf-8")
    log(f"sizing: run {decision['run']}, w {decision.get('w')}, cap {decision.get('cap')}")
    return power["sizing"]


# ---- observing ----------------------------------------------------------------------------------


def select_games(cohort: dict[str, Any]) -> dict[tuple[str, str], list[tuple[str, str, str]]]:
    """Per (player, window), the player's rapid games with evaluations in the window's months:
    `(key, month, id)` sorted by game key."""
    out: dict[tuple[str, str], list[tuple[str, str, str]]] = defaultdict(list)
    for w, months in WINDOWS.items():
        for m in months:
            with path_of(m, "games.jsonl").open(encoding="utf-8") as f:
                for line in f:
                    g = json.loads(line)
                    for side in ("white", "black"):
                        h = g[side]
                        if h in cohort:
                            out[(h, w)].append((E9.game_key(g["id"]), m, g["id"]))
    for v in out.values():
        v.sort()
    return out


def observe_all(cohort: dict[str, Any], budget: Budget | None, lo: float, hi: float,
                read: int, kept: int) -> tuple[dict, dict[str, Any]]:
    """Per (player, window), the first `read` games by game key through `cli lab observe`; of
    those, the first `kept` that are standard chess from the start position and observe without
    an error (EXP-0009's rule with the windows' caps)."""
    start = E9.start_search(budget)
    chosen = {(h, m, gid) for (h, _), games in select_games(cohort).items()
              for _, m, gid in games[:read]}
    window_of = {m: w for w, ms in WINDOWS.items() for m in ms}
    counts = {"games_read": 0, "games_observed": 0, "games_failed": 0, "games_not_standard": 0,
              "games_between_two_observed_players": 0,
              "games_with_an_opponent_outside_the_slice": 0}
    algorithm_ids: set[str] = set()
    observed: dict[tuple[str, str], list[tuple[str, str, Any]]] = defaultdict(list)

    def collect(resp: Any, meta: dict[str, dict[str, Any]]) -> None:
        algorithm_ids.add(resp.algorithm_id)
        for og in resp.games:
            if og.error:
                counts["games_failed"] += 1
                continue
            counts["games_observed"] += 1
            mt = meta[og.id]
            for s in og.sides:
                observed[(mt["sides"][s.color], mt["window"])].append(
                    (E9.game_key(mt["id"]), mt["id"], (s, mt["elos"][s.color])))

    def batches() -> Any:
        batch: list[ObserveGame] = []
        meta: dict[str, dict[str, Any]] = {}
        for w, months in WINDOWS.items():
            for m in months:
                with path_of(m, "games.jsonl").open(encoding="utf-8") as f:
                    for line in f:
                        g = json.loads(line)
                        sides = [s for s in ("white", "black")
                                 if g[s] in cohort and (g[s], m, g["id"]) in chosen]
                        if not sides:
                            continue
                        counts["games_read"] += 1
                        if len(sides) == 2:
                            counts["games_between_two_observed_players"] += 1
                        elif any(g[s] and not in_range(g[s], lo, hi) for s in ("white", "black")):
                            counts["games_with_an_opponent_outside_the_slice"] += 1
                        if not E9.standard(g["pgn"]):
                            counts["games_not_standard"] += 1
                            continue
                        uid = f"{m}:{g['id']}"
                        meta[uid] = {
                            "window": window_of[m], "id": g["id"],
                            "sides": {s: g[s] for s in sides},
                            "elos": {"white": (g["white_elo"], g["black_elo"]),
                                     "black": (g["black_elo"], g["white_elo"])},
                        }
                        batch.append(ObserveGame(
                            id=uid, pgn=g["pgn"], base_seconds=g["base_seconds"],
                            increment_seconds=g["increment_seconds"],
                            lichess_evals=E9.lichess_evals(g["pgn"]), sides=sides,
                            white_rating=g["white_elo"], black_rating=g["black_elo"],
                            termination=g["termination"],
                        ))
                        if len(batch) == BATCH:
                            yield ObserveRequest(games=batch, searches=[start]), meta
                            batch, meta = [], {}
        if batch:
            yield ObserveRequest(games=batch, searches=[start]), meta

    with ThreadPoolExecutor(max_workers=WORKERS) as pool:
        pending: list[Any] = []
        for req, meta in batches():
            if budget:
                budget.spend("observe_games", len(req.games))
            pending.append((pool.submit(rematch_lab.observe, req), meta))
            while len(pending) >= 2 * WORKERS:
                fut, mt = pending.pop(0)
                collect(fut.result(), mt)
        for fut, mt in pending:
            collect(fut.result(), mt)
    if len(algorithm_ids) != 1:
        raise SystemExit(f"the run spans algorithm ids {sorted(algorithm_ids)}")
    for v in observed.values():
        v.sort(key=lambda x: (x[0], x[1]))
        del v[kept:]
    return observed, {**counts, "algorithm_id": algorithm_ids.pop()}


def build_table(lo: float, hi: float, budget: Budget | None, read: int, kept: int
                ) -> tuple[list[dict[str, Any]], list[dict[str, Any]], dict]:
    """The analysis rows (players with 30 kept games in both windows) and the F1-eligible rows
    for the model of staying into F2 (players-file terms only)."""
    cohort, info = eligible(lo, hi)
    f2 = set(info["f2"])
    stay_rows = []
    for h, mo in cohort.items():
        w1 = [mo[m] for m in WINDOWS["F1"]]
        stay_rows.append({"h": h, "into_f2": h in f2, **players_terms(mo, w1)})
    observed, counts = observe_all({h: cohort[h] for h in sorted(f2)}, budget, lo, hi, read, kept)
    rows = []
    for h in sorted(f2):
        mo = cohort[h]
        feats = {w: observed.get((h, w), []) for w in WINDOWS}
        if any(len(v) < MIN_WINDOW_EVAL for v in feats.values()):
            continue
        w1 = [mo[m] for m in WINDOWS["F1"]]
        w2 = [mo[m] for m in WINDOWS["F2"]]
        a1, b1 = E9.halves(feats["F1"])
        a2, b2 = E9.halves(feats["F2"])
        row: dict[str, Any] = {
            "h": h,
            **players_terms(mo, w1),
            "r2_end": window_end(w2),
            "games2": total(w2, "rapid_games"),
            "analysed_share2": total(w2, "eval_games") / total(w2, "rapid_games"),
            "gamesG": (mo[G_MONTH] or {}).get("rapid_games", 0),
            "m1": E9.month_measures(feats["F1"]),
            "m2": E9.month_measures(feats["F2"]),
            "halves": {"m1a": a1, "m1b": b1, "m2a": a2, "m2b": b2},
        }
        if (mo[G_MONTH] or {}).get("rapid_games", 0) >= MIN_OUTCOME_RAPID:
            row["rG_end"] = r_end(mo[G_MONTH])
        if has_outcome(mo):
            row["rJ_end"] = r_end(mo[J_MONTH])
        rows.append(row)
    flow = dict(info["flow"])
    flow["observed_games"] = len(rows)
    flow["with_outcome"] = sum(1 for r in rows if "rJ_end" in r)
    bands: dict[str, int] = defaultdict(int)
    for r in rows:
        bands[E9.band(r["r1_end"])] += 1
    flow["by_band"] = dict(sorted(bands.items()))
    return rows, stay_rows, {"flow": flow, "games": counts}


def players_terms(mo: dict[str, Any], w1: list[dict[str, Any] | None]) -> dict[str, Any]:
    """The players-file terms measured before F2 (section 8)."""
    hp = mo[H_MONTH]
    rapid = total(w1, "rapid_games")
    every = rapid + total(w1, "bullet") + total(w1, "blitz") + total(w1, "classical")
    return {
        "rH_start": r_start(hp), "rH_end": r_end(hp), "r1_end": window_end(w1),
        "gamesH": hp["rapid_games"], "games1": rapid,
        "rapid_share1": rapid / every, "analysed_share1": total(w1, "eval_games") / rapid,
    }


# ---- the models ---------------------------------------------------------------------------------


def oriented(q: str, x: float) -> float:
    return E9.oriented(q, x)


def noise_variance(q: str, m: dict[str, Any]) -> float:
    """A window's binomial noise variance of q, for the inverse-variance-weighted level (N6):
    q(1 - q) over its opportunities for a rate; one over the games for accuracy."""
    n = max(1, m[f"{q}:opps"])
    if q == "accuracy":
        return 1.0 / n
    return max(m[q] * (1 - m[q]), 1e-4) / n


def base_terms(r: dict[str, Any], q: str, level: float) -> list[float]:
    """What every change model adjusts for (section 8): the level (oriented), the F1 end rating,
    the change from the history month's end to F1's end and within the history month, rapid
    games in H, F1 and F2, the F1 rapid share, the analysed shares and the opportunities."""
    return [oriented(q, level), (r["r1_end"] - 1500) / 100, r["r1_end"] - r["rH_end"],
            r["rH_end"] - r["rH_start"], math.log1p(r["gamesH"]), math.log1p(r["games1"]),
            math.log1p(r["games2"]), r["rapid_share1"], r["analysed_share1"],
            r["analysed_share2"], math.log1p(r["m1"][f"{q}:opps"]),
            math.log1p(r["m2"][f"{q}:opps"])]


def level_of(r: dict[str, Any], q: str, family: str) -> float:
    a, b = r["m1"][q], r["m2"][q]
    if family == "f1level":
        return a
    if family == "ivw":
        va, vb = noise_variance(q, r["m1"]), noise_variance(q, r["m2"])
        return (a / va + b / vb) / (1 / va + 1 / vb)
    return (a + b) / 2  # the window-mean level of the primary (B2)


def design(rows: list[dict[str, Any]], q: str, family: str
           ) -> tuple[np.ndarray, np.ndarray, list[dict[str, Any]]]:
    """The registered regression of one quantity (section 8). The first column after the
    constant is the standardised exposure; its coefficient is the effect, in Elo per SD."""
    ys, xs, used = [], [], []
    for r in rows:
        a, b = r["m1"][q], r["m2"][q]
        if a is None or b is None:
            continue
        change = oriented(q, b - a)
        gap = (r["m2"]["opp_gap"] - r["m1"]["opp_gap"]) / 100
        if family in ("primary", "f1level", "ivw", "adjusted", "april30"):
            if "rJ_end" not in r:
                continue
            if family == "april30" and r["gamesG"] < APRIL_SENSITIVITY:
                continue
            y = r["rJ_end"] - r["rG_end"]
            x = [change, *base_terms(r, q, level_of(r, q, family))]
            if family == "adjusted":
                x += [r["r2_end"] - r["r1_end"], r["rG_end"] - r["r2_end"], gap]
        elif family == "nogap":
            if "rG_end" not in r:
                continue
            y = r["rG_end"] - r["r2_end"]
            x = [change, *base_terms(r, q, level_of(r, q, "primary"))]
        elif family == "contemporaneous":
            y = r["r2_end"] - r["r1_end"]
            x = [change, *base_terms(r, q, level_of(r, q, "primary")), gap]
        elif family == "level":
            if "rJ_end" not in r:
                continue
            y = r["rJ_end"] - r["r1_end"]
            x = [oriented(q, a), (r["r1_end"] - 1500) / 100, r["r1_end"] - r["rH_end"],
                 r["rH_end"] - r["rH_start"], math.log1p(r["gamesH"]), math.log1p(r["games1"]),
                 r["rapid_share1"], r["analysed_share1"], math.log1p(r["m1"][f"{q}:opps"]),
                 r["m1"]["opp_gap"] / 100]
        else:
            raise ValueError(family)
        ys.append(y)
        xs.append(x)
        used.append(r)
    if not xs:
        return np.zeros(0), np.zeros((0, 0)), []
    X = np.asarray(xs, dtype=float)
    sd = X[:, 0].std(ddof=1) if len(X) > 1 else 0.0
    X[:, 0] = (X[:, 0] - X[:, 0].mean()) / sd if sd > 0 else 0.0
    return np.asarray(ys, dtype=float), sm.add_constant(X, has_constant="add"), used


def reliability(used: list[dict[str, Any]], q: str, family: str) -> dict[str, Any] | None:
    """The split-half reliability of the exposure on the players the model used (section 8):
    the change F1 to F2 (the level family: the F1 level) on odd against even games by game key,
    rho, the Spearman-Brown point r and r_L, the lower end of its 95% interval by Fisher's z at
    the players with both halves, floored at 0.01."""
    xa, xb = [], []
    for r in used:
        hv = r["halves"]
        if family == "level":
            pa, pb = hv["m1a"][q], hv["m1b"][q]
        else:
            vals = [hv[k][q] for k in ("m1a", "m2a", "m1b", "m2b")]
            if any(v is None for v in vals):
                continue
            pa, pb = vals[1] - vals[0], vals[3] - vals[2]
        if pa is None or pb is None:
            continue
        xa.append(pa)
        xb.append(pb)
    n = len(xa)
    if n < MIN_PLAYERS or np.std(xa) == 0 or np.std(xb) == 0:
        return None
    rho = float(np.corrcoef(xa, xb)[0, 1])
    point = RELIABILITY_FLOOR if rho <= 0 else min(1.0, max(RELIABILITY_FLOOR, 2 * rho / (1 + rho)))
    rho_c = min(max(rho, -0.999999), 0.999999)
    low = math.tanh(math.atanh(rho_c) - Z_RELIABILITY / math.sqrt(n - 3))
    lower = max(RELIABILITY_FLOOR, 2 * low / (1 + low)) if low > 0 else RELIABILITY_FLOOR
    return {"rho": rho, "n": n, "point": point, "lower": min(lower, point)}


def fit(rows: list[dict[str, Any]], q: str, family: str,
        weights: dict[str, float] | None = None) -> dict[str, Any]:
    y, X, used = design(rows, q, family)
    n = len(y)
    if n < MIN_PLAYERS:
        return {"n": n, "estimate": None}
    if weights is None:
        res = sm.OLS(y, X).fit(cov_type="HC3")
    else:
        res = sm.WLS(y, X, weights=[weights[r["h"]] for r in used]).fit(cov_type="HC3")
    est, se = float(res.params[1]), float(res.bse[1])
    out: dict[str, Any] = {"n": n, "estimate": est, "se": se, "p": float(res.pvalues[1]),
                           "rank_deficient": bool(np.linalg.matrix_rank(X) < X.shape[1])}
    for level in (*LEVELS, SIMULTANEOUS):
        z = norm.ppf(1 - (1 - level) / 2)
        out[f"ci{level * 100:g}"] = [est - z * se, est + z * se]
    rel = reliability(used, q, family)
    out["reliability"] = rel
    if rel is not None:
        out["minimum_observed"] = DELTA * math.sqrt(rel["lower"])
        out["disattenuated"] = est / math.sqrt(rel["point"])
        out["disattenuated_lower"] = est / math.sqrt(rel["lower"])
    else:
        out["minimum_observed"] = None
    return out


def family(rows: list[dict[str, Any]], name: str,
           weights: dict[str, float] | None = None) -> dict[str, Any]:
    """One family: Holm across the eight keys; the control tested on its own (section 8)."""
    return read_family({q: fit(rows, q, name, weights) for q in QUANTITIES})


def read_family(fits: dict[str, Any]) -> dict[str, Any]:
    """Section 8's reads on one family's fits. Holm runs over all eight keys, a key that cannot
    be estimated counting as p = 1 (S2a-2), so the family is always eight. Holm caps the chance
    that any null key reads "precedes climbing" at 5%; it is not zero (S2a-5)."""
    ps = [fits[q]["p"] if fits[q]["estimate"] is not None else 1.0 for q in KEYS]
    reject, adj, _, _ = multipletests(ps, alpha=ALPHA, method="holm")
    for q, rj, a in zip(KEYS, reject, adj, strict=True):
        fits[q]["p_holm"] = float(a)
        fits[q]["holm_reject"] = bool(rj) and fits[q]["estimate"] is not None
    for q in KEYS:
        f = fits[q]
        if f["estimate"] is None:
            f["reads"] = "not estimable"
            continue
        lo, hi = f[f"ci{SIMULTANEOUS * 100:g}"]
        d = f["minimum_observed"]
        if f["holm_reject"] and f["estimate"] > 0:
            f["reads"] = "precedes climbing"
        elif f["holm_reject"]:
            f["reads"] = "precedes falling"
        elif d is not None and -d < lo and hi < d:
            f["reads"] = "below the minimum effect"
        else:
            f["reads"] = "undetermined"
    c = fits[CONTROL]
    c["holds"] = c["estimate"] is not None and c["p"] < ALPHA and c["estimate"] > 0
    return fits


def decide(primary: dict[str, Any], f1level: dict[str, Any]) -> dict[str, Any]:
    """Section 9, in order."""
    climbing = [q for q in KEYS if primary[q]["reads"] == "precedes climbing"]
    below = [q for q in OUTCOME2_KEYS if primary[q]["reads"] == "below the minimum effect"]
    control = bool(primary[CONTROL]["holds"])
    if climbing:
        outcome = 1
        name = "candidates to teach"
    elif len(below) == len(OUTCOME2_KEYS) and control:
        outcome = 2
        name = "no further climbing from these keys"
    else:
        outcome = 3
        name = "need more data"
    artefacts = [q for q in KEYS if f1level[q].get("reads") == "precedes climbing"
                 and q not in climbing]
    # Each candidate with its raw and Holm p: under no link a key reads "precedes climbing"
    # with a family-wise chance of up to 5%, so a candidate is replicated on new months by the
    # follow-up experiment of section 9 before it can change anything (S2a-5).
    candidates = [{"key": q, "estimate": primary[q]["estimate"], "p": primary[q]["p"],
                   "p_holm": primary[q]["p_holm"]} for q in climbing]
    return {"outcome": outcome, "name": name, "climbing": climbing, "candidates": candidates,
            "below_minimum": below, "positive_control_holds": control,
            "f1level_only_climbing": artefacts,
            "follow_up": ("replicate the candidates on new months, per band (section 9)"
                          if climbing else None)}


def by_band(rows: list[dict[str, Any]]) -> dict[str, Any]:
    out = {}
    for b in ("B0", "B1", "B2", "B3"):
        sub = [r for r in rows if E9.band(r["r1_end"]) == b]
        out[b] = {"players": len(sub),
                  **{q: {k: v for k, v in fit(sub, q, "primary").items()
                         if k in ("n", "estimate", "ci95")} for q in QUANTITIES}}
    return out


def by_april(rows: list[dict[str, Any]]) -> dict[str, Any]:
    """The primary by tercile of April's rapid games (exploratory: the bias should shrink)."""
    g = sorted(r["gamesG"] for r in rows if "rJ_end" in r) or [0]
    cuts = (g[len(g) // 3], g[2 * len(g) // 3])
    groups = {"low": [r for r in rows if r["gamesG"] < cuts[0]],
              "mid": [r for r in rows if cuts[0] <= r["gamesG"] < cuts[1]],
              "high": [r for r in rows if r["gamesG"] >= cuts[1]]}
    return {"cuts": list(cuts), **{k: {"players": len(v), **{
        q: {k2: v2 for k2, v2 in fit(v, q, "primary").items() if k2 in ("n", "estimate", "ci95")}
        for q in QUANTITIES}} for k, v in groups.items()}}


def stay_terms(r: dict[str, Any]) -> list[float]:
    return [(r["r1_end"] - 1500) / 100, r["r1_end"] - r["rH_end"], r["rH_end"] - r["rH_start"],
            math.log1p(r["gamesH"]), math.log1p(r["games1"]), r["rapid_share1"],
            r["analysed_share1"]]


def attrition(rows: list[dict[str, Any]], stay_rows: list[dict[str, Any]]) -> dict[str, Any]:
    """Section 8: staying into F2 on the players-file F1 terms, staying into the outcome on the
    primary's exposure and terms (linear probability models, HC3), and the primary re-fitted
    with the product of the inverse probabilities of both from logistic models, floored at 0.05
    (sensitivities only)."""
    out: dict[str, Any] = {}
    y1 = np.array([1.0 if r["into_f2"] else 0.0 for r in stay_rows])
    X1 = sm.add_constant(np.asarray([stay_terms(r) for r in stay_rows], dtype=float),
                         has_constant="add") if stay_rows else np.zeros((0, 0))
    p_f2: dict[str, float] = {}
    if len(y1) >= MIN_PLAYERS and y1.min() != y1.max():
        lpm = sm.OLS(y1, X1).fit(cov_type="HC3")
        out["into_f2"] = {"n": len(y1), "stayed": float(y1.mean()),
                          "coefficients": [float(c) for c in lpm.params[1:]],
                          "se": [float(s) for s in lpm.bse[1:]]}
        try:
            p = sm.Logit(y1, X1).fit(disp=0, maxiter=200).predict(X1)
            p_f2 = {r["h"]: max(float(pi), 0.05) for r, pi in zip(stay_rows, p, strict=True)}
        except Exception as e:  # noqa: BLE001 - a failed fit is reported, never hidden
            out["into_f2"]["logit_error"] = type(e).__name__
    for q in QUANTITIES:
        ys, xs, used = [], [], []
        for r in rows:
            a, b = r["m1"][q], r["m2"][q]
            if a is None or b is None:
                continue
            ys.append(1.0 if "rJ_end" in r else 0.0)
            xs.append([oriented(q, b - a), *base_terms(r, q, (a + b) / 2)])
            used.append(r)
        if len(ys) < MIN_PLAYERS or min(ys) == max(ys):
            continue
        X = np.asarray(xs, dtype=float)
        sd = X[:, 0].std(ddof=1)
        X[:, 0] = (X[:, 0] - X[:, 0].mean()) / sd if sd > 0 else 0.0
        X = sm.add_constant(X, has_constant="add")
        y = np.asarray(ys)
        lpm = sm.OLS(y, X).fit(cov_type="HC3")
        entry: dict[str, Any] = {"n": len(y), "stayed": float(y.mean()),
                                 "per_sd": float(lpm.params[1]), "se": float(lpm.bse[1])}
        try:
            p = sm.Logit(y, X).fit(disp=0, maxiter=200).predict(X)
            w = {r["h"]: 1.0 / (max(float(pi), 0.05) * p_f2.get(r["h"], 1.0))
                 for r, pi in zip(used, p, strict=True)}
            ipw = fit(rows, q, "primary", w)
            entry["ipw_primary"] = {k: ipw.get(k) for k in ("n", "estimate", "ci95")}
        except Exception as e:  # noqa: BLE001
            entry["ipw_primary"] = {"error": type(e).__name__}
        out[q] = entry
    return out


def round_floats(x: Any) -> Any:
    return E9.round_floats(x)


def analyse(rows: list[dict[str, Any]], stay_rows: list[dict[str, Any]]) -> dict[str, Any]:
    fams = {name: family(rows, name) for name in FAMILIES}
    ys = [r["rJ_end"] - r["rG_end"] for r in rows if "rJ_end" in r]
    return {
        **fams,
        "decision": decide(fams["primary"], fams["f1level"]),
        "exploratory": {q: fit(rows, q, "primary") for q in EXPLORATORY},
        "by_band": by_band(rows),
        "by_april_games": by_april(rows),
        "attrition": attrition(rows, stay_rows),
        "rating_change_sd": float(np.std(ys, ddof=1)) if len(ys) > 2 else None,
    }


# ---- the run ------------------------------------------------------------------------------------


def meta_of_months() -> dict[str, Any]:
    out = {}
    for m in MONTHS:
        meta = json.loads(path_of(m, "meta.json").read_text(encoding="utf-8"))
        if meta["salt"] != SALT or meta["player_rate"] < RANGE[1]:
            raise SystemExit(f"{m}: built with another salt or a rate below {RANGE[1]}")
        out[m] = {f: meta[f] for f in ("month", "url", "sha256", "compressed_bytes",
                                       "games_seen", "games_kept", "players", "eval_games")}
    return out


def run(out: Path, w: float, cap: tuple[int, int], algorithm: str | None = None,
        stage2: dict[str, Any] | None = None) -> dict[str, Any]:
    algorithm_id = rematch_lab.algorithm()["id"]
    if algorithm is not None and algorithm_id != algorithm:
        raise SystemExit(f"the cli is {algorithm_id}, not the requested {algorithm}")
    if not 0 < w <= RANGE[1] - RANGE[0]:
        raise SystemExit("the slice must lie inside [0.3, 0.9)")
    lo, hi = RANGE[0], RANGE[0] + w
    budget = Budget(BUDGET, CAP)
    rows, stay_rows, counts = build_table(lo, hi, budget, *cap)
    if counts["games"]["algorithm_id"] != algorithm_id:
        raise SystemExit("observe answered on another algorithm")
    results = {
        "experiment": EXP,
        "algorithm_id": algorithm_id,
        "design": {"seed_namespace": EXP, "slice": [lo, hi], "cap": list(cap), "salt": SALT,
                   "months": {"H": H_MONTH, **WINDOWS, "G": G_MONTH, "J": J_MONTH},
                   "delta": DELTA, "simultaneous_level": SIMULTANEOUS,
                   "outcome2_keys": list(OUTCOME2_KEYS)},
        "months": meta_of_months(),
        **counts,
        "players": len(rows),
        **analyse(rows, stay_rows),
        "stage2": stage2,
        "compute": budget.record(),
    }
    write_results(out, round_floats(results))
    return results


def cmd_run(out: Path, algorithm: str | None) -> None:
    power = json.loads((HERE / "power.json").read_text(encoding="utf-8"))
    sizing = power.get("sizing")
    if not sizing or not sizing["decision"]["run"]:
        raise SystemExit("no sizing decision to run (section 6); the kill rule applies")
    d = sizing["decision"]
    # The id the sizing commit pinned (section 3) is the one the run must be on (S2a-1).
    pinned = algorithm if algorithm is not None else sizing["algorithm_id"]
    run(out, float(d["w"]), tuple(d["cap"]), pinned, power["stage2"])  # type: ignore[arg-type]


def main(argv: list[str] | None = None) -> int:
    argv = sys.argv[1:] if argv is None else argv
    if argv and argv[0] == "sizing":
        p = argparse.ArgumentParser(prog="analysis.py sizing")
        p.add_argument("cmd")
        p.add_argument("--streaming-cpu-hours", type=float, required=True)
        a = p.parse_args(argv)
        cmd_sizing(a.streaming_cpu_hours)
        return 0
    p = argparse.ArgumentParser(prog="analysis.py")
    p.add_argument("--out", type=Path, required=True)
    p.add_argument("--algorithm", help="the algorithm_id the run must be on (the arena's)")
    a = p.parse_args(argv)
    cmd_run(a.out, a.algorithm)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

"""EXP-0009: what actually helps people climb (preregistration.md).

Stage 1 of issue #104, on the evaluations Lichess already published. The cohort is anonymised
Lichess players followed over four monthly dumps by a salted hash of their name alone
(`lichess-db cohort`, ADR 0114): P0 (2026-07) for the history before, P1 (2026-08) and P2
(2026-09) for what Rematch measures, and P3 (2026-10) for the rating after. Every measured
quantity comes from the production Rust code through `rematch_lab.observe` (`cli lab observe`:
`evaluate_game` on Lichess's `[%eval]`, `build_move_records`, `game_accuracy`,
`compute_game_features` and `observe_game`, with no enrichment); the start position, which
Lichess does not score, is one production search at 1 million nodes. Python here selects,
counts, fits the registered regressions and decides; it computes no evaluation, judgement,
accuracy or weakness event.

Stages, in the order they run (each stage's output is committed before the next one runs):

    python analysis.py pilot --out DIR                 # DIR/pilot.json: the development slice
    python analysis.py power --pilot pilot/pilot.json  # power.json: the confirmatory slice
    python analysis.py --out DIR                       # the registered run: DIR/results.json

Only the last is the registered run; the lab check reruns it twice and compares results.json.
The pilot reads only players whose hash fraction is in [0.9, 1); the registered run only those
in [0, q), q from power.json, so no player is in both.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import re
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
from rematch_lab.models import EngineScore, GivenSearch, ObserveGame, ObserveRequest

EXP = "EXP-0009"
HERE = Path(__file__).resolve().parent

# Section 7: the cohort. The salt is public (ADR 0114): the pseudonyms group a player's games,
# they are not a secret.
SALT = hashlib.sha256(b"EXP-0009/cohort/salt").hexdigest()
MONTHS = {"P0": "2026-07", "P1": "2026-08", "P2": "2026-09", "P3": "2026-10"}
FEATURE_MONTHS = ("P1", "P2")
DATA = "exp09-cohort"
PILOT_SLICE = (0.9, 1.0)
MIN_P0_RAPID = 20
MIN_P3_RAPID = 5
MIN_EVAL_GAMES = 10  # observed games a feature month, after exclusions
MAX_EVAL_GAMES = 40  # per feature month, by sha256(salt ‖ "game:" ‖ id)
# Games observed per player and month: the cap and a margin for games that fail to observe.
OBSERVE_GAMES = 48
MAX_RATING = 2000  # R_end(P1) below this: bands B0 to B3, the audience
ENDS = 5  # the games averaged for the rating at the start and the end of a month
MIN_KEY_OPPORTUNITIES = 5  # per key and feature month
START_FEN = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
START_NODES = 1_000_000  # algo-4.0.0's ENGINE_NODES

# Section 4: the quantities, each oriented so that a positive change is an improvement.
KEYS = ("W4.missed", "W4.allowed", "W5", "W6", "W7.traps", "W7.prep", "W9a", "W9b")
QUANTITIES = (*KEYS, "blunder_rate", "accuracy")
HIGHER_IS_BETTER = {"accuracy"}
POSITIVE_CONTROL = "accuracy"

# Sections 5, 8 and 9.
DELTA = 14.0  # Elo of next-month rating per SD of improvement (0.02 points a game, ADR 0101)
ALPHA = 0.05
LEVELS = (0.80, 0.95, 0.975)
SIMULTANEOUS = 1 - ALPHA / len(QUANTITIES)  # Bonferroni, the interval read against DELTA
SMALL_KEYS_FOR_NO_ORDER = 6

# Section 6.
Q_GRID = (0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9)
TARGET_POWER = 0.80
# Stage 2: the nine keys that need the enrichment, at algo-4.0.0's 1 million nodes.
STAGE2_CPU_SECONDS_PER_GAME = 39.1  # EXP-0008, decision.md
STAGE2_MAX_CPU_HOURS = 20.0

# Section 0: the registered run's budget, in counted units.
BUDGET: dict[str, float] = {
    # Games sent to `cli lab observe` (each with one or both sides).
    "observe_games": 1_200_000,
    # The start position, once, at 1 million nodes.
    "engine_nodes": 1_000_000,
}
CAP = 1.5

WORKERS = int(os.environ.get("EXP09_WORKERS", "6"))  # speed only; never a result
BATCH = 1000

COMMENT = re.compile(r"\{([^}]*)\}")
EVAL = re.compile(r"\[%eval (#?)(-?\d+(?:\.\d+)?)\]")
HEADER = re.compile(r'^\[(\w+) "(.*)"\]$', re.MULTILINE)


def log(msg: str) -> None:
    print(f"{EXP}: {msg}", file=sys.stderr, flush=True)


# ---- the cohort -----------------------------------------------------------------------------


def path_of(month: str, name: str) -> Path:
    return external_input(f"{DATA}/{month}/{name}")


def fraction(h: str) -> float:
    """The pseudonym's hash fraction, as `lichess-db` computes it (the first 8 bytes)."""
    return int(h[:16], 16) / 2**64


def in_slice(h: str, lo: float, hi: float) -> bool:
    return lo <= fraction(h) < hi


def read_players(month: str, lo: float, hi: float) -> dict[str, dict[str, Any]]:
    out = {}
    with path_of(month, "players.jsonl").open(encoding="utf-8") as f:
        for line in f:
            p = json.loads(line)
            if in_slice(p["h"], lo, hi):
                out[p["h"]] = p
    return out


def mean(xs: list[float]) -> float:
    return sum(xs) / len(xs)


def rating_start(p: dict[str, Any]) -> float:
    return mean([float(x[2]) for x in p["first"][:ENDS]])


def rating_end(p: dict[str, Any]) -> float:
    return mean([float(x[3]) for x in p["last"][-ENDS:]])


def game_key(game_id: str) -> str:
    return hashlib.sha256(bytes.fromhex(SALT) + b"game:" + game_id.encode()).hexdigest()


def eligible(lo: float, hi: float) -> tuple[dict[str, dict[str, Any]], dict[str, int]]:
    """Section 7's rules that read the players files only, in order; the counts of each step."""
    months = {k: read_players(m, lo, hi) for k, m in MONTHS.items()}
    flow: dict[str, int] = {}
    ids = [h for h, p in months["P0"].items() if p["rapid_games"] >= MIN_P0_RAPID]
    flow["p0_history"] = len(ids)
    ids = [h for h in ids if all(months[k].get(h, {}).get("eval_games", 0) >= MIN_EVAL_GAMES
                                 for k in FEATURE_MONTHS)]
    flow["feature_months"] = len(ids)
    ids = [h for h in ids if rating_end(months["P1"][h]) < MAX_RATING]
    flow["audience"] = len(ids)
    stayed = [h for h in ids if months["P3"].get(h, {}).get("rapid_games", 0) >= MIN_P3_RAPID]
    flow["p3_active"] = len(stayed)
    return {h: {k: months[k].get(h) for k in MONTHS} for h in sorted(ids)}, flow


def lichess_evals(pgn: str) -> list[EngineScore | None]:
    """Lichess's `[%eval]` after each ply, White's point of view: one per clock comment, since
    every ply of a clocked game has one and other comments ("White offers draw") are not plies."""
    out: list[EngineScore | None] = []
    for c in COMMENT.findall(pgn.partition("\n\n")[2]):
        if "[%clk" not in c:
            continue
        m = EVAL.search(c)
        if not m:
            out.append(None)
        elif m.group(1) == "#":
            out.append(EngineScore(mate=int(m.group(2))))
        else:
            out.append(EngineScore(cp=round(float(m.group(2)) * 100)))
    return out


def standard(pgn: str) -> bool:
    h = dict(HEADER.findall(pgn.partition("\n\n")[0]))
    return h.get("Variant", "Standard") == "Standard" and "FEN" not in h


def select_games(cohort: dict[str, Any]) -> dict[tuple[str, str], list[tuple[str, str]]]:
    """Per (player, feature month), the player's rapid games with evaluations in that month, by
    their game key: `(key, id)` sorted, every one (the cap is applied after exclusions)."""
    out: dict[tuple[str, str], list[tuple[str, str]]] = defaultdict(list)
    for k in FEATURE_MONTHS:
        with path_of(MONTHS[k], "games.jsonl").open(encoding="utf-8") as f:
            for line in f:
                g = json.loads(line)
                for side in ("white", "black"):
                    h = g[side]
                    if h in cohort:
                        out[(h, k)].append((game_key(g["id"]), g["id"]))
    for v in out.values():
        v.sort()
    return out


def start_search(budget: Budget | None) -> GivenSearch:
    if budget:
        budget.spend("engine_nodes", START_NODES)
    s = rematch_lab.evaluate([START_FEN], nodes=START_NODES).searches[0]
    return GivenSearch(fen=s.fen, score=s.score, best_uci=s.best_uci)


def observe_all(cohort: dict[str, Any], budget: Budget | None) -> tuple[dict, dict[str, int]]:
    """Per (player, month), the player's first OBSERVE_GAMES rapid games with evaluations by game
    key through `cli lab observe`; of those, the first MAX_EVAL_GAMES by game key that are
    standard chess from the start position and observe without an error."""
    start = start_search(budget)
    chosen = {(h, gid) for (h, _), games in select_games(cohort).items()
              for _, gid in games[:OBSERVE_GAMES]}
    wanted: dict[str, dict[str, str]] = {}  # game id -> {color: pseudonym}
    counts = {"games_read": 0, "games_observed": 0, "games_failed": 0, "games_not_standard": 0}
    requests: list[ObserveRequest] = []
    batch: list[ObserveGame] = []
    meta: dict[str, dict[str, Any]] = {}
    for k in FEATURE_MONTHS:
        with path_of(MONTHS[k], "games.jsonl").open(encoding="utf-8") as f:
            for line in f:
                g = json.loads(line)
                sides = [s for s in ("white", "black")
                         if g[s] in cohort and (g[s], g["id"]) in chosen]
                if not sides:
                    continue
                counts["games_read"] += 1
                if not standard(g["pgn"]):
                    counts["games_not_standard"] += 1
                    continue
                # Keyed by month and id, so a game id is never taken for another month's.
                uid = f"{k}:{g['id']}"
                wanted[uid] = {s: g[s] for s in sides}
                own_opp = {
                    "white": (g["white_elo"], g["black_elo"]),
                    "black": (g["black_elo"], g["white_elo"]),
                }
                meta[uid] = {"month": k, "elos": own_opp, "id": g["id"]}
                batch.append(ObserveGame(
                    id=uid, pgn=g["pgn"], base_seconds=g["base_seconds"],
                    increment_seconds=g["increment_seconds"], lichess_evals=lichess_evals(g["pgn"]),
                    sides=sides, white_rating=g["white_elo"], black_rating=g["black_elo"],
                    termination=g["termination"],
                ))
                if len(batch) == BATCH:
                    requests.append(ObserveRequest(games=batch, searches=[start]))
                    batch = []
    if batch:
        requests.append(ObserveRequest(games=batch, searches=[start]))
    n_games = sum(len(r.games) for r in requests)
    if budget:
        budget.spend("observe_games", n_games)
    log(f"observing {n_games} games in {len(requests)} batches on {WORKERS} workers")
    algorithm_ids = set()
    observed: dict[tuple[str, str], list[tuple[str, str, Any]]] = defaultdict(list)
    with ThreadPoolExecutor(max_workers=WORKERS) as pool:
        for resp in pool.map(rematch_lab.observe, requests):
            algorithm_ids.add(resp.algorithm_id)
            for og in resp.games:
                if og.error:
                    counts["games_failed"] += 1
                    continue
                counts["games_observed"] += 1
                m = meta[og.id]
                for s in og.sides:
                    h = wanted[og.id][s.color]
                    observed[(h, m["month"])].append(
                        (game_key(m["id"]), m["id"], (s, m["elos"][s.color])))
    if len(algorithm_ids) != 1:
        raise SystemExit(f"the run spans algorithm ids {sorted(algorithm_ids)}")
    for v in observed.values():
        v.sort(key=lambda x: (x[0], x[1]))
        del v[MAX_EVAL_GAMES:]
    return observed, {**counts, "algorithm_id": algorithm_ids.pop()}  # type: ignore[dict-item]


def month_measures(obs: list[tuple[str, str, Any]]) -> dict[str, Any]:
    """A player's quantities in one month: each key's rate per chance (events over
    opportunities, missing below MIN_KEY_OPPORTUNITIES), the blunder rate per move, the mean
    game accuracy, and the mean of (opponent − own) rating."""
    out: dict[str, Any] = {"games": len(obs)}
    for key in KEYS:
        e = sum(s.keys[key].events for _, _, (s, _) in obs if key in s.keys)
        o = sum(s.keys[key].opportunities for _, _, (s, _) in obs if key in s.keys)
        out[key] = e / o if o >= MIN_KEY_OPPORTUNITIES else None
        out[f"{key}:opps"] = o
    moves = sum(s.moves for _, _, (s, _) in obs)
    out["blunder_rate"] = sum(s.blunders for _, _, (s, _) in obs) / moves if moves else None
    out["blunder_rate:opps"] = moves
    acc = [s.accuracy for _, _, (s, _) in obs if s.accuracy is not None]
    out["accuracy"] = mean(acc) if acc else None
    out["accuracy:opps"] = len(acc)
    gaps = [float(opp - own) for _, _, (_, (own, opp)) in obs if own is not None and opp is not None]
    out["opp_gap"] = mean(gaps) if gaps else 0.0
    return out


def build_table(lo: float, hi: float, budget: Budget | None) -> tuple[list[dict[str, Any]], dict]:
    cohort, flow = eligible(lo, hi)
    observed, counts = observe_all(cohort, budget)
    rows = []
    enough = 0
    for h, months in cohort.items():
        feats = {k: observed.get((h, k), []) for k in FEATURE_MONTHS}
        if any(len(v) < MIN_EVAL_GAMES for v in feats.values()):
            continue
        enough += 1
        p0, p1, p2, p3 = (months[k] for k in ("P0", "P1", "P2", "P3"))
        row: dict[str, Any] = {
            "h": h,
            "r0_start": rating_start(p0),
            "r0_end": rating_end(p0),
            "r1_end": rating_end(p1),
            "r2_start": rating_start(p2),
            "r2_end": rating_end(p2),
            "games0": p0["rapid_games"],
            "games1": p1["rapid_games"],
            "games2": p2["rapid_games"],
            "rapid_share2": p2["rapid_games"]
            / (p2["rapid_games"] + p2["bullet"] + p2["blitz"] + p2["classical"]),
            "m1": month_measures(feats["P1"]),
            "m2": month_measures(feats["P2"]),
        }
        if p3 is not None and p3["rapid_games"] >= MIN_P3_RAPID:
            row["r3_end"] = rating_end(p3)
            row["games3"] = p3["rapid_games"]
        rows.append(row)
    flow["observed_games"] = enough
    flow["with_outcome"] = sum(1 for r in rows if "r3_end" in r)
    return rows, {"flow": flow, "games": counts}


# ---- the models -----------------------------------------------------------------------------


def oriented(q: str, x: float) -> float:
    return x if q in HIGHER_IS_BETTER else -x


def band(r: float) -> str:
    return "B0" if r < 900 else "B1" if r < 1300 else "B2" if r < 1700 else "B3"


def design(rows: list[dict[str, Any]], q: str, family: str) -> tuple[np.ndarray, np.ndarray, list[str]]:
    """The registered regression of one quantity (section 8). The first column after the
    constant is the standardised exposure; its coefficient is the effect, in Elo per SD."""
    ys, xs = [], []
    for r in rows:
        a, b = r["m1"][q], r["m2"][q]
        if a is None or b is None:
            continue
        opps = (math.log1p(r["m1"][f"{q}:opps"]), math.log1p(r["m2"][f"{q}:opps"]))
        games = (math.log1p(r["games1"]), math.log1p(r["games2"]))
        if family == "primary":
            if "r3_end" not in r:
                continue
            y = r["r3_end"] - r["r2_end"]
            x = [oriented(q, b - a), oriented(q, a), (r["r2_end"] - 1500) / 100,
                 r["r2_end"] - r["r1_end"], r["r1_end"] - r["r0_end"], r["r0_end"] - r["r0_start"],
                 *games, math.log1p(r["games3"]), r["rapid_share2"], *opps,
                 (r["m2"]["opp_gap"] - r["m1"]["opp_gap"]) / 100]
        elif family == "contemporaneous":
            y = r["r2_end"] - r["r1_end"]
            x = [oriented(q, b - a), oriented(q, a), (r["r1_end"] - 1500) / 100,
                 r["r1_end"] - r["r0_end"], r["r0_end"] - r["r0_start"], *games,
                 r["rapid_share2"], *opps, (r["m2"]["opp_gap"] - r["m1"]["opp_gap"]) / 100]
        elif family == "level":
            if "r3_end" not in r:
                continue
            y = r["r3_end"] - r["r1_end"]
            x = [oriented(q, a), (r["r1_end"] - 1500) / 100, r["r1_end"] - r["r0_end"],
                 r["r0_end"] - r["r0_start"], math.log1p(r["games0"]), games[0],
                 opps[0], r["m1"]["opp_gap"] / 100]
        else:
            raise ValueError(family)
        ys.append(y)
        xs.append(x)
    if not xs:
        return np.zeros(0), np.zeros((0, 0)), []
    X = np.asarray(xs, dtype=float)
    if len(X):
        sd = X[:, 0].std(ddof=1)
        X[:, 0] = (X[:, 0] - X[:, 0].mean()) / sd if sd > 0 else 0.0
    return np.asarray(ys, dtype=float), sm.add_constant(X, has_constant="add"), [r["h"] for r in rows]


def fit(rows: list[dict[str, Any]], q: str, family: str) -> dict[str, Any]:
    y, X, _ = design(rows, q, family)
    n = len(y)
    if n < 30:
        return {"n": n, "estimate": None}
    res = sm.OLS(y, X).fit(cov_type="HC3")
    est, se = float(res.params[1]), float(res.bse[1])
    out: dict[str, Any] = {"n": n, "estimate": est, "se": se, "p": float(res.pvalues[1])}
    for level in (*LEVELS, SIMULTANEOUS):
        z = norm.ppf(1 - (1 - level) / 2)
        out[f"ci{level * 100:g}"] = [est - z * se, est + z * se]
    # Power inputs (section 6): the residual SD of y and the SD of the exposure net of the rest.
    out["resid_sd"] = float(np.sqrt(res.scale))
    rest = sm.OLS(X[:, 1], np.delete(X, 1, axis=1)).fit()
    out["exposure_sd_net"] = float(np.sqrt(rest.scale))
    out["y_sd"] = float(np.std(y, ddof=1))
    return out


def family(rows: list[dict[str, Any]], name: str) -> dict[str, Any]:
    fits = {q: fit(rows, q, name) for q in QUANTITIES}
    tested = [q for q in QUANTITIES if fits[q]["estimate"] is not None]
    if tested:
        reject, adj, _, _ = multipletests([fits[q]["p"] for q in tested], alpha=ALPHA, method="holm")
        for q, r, a in zip(tested, reject, adj, strict=True):
            fits[q]["p_holm"] = float(a)
            fits[q]["holm_reject"] = bool(r)
    for q in QUANTITIES:
        f = fits[q]
        if f["estimate"] is None:
            f["reads"] = "not estimable"
            continue
        lo, hi = f[f"ci{SIMULTANEOUS * 100:g}"]
        if f["holm_reject"] and f["estimate"] > 0:
            f["reads"] = "precedes climbing"
        elif f["holm_reject"]:
            f["reads"] = "precedes falling"
        elif -DELTA < lo and hi < DELTA:
            f["reads"] = "below the minimum effect"
        else:
            f["reads"] = "undetermined"
    return fits


def by_band(rows: list[dict[str, Any]]) -> dict[str, Any]:
    out = {}
    for b in ("B0", "B1", "B2", "B3"):
        sub = [r for r in rows if band(r["r1_end"]) == b]
        out[b] = {"players": len(sub),
                  **{q: {k: v for k, v in fit(sub, q, "primary").items()
                         if k in ("n", "estimate", "ci95")} for q in QUANTITIES}}
    return out


def attrition(rows: list[dict[str, Any]]) -> dict[str, Any]:
    """Exploratory: does a quantity in P1 or its change predict staying active in P3?"""
    out = {}
    for q in QUANTITIES:
        sub = [r for r in rows if r["m1"][q] is not None and r["m2"][q] is not None]
        if len(sub) < 30:
            continue
        stay = np.array([1.0 if "r3_end" in r else 0.0 for r in sub])
        ch = np.array([oriented(q, r["m2"][q] - r["m1"][q]) for r in sub])
        sd = ch.std(ddof=1)
        X = sm.add_constant(((ch - ch.mean()) / sd if sd > 0 else ch).reshape(-1, 1))
        res = sm.OLS(stay, X).fit(cov_type="HC3")
        out[q] = {"n": len(sub), "stayed": float(stay.mean()),
                  "per_sd": float(res.params[1]), "se": float(res.bse[1])}
    return out


def decide(primary: dict[str, Any]) -> dict[str, Any]:
    """Section 9, in order."""
    climbing = [q for q in KEYS if primary[q]["reads"] == "precedes climbing"]
    small = [q for q in KEYS if primary[q]["reads"] == "below the minimum effect"]
    control = primary[POSITIVE_CONTROL]["reads"] == "precedes climbing"
    if climbing:
        outcome = "candidates to teach"
    elif small and len(small) >= SMALL_KEYS_FOR_NO_ORDER and control:
        outcome = "no data-driven order from these keys"
    else:
        outcome = "need more data"
    return {"outcome": outcome, "climbing": climbing, "below_minimum": small,
            "positive_control_holds": control}


def round_floats(x: Any) -> Any:
    if isinstance(x, float):
        return round(x, 6)
    if isinstance(x, dict):
        return {k: round_floats(v) for k, v in x.items()}
    if isinstance(x, list):
        return [round_floats(v) for v in x]
    return x


def analyse(rows: list[dict[str, Any]]) -> dict[str, Any]:
    primary = family(rows, "primary")
    return {
        "primary": primary,
        "decision": decide(primary),
        "contemporaneous": family(rows, "contemporaneous"),
        "level": family(rows, "level"),
        "by_band": by_band(rows),
        "attrition": attrition(rows),
        "rating_change_sd": float(np.std([r["r3_end"] - r["r2_end"] for r in rows if "r3_end" in r],
                                         ddof=1)) if len(rows) > 2 else None,
    }


# ---- power ----------------------------------------------------------------------------------


def power_of(n: float, f: dict[str, Any]) -> dict[str, float]:
    """Closed form for the registered test (an HC3 t-test, read at the simultaneous level):
    the chance of 'precedes climbing' at a true effect of DELTA, and of 'below the minimum
    effect' at a true effect of 0."""
    se = f["resid_sd"] / (math.sqrt(n) * f["exposure_sd_net"])
    z = norm.ppf(1 - (1 - SIMULTANEOUS) / 2)
    climb = float(norm.cdf(DELTA / se - z))
    small = float(max(0.0, 2 * norm.cdf((DELTA - z * se) / se) - 1))
    return {"se": se, "power_climbing": climb, "power_small": small}


def cmd_power(pilot_path: Path) -> None:
    pilot = json.loads(pilot_path.read_text(encoding="utf-8"))
    slice_width = PILOT_SLICE[1] - PILOT_SLICE[0]
    grid = {}
    chosen = None
    for q_ in Q_GRID:
        per = {}
        for q in QUANTITIES:
            f = pilot["primary"][q]
            if f.get("estimate") is None:
                continue
            n = f["n"] * q_ / slice_width
            per[q] = {"n": n, **power_of(n, f)}
        worst = min(min(v["power_climbing"], v["power_small"]) for v in per.values())
        grid[f"{q_:g}"] = {"per_quantity": per, "worst": worst}
        if chosen is None and worst >= TARGET_POWER:
            chosen = q_
    # Stage 2: players needed for the median key to reach the target, and its cost.
    keys = [pilot["primary"][q] for q in KEYS if pilot["primary"][q].get("estimate") is not None]
    z = norm.ppf(1 - (1 - SIMULTANEOUS) / 2) + norm.ppf(TARGET_POWER)
    need = sorted((z * f["resid_sd"] / (DELTA * f["exposure_sd_net"])) ** 2 for f in keys)
    n2 = need[len(need) // 2] if need else None
    games = 2 * MAX_EVAL_GAMES
    out = {
        "experiment": EXP,
        "q_grid": grid,
        "q": chosen if chosen is not None else Q_GRID[-1],
        "reaches_target": chosen is not None,
        "stage2": {
            "players_for_median_key": n2,
            "games_per_player": games,
            "cpu_hours": n2 * games * STAGE2_CPU_SECONDS_PER_GAME / 3600 if n2 else None,
            "max_cpu_hours": STAGE2_MAX_CPU_HOURS,
        },
    }
    (HERE / "power.json").write_text(json.dumps(round_floats(out), indent=2, sort_keys=True) + "\n",
                                     encoding="utf-8")
    log(f"q = {out['q']}, stage 2 {out['stage2']}")


# ---- the runs -------------------------------------------------------------------------------


def meta_of_months() -> dict[str, Any]:
    out = {}
    for k, m in MONTHS.items():
        meta = json.loads(path_of(m, "meta.json").read_text(encoding="utf-8"))
        if meta["salt"] != SALT or meta["player_rate"] != 1.0:
            raise SystemExit(f"{m}: built with another salt or player rate")
        out[k] = {f: meta[f] for f in ("month", "url", "sha256", "compressed_bytes", "games_seen",
                                       "games_kept", "players", "eval_games")}
    return out


def cmd_pilot(out: Path) -> None:
    rows, counts = build_table(*PILOT_SLICE, None)
    result = {"experiment": EXP, "slice": list(PILOT_SLICE), "months": meta_of_months(),
              **counts, "players": len(rows), **analyse(rows)}
    out.mkdir(parents=True, exist_ok=True)
    (out / "pilot.json").write_text(json.dumps(round_floats(result), indent=2, sort_keys=True) + "\n",
                                    encoding="utf-8")


def figure(primary: dict[str, Any], path: Path) -> None:
    import matplotlib.pyplot as plt

    qs = [q for q in QUANTITIES if primary[q]["estimate"] is not None]
    fig, ax = plt.subplots(figsize=(7, 4.2))
    for i, q in enumerate(qs):
        f = primary[q]
        lo, hi = f[f"ci{SIMULTANEOUS * 100:g}"]
        lo95, hi95 = f["ci95"]
        ax.plot([lo, hi], [i, i], color="#8aa4c8", lw=2)
        ax.plot([lo95, hi95], [i, i], color="#2f5d9b", lw=4)
        ax.plot([f["estimate"]], [i], "o", color="#16324f")
    ax.axvline(0, color="#444", lw=0.8)
    for d in (-DELTA, DELTA):
        ax.axvline(d, color="#999", lw=0.8, ls="--")
    ax.set_yticks(range(len(qs)), qs)
    ax.set_xlabel("Next month's rating change per SD of improvement (Elo); 95% and simultaneous")
    fig.tight_layout()
    path.parent.mkdir(parents=True, exist_ok=True)
    fig.savefig(path, dpi=120, metadata={"Software": None})
    plt.close(fig)


def cmd_run(out: Path, algorithm: str | None) -> None:
    algorithm_id = rematch_lab.algorithm()["id"]
    if algorithm is not None and algorithm_id != algorithm:
        raise SystemExit(f"the cli is {algorithm_id}, not the requested {algorithm}")
    power = json.loads((HERE / "power.json").read_text(encoding="utf-8"))
    q = float(power["q"])
    if not 0 < q <= PILOT_SLICE[0]:
        raise SystemExit("the confirmatory slice must not reach the pilot's")
    budget = Budget(BUDGET, CAP)
    rows, counts = build_table(0.0, q, budget)
    if counts["games"]["algorithm_id"] != algorithm_id:
        raise SystemExit("observe answered on another algorithm")
    analysed = analyse(rows)
    stage2 = dict(power["stage2"])
    stage2["runs"] = stage2["cpu_hours"] is not None and stage2["cpu_hours"] <= STAGE2_MAX_CPU_HOURS
    results = {
        "experiment": EXP,
        "algorithm_id": algorithm_id,
        "design": {"seed_namespace": EXP, "slice": [0.0, q], "salt": SALT, "months": MONTHS,
                   "delta": DELTA, "simultaneous_level": SIMULTANEOUS},
        "months": meta_of_months(),
        **counts,
        "players": len(rows),
        **analysed,
        "stage2": stage2,
        "compute": budget.record(),
    }
    write_results(out, round_floats(results))
    figure(analysed["primary"], out / "figures" / "primary.png")


def main(argv: list[str] | None = None) -> int:
    argv = sys.argv[1:] if argv is None else argv
    if argv and argv[0] in ("pilot", "power"):
        p = argparse.ArgumentParser(prog="analysis.py")
        p.add_argument("cmd")
        p.add_argument("--out", type=Path)
        p.add_argument("--pilot", type=Path)
        a = p.parse_args(argv)
        if a.cmd == "pilot":
            cmd_pilot(a.out)
        else:
            cmd_power(a.pilot)
        return 0
    p = argparse.ArgumentParser(prog="analysis.py")
    p.add_argument("--out", type=Path, required=True)
    p.add_argument("--algorithm", help="the algorithm_id the run must be on (the arena's)")
    a = p.parse_args(argv)
    cmd_run(a.out, a.algorithm)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

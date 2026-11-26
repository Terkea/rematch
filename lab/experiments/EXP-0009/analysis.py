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
QUANTITIES = (*KEYS, "accuracy")  # the primary family
EXPLORATORY = ("blunder_rate",)
HIGHER_IS_BETTER = {"accuracy"}
POSITIVE_CONTROL = "accuracy"

# Sections 5, 8 and 9.
DELTA = 14.0  # Elo of next-month rating per SD of improvement (0.02 points a game, ADR 0101)
ALPHA = 0.05
LEVELS = (0.80, 0.95, 0.975)
SIMULTANEOUS = 1 - ALPHA / len(QUANTITIES)  # Bonferroni, the interval read against DELTA
SMALL_KEYS_FOR_NO_ORDER = 6
MIN_PLAYERS = 30
RELIABILITY_FLOOR = 0.01

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


def read_players(month: str, lo: float, hi: float,
                 keep: set[str] | None = None) -> dict[str, dict[str, Any]]:
    """The month's players in the slice (and in `keep`, when given); memory only, no result
    depends on the filter's order."""
    out = {}
    with path_of(month, "players.jsonl").open(encoding="utf-8") as f:
        for line in f:
            h = line[6:38]  # '{"h":"<32 hex>"', the line's first field
            if keep is not None and h not in keep:
                continue
            p = json.loads(line)
            if p["h"] == h and in_slice(h, lo, hi):
                out[h] = p
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
    p0 = read_players(MONTHS["P0"], lo, hi)
    history = {h for h, p in p0.items() if p["rapid_games"] >= MIN_P0_RAPID}
    months = {"P0": {h: p0[h] for h in history}}
    for k in ("P1", "P2", "P3"):
        months[k] = read_players(MONTHS[k], lo, hi, history)
    flow: dict[str, int] = {}
    ids = sorted(history)
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


def observe_all(cohort: dict[str, Any], budget: Budget | None, lo: float, hi: float
                ) -> tuple[dict, dict[str, int]]:
    """Per (player, month), the player's first OBSERVE_GAMES rapid games with evaluations by game
    key through `cli lab observe`; of those, the first MAX_EVAL_GAMES by game key that are
    standard chess from the start position and observe without an error. Batches go to the
    workers as they are read, at most 2 × WORKERS in flight, so memory stays bounded; the result
    does not depend on the order they come back in (every list is sorted by game key)."""
    start = start_search(budget)
    chosen = {(h, gid) for (h, _), games in select_games(cohort).items()
              for _, gid in games[:OBSERVE_GAMES]}
    counts = {"games_read": 0, "games_observed": 0, "games_failed": 0, "games_not_standard": 0,
              "games_with_an_opponent_in_the_other_slice": 0}
    other = (0.0, PILOT_SLICE[0]) if lo >= PILOT_SLICE[0] else PILOT_SLICE
    algorithm_ids: set[str] = set()
    observed: dict[tuple[str, str], list[tuple[str, str, Any]]] = defaultdict(list)

    def collect(resp: Any, meta: dict[str, dict[str, Any]]) -> None:
        algorithm_ids.add(resp.algorithm_id)
        for og in resp.games:
            if og.error:
                counts["games_failed"] += 1
                continue
            counts["games_observed"] += 1
            m = meta[og.id]
            for s in og.sides:
                observed[(m["sides"][s.color], m["month"])].append(
                    (game_key(m["id"]), m["id"], (s, m["elos"][s.color])))

    def batches() -> Any:
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
                    if any(g[s] and in_slice(g[s], *other) for s in ("white", "black")):
                        counts["games_with_an_opponent_in_the_other_slice"] += 1
                    if not standard(g["pgn"]):
                        counts["games_not_standard"] += 1
                        continue
                    # Keyed by month and id, so a game id is never taken for another month's.
                    uid = f"{k}:{g['id']}"
                    meta[uid] = {
                        "month": k, "id": g["id"], "sides": {s: g[s] for s in sides},
                        "elos": {"white": (g["white_elo"], g["black_elo"]),
                                 "black": (g["black_elo"], g["white_elo"])},
                    }
                    batch.append(ObserveGame(
                        id=uid, pgn=g["pgn"], base_seconds=g["base_seconds"],
                        increment_seconds=g["increment_seconds"],
                        lichess_evals=lichess_evals(g["pgn"]), sides=sides,
                        white_rating=g["white_elo"], black_rating=g["black_elo"],
                        termination=g["termination"],
                    ))
                    if len(batch) == BATCH:
                        yield ObserveRequest(games=batch, searches=[start]), meta
                        batch, meta = [], {}
        if batch:
            yield ObserveRequest(games=batch, searches=[start]), meta

    n_games = 0
    with ThreadPoolExecutor(max_workers=WORKERS) as pool:
        pending: list[Any] = []
        for req, meta in batches():
            if budget:
                budget.spend("observe_games", len(req.games))
            n_games += len(req.games)
            pending.append((pool.submit(rematch_lab.observe, req), meta))
            while len(pending) >= 2 * WORKERS:
                fut, m = pending.pop(0)
                collect(fut.result(), m)
        for fut, m in pending:
            collect(fut.result(), m)
    log(f"observed {n_games} games on {WORKERS} workers")
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


def halves(obs: list[tuple[str, str, Any]]) -> tuple[dict[str, Any], dict[str, Any]]:
    """The month's quantities on its odd and even games by game key, for the split-half
    reliability of section 8."""
    return month_measures(obs[0::2]), month_measures(obs[1::2])


def build_table(lo: float, hi: float, budget: Budget | None) -> tuple[list[dict[str, Any]], dict]:
    cohort, flow = eligible(lo, hi)
    observed, counts = observe_all(cohort, budget, lo, hi)
    rows = []
    enough = 0
    for h, months in cohort.items():
        feats = {k: observed.get((h, k), []) for k in FEATURE_MONTHS}
        if any(len(v) < MIN_EVAL_GAMES for v in feats.values()):
            continue
        enough += 1
        p0, p1, p2, p3 = (months[k] for k in ("P0", "P1", "P2", "P3"))
        total = p1["rapid_games"] + p1["bullet"] + p1["blitz"] + p1["classical"]
        a1, b1 = halves(feats["P1"])
        a2, b2 = halves(feats["P2"])
        row: dict[str, Any] = {
            "h": h,
            "r0_start": rating_start(p0),
            "r0_end": rating_end(p0),
            "r1_end": rating_end(p1),
            "r2_end": rating_end(p2),
            "games0": p0["rapid_games"],
            "games1": p1["rapid_games"],
            "games2": p2["rapid_games"],
            "rapid_share1": p1["rapid_games"] / total,
            "analysed_share1": p1["eval_games"] / p1["rapid_games"],
            "analysed_share2": p2["eval_games"] / p2["rapid_games"],
            "m1": month_measures(feats["P1"]),
            "m2": month_measures(feats["P2"]),
            "halves": {"m1a": a1, "m1b": b1, "m2a": a2, "m2b": b2},
        }
        if p3 is not None and p3["rapid_games"] >= MIN_P3_RAPID:
            row["r3_end"] = rating_end(p3)
        rows.append(row)
    flow["observed_games"] = enough
    flow["with_outcome"] = sum(1 for r in rows if "r3_end" in r)
    bands: dict[str, int] = defaultdict(int)
    for r in rows:
        bands[band(r["r1_end"])] += 1
    flow["by_band"] = dict(sorted(bands.items()))
    return rows, {"flow": flow, "games": counts}


# ---- the models -----------------------------------------------------------------------------


def oriented(q: str, x: float) -> float:
    return x if q in HIGHER_IS_BETTER else -x


def band(r: float) -> str:
    return "B0" if r < 900 else "B1" if r < 1300 else "B2" if r < 1700 else "B3"


def base_covariates(r: dict[str, Any], q: str, a: float) -> list[float]:
    """What every change model adjusts for, each measured before the outcome window and none a
    consequence of the exposure's rating effect: the P1 level, the P1 end rating, the change from
    P0 to P1 and within P0, rapid games in P0 to P2, the P1 rapid share, the analysed share of
    rapid games in P1 and P2, and the opportunities in P1 and P2."""
    return [oriented(q, a), (r["r1_end"] - 1500) / 100, r["r1_end"] - r["r0_end"],
            r["r0_end"] - r["r0_start"], math.log1p(r["games0"]), math.log1p(r["games1"]),
            math.log1p(r["games2"]), r["rapid_share1"], r["analysed_share1"], r["analysed_share2"],
            math.log1p(r["m1"][f"{q}:opps"]), math.log1p(r["m2"][f"{q}:opps"])]


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
        if family in ("primary", "adjusted"):
            if "r3_end" not in r:
                continue
            y = r["r3_end"] - r["r2_end"]
            x = [change, *base_covariates(r, q, a)]
            if family == "adjusted":
                x += [r["r2_end"] - r["r1_end"], gap]
        elif family == "contemporaneous":
            y = r["r2_end"] - r["r1_end"]
            x = [change, *base_covariates(r, q, a), gap]
        elif family == "level":
            if "r3_end" not in r:
                continue
            y = r["r3_end"] - r["r1_end"]
            x = [oriented(q, a), (r["r1_end"] - 1500) / 100, r["r1_end"] - r["r0_end"],
                 r["r0_end"] - r["r0_start"], math.log1p(r["games0"]), math.log1p(r["games1"]),
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


def reliability(used: list[dict[str, Any]], q: str, family: str) -> float | None:
    """The split-half reliability of the exposure (Spearman-Brown) on the players the model
    used: the P1-to-P2 change (or, for the level family, the P1 level) on odd games against even
    games by game key."""
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
    if len(xa) < MIN_PLAYERS or np.std(xa) == 0 or np.std(xb) == 0:
        return None
    rho = float(np.corrcoef(xa, xb)[0, 1])
    if rho <= 0:
        return RELIABILITY_FLOOR
    return min(1.0, max(RELIABILITY_FLOOR, 2 * rho / (1 + rho)))


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
    out: dict[str, Any] = {"n": n, "estimate": est, "se": se, "p": float(res.pvalues[1])}
    for level in (*LEVELS, SIMULTANEOUS):
        z = norm.ppf(1 - (1 - level) / 2)
        out[f"ci{level * 100:g}"] = [est - z * se, est + z * se]
    rel = reliability(used, q, family)
    out["reliability"] = rel
    # The minimum effect in the coefficient's own units: per SD of the observed exposure, which
    # is per SD of the true one times √reliability (section 5).
    out["delta_observed"] = DELTA * math.sqrt(rel) if rel is not None else None
    out["disattenuated"] = est / math.sqrt(rel) if rel is not None else None
    return out


def family(rows: list[dict[str, Any]], name: str,
           weights: dict[str, float] | None = None) -> dict[str, Any]:
    fits = {q: fit(rows, q, name, weights) for q in QUANTITIES}
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
        d = f["delta_observed"]
        if f["holm_reject"] and f["estimate"] > 0:
            f["reads"] = "precedes climbing"
        elif f["holm_reject"]:
            f["reads"] = "precedes falling"
        elif d is not None and -d < lo and hi < d:
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


def staying(rows: list[dict[str, Any]], q: str) -> tuple[np.ndarray, np.ndarray, list[dict]]:
    """Staying active in P3 on the primary's exposure and covariates (no outcome term)."""
    ys, xs, used = [], [], []
    for r in rows:
        a, b = r["m1"][q], r["m2"][q]
        if a is None or b is None:
            continue
        ys.append(1.0 if "r3_end" in r else 0.0)
        xs.append([oriented(q, b - a), *base_covariates(r, q, a)])
        used.append(r)
    if not xs:
        return np.zeros(0), np.zeros((0, 0)), []
    X = np.asarray(xs, dtype=float)
    sd = X[:, 0].std(ddof=1) if len(X) > 1 else 0.0
    X[:, 0] = (X[:, 0] - X[:, 0].mean()) / sd if sd > 0 else 0.0
    return np.asarray(ys), sm.add_constant(X, has_constant="add"), used


def attrition(rows: list[dict[str, Any]]) -> dict[str, Any]:
    """Section 8's attrition analyses: whether the exposure predicts staying active (a linear
    probability model with HC3), and the primary re-fitted with inverse-probability-of-staying
    weights from a logistic model on the same terms (a sensitivity, never a decision)."""
    out = {}
    for q in QUANTITIES:
        y, X, used = staying(rows, q)
        if len(y) < MIN_PLAYERS or y.min() == y.max():
            continue
        lpm = sm.OLS(y, X).fit(cov_type="HC3")
        entry: dict[str, Any] = {"n": len(y), "stayed": float(y.mean()),
                                 "per_sd": float(lpm.params[1]), "se": float(lpm.bse[1])}
        try:
            p = sm.Logit(y, X).fit(disp=0, maxiter=200).predict(X)
            w = {r["h"]: 1.0 / max(float(pi), 0.05) for r, pi in zip(used, p, strict=True)}
            ipw = fit(rows, q, "primary", w)
            entry["ipw_primary"] = {k: ipw.get(k) for k in ("n", "estimate", "ci95")}
        except Exception as e:  # noqa: BLE001 - a failed fit is reported, never hidden
            entry["ipw_primary"] = {"error": type(e).__name__}
        out[q] = entry
    return out


def decide(primary: dict[str, Any]) -> dict[str, Any]:
    """Section 9, in order."""
    climbing = [q for q in KEYS if primary[q]["reads"] == "precedes climbing"]
    small = [q for q in KEYS if primary[q]["reads"] == "below the minimum effect"]
    control = primary[POSITIVE_CONTROL]["reads"] == "precedes climbing"
    if climbing:
        outcome = "candidates to teach"
    elif len(small) >= SMALL_KEYS_FOR_NO_ORDER and control:
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
    with_outcome = [r["r3_end"] - r["r2_end"] for r in rows if "r3_end" in r]
    return {
        "primary": primary,
        "decision": decide(primary),
        "adjusted": family(rows, "adjusted"),
        "contemporaneous": family(rows, "contemporaneous"),
        "level": family(rows, "level"),
        "exploratory": {q: fit(rows, q, "primary") for q in EXPLORATORY},
        "by_band": by_band(rows),
        "attrition": attrition(rows),
        "rating_change_sd": float(np.std(with_outcome, ddof=1)) if len(with_outcome) > 2 else None,
    }


# ---- power ----------------------------------------------------------------------------------


POWER_INPUTS = ("n", "se", "reliability", "delta_observed")


def power_of(n: float, f: dict[str, Any]) -> dict[str, float]:
    """The registered read at n players (section 6): the pilot's HC3 standard error scaled by
    √(n_pilot / n); the chance of 'precedes climbing' at a true effect of DELTA per true SD
    (DELTA × √reliability per observed SD), and of 'below the minimum effect' at 0. Bonferroni
    at the simultaneous level stands in for Holm, so both are slightly conservative."""
    se = f["se"] * math.sqrt(f["n"] / n)
    z = norm.ppf(1 - (1 - SIMULTANEOUS) / 2)
    d = f["delta_observed"]
    climb = float(norm.cdf(d / se - z))
    small = float(max(0.0, 2 * norm.cdf((d - z * se) / se) - 1))
    return {"n": n, "se": se, "power_climbing": climb, "power_small": small}


def cmd_power(pilot_path: Path) -> None:
    pilot = json.loads(pilot_path.read_text(encoding="utf-8"))
    width = PILOT_SLICE[1] - PILOT_SLICE[0]
    grid = {}
    chosen = None
    usable = {q: f for q, f in pilot["primary"].items() if f.get("delta_observed") is not None}

    def at_target(p: dict[str, float]) -> bool:
        return min(p["power_climbing"], p["power_small"]) >= TARGET_POWER

    for q_ in Q_GRID:
        per = {q: power_of(f["n"] * q_ / width, f) for q, f in usable.items()}
        keys_ok = sum(1 for q in KEYS if q in per and at_target(per[q]))
        control_ok = (POSITIVE_CONTROL in per
                      and per[POSITIVE_CONTROL]["power_climbing"] >= TARGET_POWER)
        grid[f"{q_:g}"] = {"per_quantity": per, "keys_at_target": keys_ok,
                           "control_at_target": control_ok}
        if chosen is None and keys_ok >= SMALL_KEYS_FOR_NO_ORDER and control_ok:
            chosen = q_
    # Stage 2: the players the median key needs at the target, and its engine cost.
    z = norm.ppf(1 - (1 - SIMULTANEOUS) / 2) + norm.ppf(TARGET_POWER)
    need = sorted((z * f["se"] * math.sqrt(f["n"]) / f["delta_observed"]) ** 2
                  for q, f in usable.items() if q in KEYS)
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
    """The development slice: only what the power analysis needs (n, the HC3 standard error,
    the reliability) and the counts, never an estimate or a read (section 6)."""
    rows, counts = build_table(*PILOT_SLICE, None)
    primary = family(rows, "primary")
    result = {"experiment": EXP, "slice": list(PILOT_SLICE), "months": meta_of_months(),
              **counts, "players": len(rows),
              "primary": {q: {k: f.get(k) for k in POWER_INPUTS} for q, f in primary.items()}}
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

"""EXP-0001: how many synthetic players and games the Tier 1 benchmark needs (preregistration.md).

One run of the real pipeline (`cli bench run --suite-file`, through `rematch_lab.bench`) on a
registered pilot suite: 6 players per flaw at the smoke suite's rates and 2 clean controls, 120
games each, checkpoints at 30, 45, 60, 80 and 120 games, Stockfish at 12,000 nodes. Then a Monte
Carlo power analysis of the registered decision rule, with the player as the unit:

- recall at 2 (primary): a paired comparison of two algorithm versions on the same players, the
  candidate 0.1 better; the rule ships when the two-sided 97.5% Newcombe interval of the paired
  difference lies above 0. The baseline hit probability of each flaw is drawn from its Jeffreys
  posterior given the pilot's players, so the pilot's own uncertainty is carried into the power.
- interval coverage (secondary): the 95% half-width a suite reaches, and the power for a paired
  change of 0.05, by resampling the pilot's players within each flaw.

    python analysis.py --out DIR    # DIR/results.json and DIR/figures/
"""

from __future__ import annotations

import argparse
import json
import math
import sys
import tempfile
from pathlib import Path

import numpy as np
from scipy import stats

import rematch_lab as lab
from rematch_lab.experiments import seed, write_results

EXP = "EXP-0001"
# The seed namespace (--seed-namespace); the registered run uses the experiment id.
SEED_NS = EXP

# ---- the pilot suite (section 7) ---------------------------------------------------------------
FLAWS = [
    ("hang", 0.05, "W1"),
    ("miss_mate", 0.5, "W4.missed"),
    ("opening_blunder", 0.3, "W7.traps"),
    ("low_clock_blunder", 0.25, "W9b"),
    ("no_convert", 0.5, "W5"),
]
SEEDS_PER_FLAW = 6
CLEAN = 2
GAMES = 120
CHECKPOINTS = [30, 45, 60, 80, 120]
NODES = 12_000
OPPONENTS = 16
ENGINES = 8
# Seeds of the named suites (backend/crates/bench/src/suite.rs), which the pilot must not reuse.
NAMED_SUITE_SEEDS = set(range(11, 17)) | set(range(101, 116))

# ---- the power analysis (sections 5, 6 and 9) --------------------------------------------------
DELTA = 0.10  # recall at 2, absolute
DELTA_COVERAGE = 0.05  # coverage, absolute
CHURN = [0.0, 0.05, 0.10]
PRIMARY_CHURN = 0.10  # the conservative end: churn is assumed, not measured
M_GRID = [1, 2, 3, 4, 5, 6, 8, 10, 12, 14, 16, 20, 24, 28, 32, 40, 48, 64, 80, 100]
SIMS_RECALL = 4000
SIMS_COVERAGE = 2000
SHIP_LEVEL = 0.975
GUARD_LEVEL = 0.80
TARGET_POWER = 0.80
DELTA_GRID = [0.05, 0.10, 0.15, 0.20, 0.25, 0.30, 0.35, 0.40, 0.45, 0.50]

# ---- the budgets and suite shapes (section 9) ---------------------------------------------------
SMOKE_GAMES = 80
SMOKE_CLEAN = 1
SMOKE_BUDGET_GAMES = 1650
FULL_GAMES_OPTIONS = [80, 120]
# The rate sweep leaves the full suite (review, concern 5): the scorer's recall would mix
# players the power analysis does not model.
FULL_SWEEP_PLAYERS = 0
FULL_CLEAN = 2
FULL_BUDGET_GAMES = 9000

SEED_NAMES = [
    *[f"player/{f}/{j}" for f, _, _ in FLAWS for j in range(SEEDS_PER_FLAW)],
    *[f"player/clean/{j}" for j in range(CLEAN)],
    "power/recall",
    "power/coverage",
    "power/smoke",
    "bootstrap/observed",
]


def r6(x: float | None) -> float | None:
    return None if x is None else round(float(x), 6)


# ---- intervals -------------------------------------------------------------------------------


def z_of(level: float) -> float:
    return float(stats.norm.ppf(0.5 + level / 2.0))


def wilson(x: np.ndarray, n: np.ndarray, z: float) -> tuple[np.ndarray, np.ndarray]:
    x = np.asarray(x, dtype=float)
    n = np.asarray(n, dtype=float)
    p = x / n
    denom = 1.0 + z * z / n
    centre = (p + z * z / (2.0 * n)) / denom
    half = z * np.sqrt(p * (1.0 - p) / n + z * z / (4.0 * n * n)) / denom
    return centre - half, centre + half


def newcombe_paired(
    n11: np.ndarray, n10: np.ndarray, n01: np.ndarray, n00: np.ndarray, z: float
) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
    """Newcombe (1998) method 10: the interval of p1 - p2 for paired shares, p1 the candidate's
    (n11 + n10) / n and p2 the reference's (n11 + n01) / n. Returns (difference, low, high)."""
    n11, n10, n01, n00 = (np.asarray(v, dtype=float) for v in (n11, n10, n01, n00))
    n = n11 + n10 + n01 + n00
    p1 = (n11 + n10) / n
    p2 = (n11 + n01) / n
    l1, u1 = wilson(n11 + n10, n, z)
    l2, u2 = wilson(n11 + n01, n, z)
    den = np.sqrt((n11 + n10) * (n01 + n00) * (n11 + n01) * (n10 + n00))
    with np.errstate(invalid="ignore", divide="ignore"):
        phi = np.where(den > 0, (n11 * n00 - n10 * n01) / np.where(den > 0, den, 1.0), 0.0)
    d = p1 - p2
    lo = d - np.sqrt(np.maximum((p1 - l1) ** 2 - 2 * phi * (p1 - l1) * (u2 - p2) + (u2 - p2) ** 2, 0))
    hi = d + np.sqrt(np.maximum((u1 - p1) ** 2 - 2 * phi * (u1 - p1) * (p2 - l2) + (p2 - l2) ** 2, 0))
    return d, lo, hi


# ---- the pilot run ---------------------------------------------------------------------------


def pilot_suite() -> dict:
    players = []
    for f, rate, _ in FLAWS:
        for j in range(SEEDS_PER_FLAW):
            players.append(
                {
                    "name": f"exp1-{f.replace('_', '')}-{j}",
                    "flaw": [f, rate],
                    "games": GAMES,
                    "seed": seed(SEED_NS, f"player/{f}/{j}"),
                }
            )
    for j in range(CLEAN):
        players.append(
            {
                "name": f"exp1-clean-{j}",
                "flaw": None,
                "games": GAMES,
                "seed": seed(SEED_NS, f"player/clean/{j}"),
            }
        )
    seeds = [p["seed"] for p in players]
    if len(set(seeds)) != len(seeds) or set(seeds) & NAMED_SUITE_SEEDS:
        raise SystemExit("the pilot's seeds must be distinct and disjoint from the named suites")
    return {
        "name": "exp-0001-pilot",
        "players": players,
        "checkpoints": CHECKPOINTS,
        "nodes": NODES,
        "opponents": OPPONENTS,
        "perturb": False,
    }


def player_rows(run: dict) -> list[dict]:
    """Per player and checkpoint: the target's rank and interval, whether it is in the top 2,
    and whether the interval holds the true rate (the scorer's truth, `Tally::truth`)."""
    truth = {c["player"]: c.get("truth") for c in run["scorecard"]["cases"]}
    rows = []
    for p in run["players"]:
        flaw = p.get("flaw")
        target = flaw["target_key"] if flaw else None
        cps = []
        for c in p["checkpoints"]:
            ranks = {r["key"]: r["rank"] for r in c["ranked"]}
            est = next((e for e in c["estimates"] if e["key"] == target), None) if target else None
            t = truth.get(p["name"])
            covered = None
            if est is not None and t is not None:
                covered = bool(est["low"] <= t <= est["high"])
            cps.append(
                {
                    "games": c["games"],
                    "rank1": c["ranked"][0]["key"] if c["ranked"] else None,
                    "target_rank": ranks.get(target) if target else None,
                    "hit2": (ranks.get(target, 99) <= 2) if target else None,
                    "estimate": None
                    if est is None
                    else [r6(est["mean"]), r6(est["low"]), r6(est["high"])],
                    "covered": covered,
                }
            )
        rows.append(
            {
                "name": p["name"],
                "flaw": flaw["id"] if flaw else None,
                "target": target,
                "truth": r6(truth.get(p["name"])),
                "games": p["generation"]["games"],
                "mean_plies": r6(p["generation"]["mean_plies"]),
                "time_forfeits": p["generation"]["time_forfeits"],
                "injected_moves": p["generation"]["injected_moves"],
                "deterministic": p["deterministic"],
                "checkpoints": cps,
            }
        )
    return rows


# ---- the power analysis of recall at 2 ---------------------------------------------------------


def hits_at(rows: list[dict], g: int) -> dict[str, list[int]]:
    out: dict[str, list[int]] = {f: [] for f, _, _ in FLAWS}
    for r in rows:
        if r["flaw"] is None:
            continue
        c = next(c for c in r["checkpoints"] if c["games"] == g)
        out[r["flaw"]].append(int(bool(c["hit2"])))
    return out


def simulate_recall(
    rng: np.random.Generator,
    hits: dict[str, list[int]],
    m: int,
    delta: float,
    churn: float,
    sims: int,
    tolerance: float | None = None,
) -> dict:
    """The decision rule on `sims` simulated suites of `m` players per flaw.

    With `tolerance`, also the rule CI runs (`bench::results::regressions`): a regression when
    the candidate's recall is below the reference's by more than `tolerance`.

    Each simulation draws each flaw's hit probability from Beta(h + 1/2, n - h + 1/2) (the
    pilot's h hits of n players), the reference outcome of every player from it, and the
    candidate's by flipping: a reference hit is lost with probability `churn` (delta >= 0) or
    the rate that gives the mean change (delta < 0), a miss is gained with the rate that gives
    the mean change (delta >= 0) or `churn` (delta < 0). A simulation whose required rate is
    above 1 cannot hold the change and counts as not shipped and not flagged."""
    k = len(FLAWS)
    h = np.array([sum(hits[f]) for f, _, _ in FLAWS], dtype=float)
    n = np.array([len(hits[f]) for f, _, _ in FLAWS], dtype=float)
    pi = rng.beta(h + 0.5, n - h + 0.5, size=(sims, k))
    pibar = pi.mean(axis=1)
    with np.errstate(divide="ignore", invalid="ignore"):
        if delta >= 0:
            gain = (delta + churn * pibar) / (1.0 - pibar)
            lose = np.full(sims, churn)
        else:
            gain = np.full(sims, churn)
            lose = (-delta + churn * (1.0 - pibar)) / pibar
    feasible = np.isfinite(gain) & np.isfinite(lose) & (gain <= 1.0) & (lose <= 1.0)
    gain = np.clip(np.nan_to_num(gain), 0, 1)
    lose = np.clip(np.nan_to_num(lose), 0, 1)
    ref = rng.random((sims, k, m)) < pi[:, :, None]
    u = rng.random((sims, k, m))
    cand = np.where(ref, u >= lose[:, None, None], u < gain[:, None, None])
    ref = ref.reshape(sims, -1)
    cand = cand.reshape(sims, -1)
    n11 = (ref & cand).sum(1)
    n10 = (~ref & cand).sum(1)
    n01 = (ref & ~cand).sum(1)
    n00 = (~ref & ~cand).sum(1)
    _, lo_s, _ = newcombe_paired(n11, n10, n01, n00, z_of(SHIP_LEVEL))
    _, _, hi_g = newcombe_paired(n11, n10, n01, n00, z_of(GUARD_LEVEL))
    ship = (lo_s > 0) & feasible
    flagged = (hi_g < 0) & feasible
    drop = (n01 - n10) / (k * m)
    ci_flagged = (drop > tolerance + 1e-9) & feasible if tolerance is not None else flagged
    return {
        "ship": float(ship.mean()),
        "flagged": float(flagged.mean()),
        "ci_flagged": float(ci_flagged.mean()),
        "infeasible": float((~feasible).mean()),
        "mean_difference": float(((n10 - n01) / (k * m)).mean()),
    }


def mc_ci(p: float, sims: int) -> list[float]:
    lo, hi = wilson(np.array(p * sims), np.array(float(sims)), 1.959964)
    return [r6(float(lo)), r6(float(hi))]


def required_m(curve: list[dict], key: str = "ship") -> int | None:
    """The smallest grid m from which the power stays at least the target for every larger m
    (a single noisy crossing does not count)."""
    for i, row in enumerate(curve):
        if all(r[key] >= TARGET_POWER for r in curve[i:]):
            return row["players_per_flaw"]
    return None


# ---- the coverage secondary ------------------------------------------------------------------


def coverage_vectors(rows: list[dict], g: int) -> dict[str, list[np.ndarray]]:
    out: dict[str, list[np.ndarray]] = {f: [] for f, _, _ in FLAWS}
    for r in rows:
        if r["flaw"] is None:
            continue
        v = [c["covered"] for c in r["checkpoints"] if c["games"] <= g and c["covered"] is not None]
        if v:
            out[r["flaw"]].append(np.array(v, dtype=bool))
    return out


def simulate_coverage(
    rng: np.random.Generator, vecs: dict[str, list[np.ndarray]], m: int, sims: int, churn: float
) -> dict:
    """Resamples m players per flaw from the pilot's (the player is the unit). Returns the 95%
    half-width of one version's coverage, and the power of the paired rule (97.5% t interval of
    the per-player differences above 0) for a change of DELTA_COVERAGE, with the candidate's
    flips made interval by interval and, as the clustered extreme, player by player."""
    est = np.empty(sims)
    ship_i = np.zeros(sims, dtype=bool)
    ship_p = np.zeros(sims, dtype=bool)
    tq = None
    for s in range(sims):
        players = []
        for f, _, _ in FLAWS:
            pool = vecs[f]
            if not pool:
                continue
            idx = rng.integers(0, len(pool), size=m)
            players.extend(pool[i] for i in idx)
        y = np.array([v.mean() for v in players])
        est[s] = y.mean()
        cbar = np.concatenate(players).mean()
        gain = (DELTA_COVERAGE + churn * cbar) / (1.0 - cbar) if cbar < 1 else np.inf
        if not gain <= 1.0:
            continue
        d_i = np.empty(len(players))
        d_p = np.empty(len(players))
        for j, v in enumerate(players):
            u = rng.random(v.size)
            v2 = np.where(v, u >= churn, u < gain)
            d_i[j] = v2.mean() - v.mean()
            ug, ul = rng.random(2)
            v3 = np.where(v, ul >= churn, ug < gain)
            d_p[j] = v3.mean() - v.mean()
        n = len(players)
        if tq is None:
            tq = float(stats.t.ppf(0.5 + SHIP_LEVEL / 2, n - 1)) if n > 1 else math.inf
        for d, out in ((d_i, ship_i), (d_p, ship_p)):
            sd = d.std(ddof=1) if n > 1 else 0.0
            lo = d.mean() - tq * sd / math.sqrt(n) if sd > 0 else d.mean()
            out[s] = lo > 0
    return {
        "half_width_95": r6(1.959964 * est.std(ddof=1)),
        "half_width_80": r6(1.281552 * est.std(ddof=1)),
        "power_interval_flips": r6(ship_i.mean()),
        "power_player_flips": r6(ship_p.mean()),
    }


def icc_oneway(vecs: dict[str, list[np.ndarray]]) -> float | None:
    """The one-way ANOVA ICC of coverage within players (unequal sizes, Searle's n0)."""
    groups = [v.astype(float) for f in vecs for v in vecs[f]]
    groups = [g for g in groups if g.size > 0]
    k = len(groups)
    if k < 2:
        return None
    sizes = np.array([g.size for g in groups], dtype=float)
    total = np.concatenate(groups)
    grand = total.mean()
    ssb = sum(g.size * (g.mean() - grand) ** 2 for g in groups)
    ssw = sum(((g - g.mean()) ** 2).sum() for g in groups)
    msb = ssb / (k - 1)
    dfw = total.size - k
    if dfw <= 0:
        return None
    msw = ssw / dfw
    n0 = (sizes.sum() - (sizes**2).sum() / sizes.sum()) / (k - 1)
    den = msb + (n0 - 1) * msw
    return None if den <= 0 else float((msb - msw) / den)


# ---- the run ---------------------------------------------------------------------------------


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", required=True)
    ap.add_argument(
        "--algorithm",
        default=None,
        help="the algorithm_id the run must be on (default: whatever the cli is); the cli of "
        "$REMATCH_CLI is the version that runs, and a different one stops the run",
    )
    ap.add_argument(
        "--seed-namespace",
        default=EXP,
        help="the namespace of every named seed (default: the experiment id, the registered "
        "seeds); another namespace draws a fresh replication",
    )
    args = ap.parse_args()
    out_dir = Path(args.out)
    global SEED_NS
    SEED_NS = args.seed_namespace
    (out_dir / "figures").mkdir(parents=True, exist_ok=True)

    algorithm_id = lab.algorithm()["id"]
    if args.algorithm is not None and algorithm_id != args.algorithm:
        raise SystemExit(f"the cli is {algorithm_id}, not the requested {args.algorithm}")
    suite = pilot_suite()
    with tempfile.TemporaryDirectory() as tmp:
        suite_path = Path(tmp) / "suite.json"
        suite_path.write_text(json.dumps(suite), encoding="utf-8")
        run = lab.bench(suite_file=suite_path, out_dir=Path(tmp) / "bench", engines=ENGINES)
    print(
        f"{EXP}: pilot ran in {run['seconds']:.0f}s, {run['engine_searches']} engine searches, "
        f"{run['memo_hits']} served from memory",
        file=sys.stderr,
    )
    rows = player_rows(run)
    flawed = [r for r in rows if r["flaw"] is not None]
    clean = [r for r in rows if r["flaw"] is None]
    pilot_games = sum(r["games"] for r in rows)

    # Observed (descriptive): recall at 2 per checkpoint with Wilson 95%, per flaw; coverage.
    observed = {"recall_at_2": [], "coverage": [], "clean_rank1": []}
    boot_rng = np.random.default_rng(seed(SEED_NS, "bootstrap/observed"))
    for g in CHECKPOINTS:
        hits = hits_at(rows, g)
        all_hits = [x for f in hits for x in hits[f]]
        lo, hi = wilson(np.array(sum(all_hits)), np.array(len(all_hits)), 1.959964)
        observed["recall_at_2"].append(
            {
                "games": g,
                "hits": int(sum(all_hits)),
                "players": len(all_hits),
                "recall": r6(sum(all_hits) / len(all_hits)),
                "ci95": [r6(float(lo)), r6(float(hi))],
                "per_flaw": {f: [int(sum(hits[f])), len(hits[f])] for f in hits},
            }
        )
        vecs = coverage_vectors(rows, g)
        ys = {f: np.array([v.mean() for v in vecs[f]]) for f in vecs}
        point = float(np.concatenate([ys[f] for f in ys if ys[f].size]).mean())
        boots = np.empty(2000)
        for b in range(2000):
            parts = [
                ys[f][boot_rng.integers(0, ys[f].size, ys[f].size)] for f in ys if ys[f].size
            ]
            boots[b] = np.concatenate(parts).mean()
        observed["coverage"].append(
            {
                "games_up_to": g,
                "intervals": int(sum(v.size for f in vecs for v in vecs[f])),
                "coverage": r6(point),
                "ci95_cluster_bootstrap": [
                    r6(float(np.quantile(boots, 0.025))),
                    r6(float(np.quantile(boots, 0.975))),
                ],
                "per_flaw": {f: r6(float(ys[f].mean())) if ys[f].size else None for f in ys},
                "icc_within_player": r6(icc_oneway(vecs)),
            }
        )
        observed["clean_rank1"].append(
            {
                "games": g,
                "with_rank1": sum(
                    1
                    for r in clean
                    for c in r["checkpoints"]
                    if c["games"] == g and c["rank1"] is not None
                ),
                "clean_players": len(clean),
            }
        )

    # Primary: power of the ship rule for +0.1 in recall at 2, per G, churn and players per flaw.
    recall_ss = np.random.SeedSequence(seed(SEED_NS, "power/recall"))
    cells = [(g, b, m) for g in CHECKPOINTS for b in CHURN for m in M_GRID]
    streams = recall_ss.spawn(len(cells))
    power_recall = []
    for (g, b, m), ss in zip(cells, streams, strict=True):
        res = simulate_recall(np.random.default_rng(ss), hits_at(rows, g), m, DELTA, b, SIMS_RECALL)
        power_recall.append(
            {
                "games": g,
                "churn": b,
                "players_per_flaw": m,
                "flawed_players": m * len(FLAWS),
                "ship": r6(res["ship"]),
                "ship_mc_ci95": mc_ci(res["ship"], SIMS_RECALL),
                "infeasible": r6(res["infeasible"]),
            }
        )
    required = {
        f"{g}/{b}": required_m(
            [r for r in power_recall if r["games"] == g and r["churn"] == b]
        )
        for g in CHECKPOINTS
        for b in CHURN
    }

    # The budgets and the suites they allow.
    smoke_m = max(
        (
            m
            for m in range(1, 1000)
            if (len(FLAWS) * m + SMOKE_CLEAN) * SMOKE_GAMES <= SMOKE_BUDGET_GAMES
        ),
        default=None,
    )
    full_budget_m = {
        g: max(
            (
                m
                for m in range(1, 1000)
                if (len(FLAWS) * m + FULL_SWEEP_PLAYERS + FULL_CLEAN) * g <= FULL_BUDGET_GAMES
            ),
            default=None,
        )
        for g in FULL_GAMES_OPTIONS
    }

    # Power, MDE and the guard at the budget sizes (exact m, not the grid).
    smoke_ss = np.random.SeedSequence(seed(SEED_NS, "power/smoke"))
    budget_cells = [("smoke", SMOKE_GAMES, smoke_m)] + [
        ("full", g, full_budget_m[g]) for g in FULL_GAMES_OPTIONS
    ]
    budget_streams = iter(smoke_ss.spawn(len(budget_cells) * (len(DELTA_GRID) * 2 + 2)))
    at_budget = []
    for name, g, m in budget_cells:
        hits = hits_at(rows, g)
        row = {"suite": name, "games": g, "players_per_flaw": m, "flawed_players": m * len(FLAWS)}
        n = m * len(FLAWS)
        # The tolerance: the smallest number k of lost detections (none gained) whose 80% paired
        # interval lies wholly below 0, from a reference at the pilot's recall at G (H hits of
        # n, so n11 = H - k); a drop of k / n is flagged by CI, k - 1 is not.
        pilot_recall = sum(sum(v) for v in hits.values()) / sum(len(v) for v in hits.values())
        big_h = int(round(pilot_recall * n))
        kstar = None
        for k in range(1, big_h + 1):
            _, _, hi = newcombe_paired(
                np.array(float(big_h - k)),
                np.array(0.0),
                np.array(float(k)),
                np.array(float(n - big_h)),
                z_of(GUARD_LEVEL),
            )
            if float(hi) < 0:
                kstar = k
                break
        tol = None if kstar is None else round((kstar - 0.5) / n, 3)
        row["reference_hits"] = big_h
        row["guard_k_lost"] = kstar
        row["recall_tolerance"] = r6(tol)
        ship = simulate_recall(
            np.random.default_rng(next(budget_streams)), hits, m, DELTA, PRIMARY_CHURN, SIMS_RECALL
        )
        row["ship_power_delta_0_1"] = r6(ship["ship"])
        row["ship_power_mc_ci95"] = mc_ci(ship["ship"], SIMS_RECALL)
        null = simulate_recall(
            np.random.default_rng(next(budget_streams)), hits, m, 0.0, PRIMARY_CHURN, SIMS_RECALL,
            tol,
        )
        row["guard_false_alarm_delta_0"] = r6(null["flagged"])
        row["ci_rule_false_alarm_delta_0"] = r6(null["ci_flagged"])
        mde_ship = None
        mde_guard = None
        mde_ci = None
        curve = []
        for d in DELTA_GRID:
            s = simulate_recall(
                np.random.default_rng(next(budget_streams)), hits, m, d, PRIMARY_CHURN, SIMS_RECALL
            )
            r = simulate_recall(
                np.random.default_rng(next(budget_streams)),
                hits,
                m,
                -d,
                PRIMARY_CHURN,
                SIMS_RECALL,
                tol,
            )
            curve.append(
                {
                    "delta": d,
                    "ship": r6(s["ship"]),
                    "guard_flags_drop": r6(r["flagged"]),
                    "ci_rule_flags_drop": r6(r["ci_flagged"]),
                }
            )
            if mde_ship is None and s["ship"] >= TARGET_POWER:
                mde_ship = d
            if mde_guard is None and r["flagged"] >= TARGET_POWER:
                mde_guard = d
            if mde_ci is None and r["ci_flagged"] >= TARGET_POWER:
                mde_ci = d
        row["curve"] = curve
        row["mde_ship"] = mde_ship
        row["mde_guard_drop"] = mde_guard
        row["mde_ci_rule_drop"] = mde_ci
        row["games_total"] = (
            (n + SMOKE_CLEAN) * g if name == "smoke" else (n + FULL_SWEEP_PLAYERS + FULL_CLEAN) * g
        )
        at_budget.append(row)

    # Secondary: coverage precision and power at the grid and budget sizes.
    cov_ss = np.random.SeedSequence(seed(SEED_NS, "power/coverage"))
    cov_m = [1, 2, 3, 4, 6, 8, 13, 20, 32, 64]
    cov_cells = [(g, m) for g in (80, 120) for m in cov_m]
    cov_streams = cov_ss.spawn(len(cov_cells) + 1)
    coverage_power = []
    for (g, m), ss in zip(cov_cells, cov_streams[:-1], strict=True):
        res = simulate_coverage(
            np.random.default_rng(ss), coverage_vectors(rows, g), m, SIMS_COVERAGE, PRIMARY_CHURN
        )
        coverage_power.append({"games_up_to": g, "players_per_flaw": m, **res})
    # The coverage tolerance at the smoke size, on the statistic CI checks: the growth of the
    # distance from 0.8, |c' - 0.8| - |c - 0.8|, when the true change is 0 (churn only), with
    # the player-by-player flips (the clustered, wider model); its 80th percentile, so churn
    # alone exceeds it 20% of the time.
    null_rng = np.random.default_rng(cov_streams[-1])
    vecs80 = coverage_vectors(rows, SMOKE_GAMES)
    diffs = np.empty(SIMS_COVERAGE)
    for s in range(SIMS_COVERAGE):
        players = []
        for f, _, _ in FLAWS:
            pool = vecs80[f]
            if pool:
                players.extend(pool[i] for i in null_rng.integers(0, len(pool), size=smoke_m))
        allv = np.concatenate(players)
        cbar = allv.mean()
        gain = PRIMARY_CHURN * cbar / (1 - cbar) if cbar < 1 else 0.0
        flipped = []
        for v in players:
            ug, ul = null_rng.random(2)
            flipped.append(np.where(v, ul >= PRIMARY_CHURN, ug < gain))
        c2 = np.concatenate(flipped).mean()
        diffs[s] = abs(c2 - 0.8) - abs(cbar - 0.8)
    coverage_tolerance = r6(round(float(np.quantile(diffs, 0.8)), 3))

    # The decision (section 9).
    primary_required = {g: required[f"{g}/{PRIMARY_CHURN}"] for g in FULL_GAMES_OPTIONS}
    reachable = [
        (g, primary_required[g])
        for g in FULL_GAMES_OPTIONS
        if primary_required[g] is not None and primary_required[g] <= full_budget_m[g]
    ]
    if reachable:
        g, m = min(
            reachable,
            key=lambda t: ((len(FLAWS) * t[1] + FULL_SWEEP_PLAYERS + FULL_CLEAN) * t[0], -t[0]),
        )
        decision = {"outcome": "resize: the full suite reaches 80% power within its budget"}
        full_choice = {"games": g, "players_per_flaw": m}
    else:
        best = None
        for row in at_budget:
            if row["suite"] != "full":
                continue
            if (
                best is None
                or row["ship_power_delta_0_1"] > best["ship_power_delta_0_1"] + 0.01
                or (
                    abs(row["ship_power_delta_0_1"] - best["ship_power_delta_0_1"]) <= 0.01
                    and row["games"] > best["games"]
                )
            ):
                best = row
        decision = {
            "outcome": "budget-bound: no full suite within the budget reaches 80% power for 0.1; "
            "the full suite is sized to the budget and its MDE registered"
        }
        full_choice = {"games": best["games"], "players_per_flaw": best["players_per_flaw"]}
    smoke_row = at_budget[0]
    decision.update(
        {
            "full_suite": full_choice,
            "smoke_suite": {"games": SMOKE_GAMES, "players_per_flaw": smoke_m},
            "recall_tolerance_smoke": smoke_row["recall_tolerance"],
            "coverage_tolerance_smoke": coverage_tolerance,
        }
    )

    results = {
        "experiment": EXP,
        "algorithm_id": algorithm_id,
        "pilot": {
            "suite": suite,
            "games_total": pilot_games,
            "players": rows,
            "scorecard_metrics": run["scorecard"]["metrics"],
        },
        "observed": observed,
        "power_recall": power_recall,
        "required_players_per_flaw": required,
        "budgets": {
            "smoke_games": SMOKE_BUDGET_GAMES,
            "full_games": FULL_BUDGET_GAMES,
            "smoke_players_per_flaw": smoke_m,
            "full_players_per_flaw": {str(g): full_budget_m[g] for g in FULL_GAMES_OPTIONS},
        },
        "at_budget": at_budget,
        "coverage_power": coverage_power,
        "decision": decision,
        "settings": {
            "delta": DELTA,
            "delta_coverage": DELTA_COVERAGE,
            "churn": CHURN,
            "primary_churn": PRIMARY_CHURN,
            "sims_recall": SIMS_RECALL,
            "sims_coverage": SIMS_COVERAGE,
            "ship_level": SHIP_LEVEL,
            "guard_level": GUARD_LEVEL,
            "seed_names": SEED_NAMES,
            "seed_namespace": SEED_NS,
        },
    }
    write_results(out_dir, results)
    figures(out_dir / "figures", results)
    return 0


def figures(fig_dir: Path, res: dict) -> None:
    import matplotlib

    matplotlib.use("Agg")
    import matplotlib.pyplot as plt

    fig, axes = plt.subplots(1, len(CHURN), figsize=(12, 4), sharey=True)
    for ax, b in zip(axes, CHURN, strict=True):
        for g in CHECKPOINTS:
            pts = [r for r in res["power_recall"] if r["games"] == g and r["churn"] == b]
            ax.plot([r["flawed_players"] for r in pts], [r["ship"] for r in pts], label=f"{g} games")
        ax.axhline(TARGET_POWER, color="grey", lw=0.8, ls="--")
        ax.set_xscale("log")
        ax.set_title(f"churn {b}")
        ax.set_xlabel("flawed players (5 flaws)")
    axes[0].set_ylabel("P(ship) for +0.1 recall at 2")
    axes[-1].legend(fontsize=8)
    fig.tight_layout()
    fig.savefig(fig_dir / "power-recall.png", dpi=120, metadata={"Software": None})
    plt.close(fig)

    fig, ax = plt.subplots(figsize=(6, 4))
    for g in (80, 120):
        pts = [r for r in res["coverage_power"] if r["games_up_to"] == g]
        ax.plot(
            [r["players_per_flaw"] * len(FLAWS) for r in pts],
            [r["half_width_95"] for r in pts],
            label=f"checkpoints up to {g} games",
        )
    ax.set_xscale("log")
    ax.set_xlabel("flawed players")
    ax.set_ylabel("95% half-width of coverage")
    ax.legend(fontsize=8)
    fig.tight_layout()
    fig.savefig(fig_dir / "coverage-precision.png", dpi=120, metadata={"Software": None})
    plt.close(fig)


if __name__ == "__main__":
    raise SystemExit(main())

"""EXP-0002: do the 80% ranges of the weakness rates hold the true rate 80% of the time?
(preregistration.md, issue #13)

Synthetic players whose true per-chance rate of each of five weaknesses we choose, at a multiple
of the peers' rate, in three rating bands, with and without game-to-game overdispersion. Their
series points go through the production `compute_estimates` (`cli lab profile`, through
`rematch_lab.profile`) at 30, 60, 120 and 250 games, and every 80% interval is scored against
the true rate. The player is the unit: every interval of a player is resampled together.

    python analysis.py --out DIR    # DIR/results.json and DIR/figures/
"""

from __future__ import annotations

import argparse
import math
from datetime import UTC, datetime, timedelta
from pathlib import Path

import numpy as np
from scipy import stats

import rematch_lab as lab
from rematch_lab.experiments import seed, write_results
from rematch_lab.models import BandReferenceStat, LabPoint, PlayerFacts, ProfileRequest

EXP = "EXP-0002"
# The seed namespace (--seed-namespace); the registered run uses the experiment id.
SEED_NS = EXP

# ---- the design (section 7) --------------------------------------------------------------------
# key: chances per game, the B2 peers' rate, whether its prior is a peer row (else the evaluation
# baseline of ADR 0101, a flat prior), and the mean chances in a game that has any (for the
# overdispersion).
KEYS = {
    "W1": {"chances": ("poisson", 53.0), "mu_b2": 0.004, "peer": True, "per_game": 53.0},
    "W4.missed": {"chances": ("poisson", 1.0), "mu_b2": 0.30, "peer": True, "per_game": None},
    "W7.traps": {"chances": ("one",), "mu_b2": 0.10, "peer": True, "per_game": None},
    "W9b": {"chances": ("zip", 1 / 3, 31.8), "mu_b2": 0.02, "peer": True, "per_game": 31.8},
    "W5": {"chances": ("bernoulli", 0.4), "mu_b2": 0.15, "peer": False, "per_game": None},
}
# Odds multiplier of the peers' rate per band, relative to B2 (section 7).
BANDS = {"B1": 1.25, "B2": 1.0, "B3": 0.8}
RHOS = [0.5, 1.0, 2.0, 4.0, 8.0]
PRIMARY_RHOS = (2.0, 4.0, 8.0)
# The schedule: two games a day (the product's recency weights apply), or every game inside the
# last day (weights of about 1), the contrast that separates the weighting from the prior.
RECENCIES = ["spread", "compressed"]
# The #13 case (docs/benchmark.md): W1 against opponents at 0.0008 with a true rate of 0.023.
ISSUE13 = {"key": "W1", "peer": 0.0008, "theta": 0.023, "band": "B2"}
DISPERSIONS = ["none", "de1.5"]
DESIGN_EFFECT = 1.5
CHECKPOINTS = [30, 60, 120, 250]
PLAYERS_PER_CELL = 200
GAMES = max(CHECKPOINTS)
ANCHOR = datetime(2026, 11, 1, 12, 0, tzinfo=UTC)
SPACING = timedelta(hours=12)
CHUNK = 200
BOOTSTRAP = 2000
BAND_TARGET = (0.75, 0.85)
NOMINAL = 0.80
PIT_LEVELS = [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 0.95]

CELLS = [(b, r, d) for b in BANDS for r in RHOS for d in DISPERSIONS]
SEED_NAMES = [f"players/{b}/{r}/{d}" for b, r, d in CELLS] + [
    *[f"issue13/{d}" for d in DISPERSIONS],
    "bootstrap/primary",
    "bootstrap/margins",
]


def r6(x: float | None) -> float | None:
    return None if x is None or not math.isfinite(x) else round(float(x), 6)


def odds_scale(p: float, factor: float) -> float:
    o = p / (1.0 - p) * factor
    return o / (1.0 + o)


def peer_rate(key: str, band: str) -> float:
    k = KEYS[key]
    return odds_scale(k["mu_b2"], BANDS[band]) if k["peer"] else k["mu_b2"]


def true_rate(key: str, band: str, rho: float) -> float:
    return odds_scale(peer_rate(key, band), rho)


def stat(key: str, rate: float, *, peer: bool) -> BandReferenceStat:
    metric = f"weakness:{key}"
    if not peer:
        return BandReferenceStat(
            metric=metric, median=rate, p25=rate, p40=rate, p75=rate, pooled_rate=rate,
            epl_per_event=1.0, opps_per_game=None, opp_slope_per_100=None, n_players=0,
            n_games=0, low_sample=False,
        )
    return BandReferenceStat(
        metric=metric, median=rate, p25=rate * 0.6, p40=rate * 0.85, p75=rate * 1.5,
        pooled_rate=rate, epl_per_event=0.3, opps_per_game=None, opp_slope_per_100=None,
        n_players=40, n_games=1200, low_sample=False,
    )


def chances(rng: np.random.Generator, spec: tuple, n: int) -> np.ndarray:
    kind = spec[0]
    if kind == "poisson":
        return rng.poisson(spec[1], n)
    if kind == "one":
        return np.ones(n, dtype=np.int64)
    if kind == "zip":
        reach = rng.random(n) < spec[1]
        return np.where(reach, rng.poisson(spec[2], n), 0)
    if kind == "bernoulli":
        return (rng.random(n) < spec[1]).astype(np.int64)
    raise ValueError(kind)


def events(
    rng: np.random.Generator, c: np.ndarray, theta: float, per_game: float | None, disp: str
) -> np.ndarray:
    """Binomial events per game; with `de1.5`, a game-level rate from Beta(theta k, (1 - theta) k)
    whose intraclass correlation 1 / (k + 1) gives a design effect of 1.5 at `per_game`
    chances, for the keys with many chances in a game (W1, W9b)."""
    if disp == "de1.5" and per_game is not None:
        kappa = (per_game - 1.0) / (DESIGN_EFFECT - 1.0) - 1.0
        tg = rng.beta(theta * kappa, (1.0 - theta) * kappa, c.size)
        return rng.binomial(c, tg)
    return rng.binomial(c, theta)


def simulate_cell(band: str, rho: float, disp: str) -> dict[str, np.ndarray]:
    """Every player of a cell: per key, (players, games) chances and events, in key order. The
    #13 cell (rho "issue13") has W1 only, at its own rates."""
    if rho == "issue13":
        rng = np.random.default_rng(seed(SEED_NS, f"issue13/{disp}"))
        k = KEYS["W1"]
        c = chances(rng, k["chances"], PLAYERS_PER_CELL * GAMES).reshape(PLAYERS_PER_CELL, GAMES)
        e = events(rng, c.ravel(), ISSUE13["theta"], k["per_game"], disp).reshape(c.shape)
        return {"W1": np.stack([c, e])}
    rng = np.random.default_rng(seed(SEED_NS, f"players/{band}/{rho}/{disp}"))
    out = {}
    for key, k in KEYS.items():
        theta = true_rate(key, band, rho)
        c = chances(rng, k["chances"], PLAYERS_PER_CELL * GAMES).reshape(PLAYERS_PER_CELL, GAMES)
        e = events(rng, c.ravel(), theta, k["per_game"], disp).reshape(c.shape)
        out[key] = np.stack([c, e])
    return out


PLAYED = {
    "spread": [ANCHOR - SPACING * GAMES + SPACING * i for i in range(GAMES)],
    # Every game in the 24 hours before the anchor: ages under a day, weights above 0.988.
    "compressed": [ANCHOR - timedelta(hours=24) + timedelta(hours=24) * i / GAMES for i in range(GAMES)],
}


def theta_of(key: str, band: str, rho) -> float:
    return ISSUE13["theta"] if rho == "issue13" else true_rate(key, band, rho)


def facts(
    band: str, pid: str, data: dict[str, np.ndarray], i: int, g: int, recency: str, rho
) -> PlayerFacts:
    played = PLAYED[recency]
    now = played[g - 1] + timedelta(hours=1)
    series = {
        key: [
            LabPoint(played_at=played[t], events=int(arr[1, i, t]), opportunities=int(arr[0, i, t]))
            for t in range(g)
        ]
        for key, arr in data.items()
    }
    reference = {
        f"weakness:{key}": stat(
            key, ISSUE13["peer"] if rho == "issue13" else peer_rate(key, band), peer=True
        )
        for key, k in KEYS.items()
        if k["peer"]
    }
    model = {key: stat(key, k["mu_b2"], peer=False) for key, k in KEYS.items() if not k["peer"]}
    return PlayerFacts(
        id=pid, now=now, band=band, games_in_window=g, series=series, reference=reference,
        model_reference=model,
    )


def run_profiles() -> tuple[str, list[dict]]:
    """Scores every interval: one row per (cell, player, key, checkpoint)."""
    rows: list[dict] = []
    algorithm_id = None
    plan = [(b, r, d, rec) for b, r, d in CELLS for rec in RECENCIES] + [
        (ISSUE13["band"], "issue13", d, "spread") for d in DISPERSIONS
    ]
    for band, rho, disp, recency in plan:
        # The same draws for both schedules (paired): only the dates differ.
        data = simulate_cell(band, rho, disp)
        pending: list[tuple[PlayerFacts, int, int]] = []

        def flush() -> None:
            nonlocal algorithm_id
            if not pending:
                return
            resp = lab.profile(ProfileRequest(players=[f for f, _, _ in pending]))
            if algorithm_id is None:
                algorithm_id = resp.algorithm_id
            elif resp.algorithm_id != algorithm_id:
                raise SystemExit("the algorithm_id changed during the run")
            for (f, i, g), p in zip(pending, resp.players, strict=True):
                if p.id != f.id:
                    raise SystemExit("profile responses out of order")
                by_key = {e.key: e for e in p.estimates}
                for key in data:
                    e = by_key[key]
                    theta = theta_of(key, band, rho)
                    post = e.posterior
                    rows.append(
                        {
                            "band": band, "rho": rho, "disp": disp, "recency": recency,
                            "player": i, "key": key, "credible": e.credible,
                            "games": g, "theta": theta, "low": post.interval.low,
                            "high": post.interval.high, "mean": post.mean,
                            "covered": post.interval.low <= theta <= post.interval.high,
                            "pit": float(stats.beta.cdf(theta, post.alpha, post.beta)),
                            "events": e.k_unweighted, "chances": e.n_unweighted,
                            "reason": e.reason,
                        }
                    )
            pending.clear()

        for i in range(PLAYERS_PER_CELL):
            for g in CHECKPOINTS:
                pid = f"{band}-{rho}-{disp}-{recency}-{i:03d}-{g}"
                pending.append((facts(band, pid, data, i, g, recency, rho), i, g))
                if len(pending) >= CHUNK:
                    flush()
        flush()
    assert algorithm_id is not None
    return algorithm_id, rows


# ---- the analysis (section 8) ------------------------------------------------------------------


def player_means(rows: list[dict], pred) -> dict[tuple, np.ndarray]:
    """Per cell, each player's share of covered intervals among the rows `pred` keeps."""
    acc: dict[tuple, dict[int, list[bool]]] = {}
    for r in rows:
        if pred(r):
            cell = (r["band"], r["rho"], r["disp"], r["recency"])
            acc.setdefault(cell, {}).setdefault(r["player"], []).append(r["covered"])
    return {
        cell: np.array([np.mean(v) for _, v in sorted(players.items())])
        for cell, players in sorted(acc.items(), key=lambda kv: str(kv[0]))
    }


def stratified_boot(rng: np.random.Generator, by_cell: dict[tuple, np.ndarray]) -> np.ndarray:
    """B resamples of the equal-weighted mean over cells, players resampled within each cell."""
    cells = list(by_cell.values())
    out = np.empty(BOOTSTRAP)
    for b in range(BOOTSTRAP):
        out[b] = np.mean([y[rng.integers(0, y.size, y.size)].mean() for y in cells])
    return out


def estimate(by_cell: dict[tuple, np.ndarray]) -> float:
    return float(np.mean([y.mean() for y in by_cell.values()]))


def wilson95(x: int, n: int) -> list:
    if n == 0:
        return [None, None]
    z = 1.959964
    p = x / n
    den = 1 + z * z / n
    c = (p + z * z / (2 * n)) / den
    h = z * math.sqrt(p * (1 - p) / n + z * z / (4 * n * n)) / den
    return [r6(c - h), r6(c + h)]


def classify(lo: float, hi: float) -> str:
    a, b = BAND_TARGET
    if a <= lo and hi <= b:
        return "within the band"
    if hi < a or lo > b:
        return "outside the band"
    return "need more data"


def holm(pvals: list[float]) -> list[float]:
    order = sorted(range(len(pvals)), key=lambda i: pvals[i])
    adj = [0.0] * len(pvals)
    running = 0.0
    for rank, i in enumerate(order):
        running = max(running, min(1.0, (len(pvals) - rank) * pvals[i]))
        adj[i] = running
    return adj


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

    if args.algorithm is not None and lab.algorithm()["id"] != args.algorithm:
        raise SystemExit(f"the cli is not the requested {args.algorithm}")
    algorithm_id, rows = run_profiles()

    spread = [r for r in rows if r["recency"] == "spread" and r["rho"] != "issue13"]
    in_primary = [r for r in spread if r["rho"] in PRIMARY_RHOS]

    # Primary: the 20 key x checkpoint coverages at rho in {2, 4, 8} (two games a day), each
    # pooled over band and dispersion with cells weighted equally; Bonferroni intervals for 20.
    prng = np.random.default_rng(seed(SEED_NS, "bootstrap/primary"))
    z_prim = float(stats.norm.ppf(1 - 0.05 / (2 * len(KEYS) * len(CHECKPOINTS))))
    primary_cells = []
    for key in KEYS:
        for g in CHECKPOINTS:
            by = player_means(in_primary, lambda r, k=key, gg=g: r["key"] == k and r["games"] == gg)
            est = estimate(by)
            bs = stratified_boot(prng, by)
            se = float(bs.std(ddof=1))
            lo, hi = est - z_prim * se, est + z_prim * se
            primary_cells.append(
                {
                    "key": key,
                    "games": g,
                    "coverage": r6(est),
                    "ci95": [r6(float(np.quantile(bs, 0.025))), r6(float(np.quantile(bs, 0.975)))],
                    "ci_bonferroni": [r6(lo), r6(hi)],
                    "classification": classify(lo, hi),
                }
            )
    classes = [c["classification"] for c in primary_cells]
    if all(c == "within the band" for c in classes):
        primary_decision = "within the band"
    elif any(c == "outside the band" for c in classes):
        primary_decision = "outside the band"
    else:
        primary_decision = "need more data"
    worst = max(primary_cells, key=lambda c: abs(c["coverage"] - NOMINAL))
    pooled = player_means(in_primary, lambda r: True)
    pooled_bs = stratified_boot(prng, pooled)
    primary = {
        "estimand": "each key x checkpoint coverage of the 80% interval, rho in {2, 4, 8}, "
        "two games a day, pooled over band and dispersion",
        "z_bonferroni": r6(z_prim),
        "cells": primary_cells,
        "outside": sum(c == "outside the band" for c in classes),
        "within": sum(c == "within the band" for c in classes),
        "worst": worst,
        "pooled_coverage": r6(estimate(pooled)),
        "pooled_ci95": [
            r6(float(np.quantile(pooled_bs, 0.025))),
            r6(float(np.quantile(pooled_bs, 0.975))),
        ],
        "players": int(sum(y.size for y in pooled.values())),
        "intervals": len(in_primary),
        "decision": primary_decision,
    }

    # Confirmatory margins (Holm over the 21): key, band (the peers' rate), checkpoint and
    # dispersion at the primary rhos and two games a day; rho over every rho at two games a day;
    # the schedule at the primary rhos.
    margins_def = (
        [("key", k, in_primary) for k in KEYS]
        + [("band", b, in_primary) for b in BANDS]
        + [("games", g, in_primary) for g in CHECKPOINTS]
        + [("disp", d, in_primary) for d in DISPERSIONS]
        + [("rho", r, spread) for r in RHOS]
        + [("recency", rec, [r for r in rows if r["rho"] in PRIMARY_RHOS]) for rec in RECENCIES]
    )
    mrng = np.random.default_rng(seed(SEED_NS, "bootstrap/margins"))
    z_bonf = float(stats.norm.ppf(1 - 0.05 / (2 * len(margins_def))))
    margins = []
    for factor, level, subset in margins_def:
        by = player_means(subset, lambda r, f=factor, v=level: r[f] == v)
        est = estimate(by)
        bs = stratified_boot(mrng, by)
        se = float(bs.std(ddof=1))
        zstat = (est - NOMINAL) / se if se > 0 else math.inf
        p = float(2 * stats.norm.sf(abs(zstat))) if math.isfinite(zstat) else 0.0
        lo, hi = est - z_bonf * se, est + z_bonf * se
        margins.append(
            {
                "factor": factor,
                "level": level,
                "coverage": r6(est),
                "ci95": [r6(float(np.quantile(bs, 0.025))), r6(float(np.quantile(bs, 0.975)))],
                "ci_bonferroni": [r6(lo), r6(hi)],
                "p_vs_0_80": r6(p),
                "classification": classify(lo, hi),
            }
        )
    for m, adj in zip(margins, holm([m["p_vs_0_80"] for m in margins]), strict=True):
        m["p_holm"] = r6(adj)

    # Secondary: coverage among the estimates Rematch shows (credible), per key, every rho, two
    # games a day; and the #13 cell. Wilson 95% over intervals, descriptive: a player can give
    # up to four checkpoints, so these intervals are too narrow.
    credible = []
    for key in KEYS:
        rs = [r for r in spread if r["key"] == key and r["credible"]]
        x = sum(r["covered"] for r in rs)
        credible.append(
            {
                "key": key,
                "intervals": len(rs),
                "players": len({(r["band"], r["rho"], r["disp"], r["player"]) for r in rs}),
                "coverage": r6(x / len(rs)) if rs else None,
                "ci95_wilson": wilson95(x, len(rs)),
                "below": r6(sum(r["high"] < r["theta"] for r in rs) / len(rs)) if rs else None,
            }
        )
    issue13 = []
    for d in DISPERSIONS:
        for g in CHECKPOINTS:
            rs = [r for r in rows if r["rho"] == "issue13" and r["disp"] == d and r["games"] == g]
            x = sum(r["covered"] for r in rs)
            issue13.append(
                {
                    "disp": d,
                    "games": g,
                    "players": len(rs),
                    "coverage": r6(x / len(rs)),
                    "ci95": wilson95(x, len(rs)),
                    "below": r6(sum(r["high"] < r["theta"] for r in rs) / len(rs)),
                    "mean_ratio": r6(float(np.mean([r["mean"] / r["theta"] for r in rs]))),
                    "credible": r6(sum(r["credible"] for r in rs) / len(rs)),
                }
            )

    # Exploratory: every cell (one interval per player: Wilson 95%), the calibration curve
    # from the posterior's PIT, and the bias of the posterior mean.
    cells = []
    groups: dict[tuple, list[dict]] = {}
    for r in rows:
        if r["rho"] == "issue13":
            continue
        cell = (r["key"], r["band"], r["rho"], r["disp"], r["recency"], r["games"])
        groups.setdefault(cell, []).append(r)
    for (key, band, rho, disp, rec, g), rs in sorted(groups.items(), key=lambda kv: str(kv[0])):
        n = len(rs)
        x = sum(r["covered"] for r in rs)
        cells.append(
            {
                "key": key,
                "band": band,
                "rho": rho,
                "disp": disp,
                "recency": rec,
                "games": g,
                "theta": r6(rs[0]["theta"]),
                "players": n,
                "coverage": r6(x / n),
                "ci95": wilson95(x, n),
                "below": r6(sum(r["high"] < r["theta"] for r in rs) / n),
                "above": r6(sum(r["low"] > r["theta"] for r in rs) / n),
                "credible": r6(sum(r["credible"] for r in rs) / n),
                "mean_ratio": r6(float(np.mean([r["mean"] / r["theta"] for r in rs]))),
                "median_width_ratio": r6(
                    float(np.median([(r["high"] - r["low"]) / r["theta"] for r in rs]))
                ),
                "no_event_share": r6(sum(r["events"] == 0 for r in rs) / n),
            }
        )
    calibration = []
    for rho in RHOS:
        pits = np.array([r["pit"] for r in spread if r["rho"] == rho])
        calibration.append(
            {
                "rho": rho,
                "levels": PIT_LEVELS,
                "empirical": [r6(float(np.mean(np.abs(pits - 0.5) <= q / 2))) for q in PIT_LEVELS],
            }
        )
    pits_all = np.array([r["pit"] for r in in_primary])
    calibration_primary = [r6(float(np.mean(np.abs(pits_all - 0.5) <= q / 2))) for q in PIT_LEVELS]

    results = {
        "experiment": EXP,
        "algorithm_id": algorithm_id,
        "design": {
            "keys": {
                k: {
                    "chances": list(v["chances"]),
                    "peer_rate": {b: r6(peer_rate(k, b)) for b in BANDS},
                    "prior": "peer row" if v["peer"] else "evaluation baseline (flat)",
                }
                for k, v in KEYS.items()
            },
            "bands": BANDS,
            "rhos": RHOS,
            "dispersions": DISPERSIONS,
            "recencies": RECENCIES,
            "checkpoints": CHECKPOINTS,
            "players_per_cell": PLAYERS_PER_CELL,
            "seed_names": SEED_NAMES,
            "seed_namespace": SEED_NS,
        },
        "primary": primary,
        "margins": margins,
        "credible": credible,
        "issue13": {"case": ISSUE13, "rows": issue13},
        "cells": cells,
        "calibration": calibration,
        "calibration_primary": {"levels": PIT_LEVELS, "empirical": calibration_primary},
    }
    write_results(out_dir, results)
    figures(out_dir / "figures", results)
    return 0


def figures(fig_dir: Path, res: dict) -> None:
    import matplotlib

    matplotlib.use("Agg")
    import matplotlib.pyplot as plt

    fig, ax = plt.subplots(figsize=(5, 5))
    ax.plot([0, 1], [0, 1], color="grey", lw=0.8, ls="--")
    for c in res["calibration"]:
        ax.plot(c["levels"], c["empirical"], marker="o", ms=3, label=f"rho {c['rho']}")
    ax.axvline(NOMINAL, color="grey", lw=0.5)
    ax.set_xlabel("nominal level of the central posterior interval")
    ax.set_ylabel("share holding the true rate")
    ax.legend(fontsize=8)
    fig.tight_layout()
    fig.savefig(fig_dir / "calibration-curve.png", dpi=120, metadata={"Software": None})
    plt.close(fig)

    keys = list(KEYS)
    fig, axes = plt.subplots(1, len(keys), figsize=(16, 3.6), sharey=True)
    for ax, key in zip(axes, keys, strict=True):
        for rho in RHOS:
            ys = []
            for g in CHECKPOINTS:
                cs = [
                    c["coverage"]
                    for c in res["cells"]
                    if c["key"] == key
                    and c["rho"] == rho
                    and c["games"] == g
                    and c["recency"] == "spread"
                ]
                ys.append(float(np.mean(cs)))
            ax.plot(CHECKPOINTS, ys, marker="o", ms=3, label=f"rho {rho}")
        ax.axhspan(*BAND_TARGET, color="grey", alpha=0.15)
        ax.set_xscale("log")
        ax.set_title(key)
        ax.set_xlabel("games")
    axes[0].set_ylabel("coverage of the 80% range")
    axes[-1].legend(fontsize=7)
    fig.tight_layout()
    fig.savefig(fig_dir / "coverage-by-key.png", dpi=120, metadata={"Software": None})
    plt.close(fig)


if __name__ == "__main__":
    raise SystemExit(main())

"""EXP-0006: does the challenger's prior rule bring the 80% ranges into [0.75, 0.85]?
(preregistration.md, issues #101 and #13)

A comparison of the champion (production) and the challenger (`ALGO_VARIANT=challenger`, frozen
before registration) on fresh draws of EXP-0002's design (its frozen generator, imported, on the
seed namespace EXP-0006), paired: both rules see the same players. The primary is the worst of
EXP-0002's 20 key x checkpoint coverages; the guards are the coverage at the peers' rate, the
false weaknesses among players at or below it, and the Tier 1 guards from EXP-0005's side-by-side
run of the same challenger (recall at 2, clean false alarms, top-2 stability).

    python analysis.py --out DIR    # DIR/results.json and DIR/figures/
"""

from __future__ import annotations

import argparse
import importlib.util
import json
import math
from concurrent.futures import ProcessPoolExecutor
from pathlib import Path

import numpy as np
from scipy import stats

import rematch_lab as lab
from rematch_lab.experiments import Budget, seed, write_results
from rematch_lab.models import ProfileRequest

EXP = "EXP-0006"
SEED_NS = EXP
HERE = Path(__file__).resolve().parent
EXPERIMENTS = HERE.parent
VARIANTS = ("champion", "challenger")
PLAYERS = 200
WORKERS = 6
CHUNK = 200
BOOTSTRAP = 2000
NOMINAL = 0.80
BAND = (0.75, 0.85)
MIN_EFFECT = 0.05  # in D, the worst cell's distance from 0.80
SHIP_LEVEL = 0.975
GUARD_LEVEL = 0.80
MARGINS = {
    "rho1_deviation": 0.05,  # |coverage - 0.80| at the peers' rate may grow by at most this
    "false_credible": 0.01,  # credible estimates among players at or below the peers' rate
    "recall_at_2": 0.10,  # EXP-0001's smoke tolerance
    "clean_rank1": 0,  # no new clean false alarm (bench::results::regressions)
    "top2_agreement": 0.25,  # bench/baseline.json's tolerance
}
BUDGET = {"profile_calls": 51_200}
SEED_NAMES = ["players/<band>/<rho>/<disp> (30 cells)", "issue13/none", "issue13/de1.5", "bootstrap/paired"]


def r6(x):
    return None if x is None or not math.isfinite(float(x)) else round(float(x), 6)


def load(name: str, rel: str):
    spec = importlib.util.spec_from_file_location(name, EXPERIMENTS / rel)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def load_exp2():
    exp2 = load("exp2", "EXP-0002/analysis.py")
    exp2.SEED_NS = SEED_NS
    exp2.PLAYERS_PER_CELL = PLAYERS
    return exp2


def run_cell(job: tuple) -> tuple[str, list[tuple]]:
    """One cell's players under one rule, in a worker process: the cell is drawn from its own
    named seed, so a worker needs nothing but the cell's name."""
    variant, band, rho, disp = job
    exp2 = load_exp2()
    data = exp2.simulate_cell(band, rho, disp)
    out, ids = [], set()
    batch = [
        (exp2.facts(band, f"{band}-{rho}-{disp}-{i:03d}-{g}", data, i, g, "spread", rho), i, g)
        for i in range(PLAYERS)
        for g in exp2.CHECKPOINTS
    ]
    for j in range(0, len(batch), CHUNK):
        part = batch[j : j + CHUNK]
        resp = lab.profile(ProfileRequest(players=[b[0] for b in part]), variant=variant)
        ids.add(resp.algorithm_id)
        for (f, i, g), p in zip(part, resp.players, strict=True):
            if p.id != f.id:
                raise SystemExit("profile responses out of order")
            by = {e.key: e for e in p.estimates}
            for key in data:
                e = by[key]
                theta = exp2.theta_of(key, band, rho)
                iv = e.posterior.interval
                out.append((key, g, i, iv.low <= theta <= iv.high, e.credible, iv.high < theta))
    if len(ids) != 1:
        raise SystemExit("the algorithm_id changed during the run")
    return ids.pop(), out


def run_variant(exp2, variant: str, budget: Budget) -> tuple[str, dict]:
    """Every interval of every player under `variant`: covered[(key, games, cell)] and
    credible[...] as arrays over the cell's players, in player order."""
    plan = [(b, r, d) for b, r, d in exp2.CELLS] + [(exp2.ISSUE13["band"], "issue13", d) for d in exp2.DISPERSIONS]
    calls = sum(len(exp2.CHECKPOINTS) * PLAYERS for _ in plan)
    budget.spend("profile_calls", calls)
    with ProcessPoolExecutor(WORKERS) as ex:
        parts = list(ex.map(run_cell, [(variant, *c) for c in plan]))
    ids = {a for a, _ in parts}
    if len(ids) != 1:
        raise SystemExit("the algorithm_id changed during the run")
    covered: dict[tuple, np.ndarray] = {}
    credible: dict[tuple, np.ndarray] = {}
    high_miss: dict[tuple, np.ndarray] = {}
    for cell, (_, rows) in zip(plan, parts, strict=True):
        for key, g, i, cov, cred, hm in rows:
            k = (key, g, cell)
            for store, v in ((covered, cov), (credible, cred), (high_miss, hm)):
                store.setdefault(k, np.zeros(PLAYERS, dtype=bool))[i] = v
    return ids.pop(), {"covered": covered, "credible": credible, "high_miss": high_miss}


def cells_of(exp2, rhos) -> list[tuple]:
    return [(b, r, d) for b, r, d in exp2.CELLS if r in rhos]


def stat(arrs: dict, keys_g: list[tuple], cells: list[tuple], idx: np.ndarray | None) -> np.ndarray:
    """The equal-weighted mean over cells of the mean over players (each player's share of the
    intervals in keys_g); with idx (B x players per cell), one value per resample."""
    per_cell = []
    for c in cells:
        player = np.mean([arrs[(k, g, c)] for k, g in keys_g], axis=0)  # (players,)
        per_cell.append(player[idx].mean(axis=-1) if idx is not None else player.mean())
    return np.mean(per_cell, axis=0)


def ci(boot: np.ndarray, level: float) -> list:
    a = (1 - level) / 2
    return [r6(np.quantile(boot, a)), r6(np.quantile(boot, 1 - a))]


def wilson(x: int, n: int, z: float) -> tuple[float, float]:
    p = x / n
    den = 1 + z * z / n
    c = (p + z * z / (2 * n)) / den
    h = z * math.sqrt(p * (1 - p) / n + z * z / (4 * n * n)) / den
    return c - h, c + h


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", required=True)
    ap.add_argument("--algorithm", default=None)
    args = ap.parse_args()
    out_dir = Path(args.out)
    (out_dir / "figures").mkdir(parents=True, exist_ok=True)
    if args.algorithm is not None and lab.algorithm()["id"] != args.algorithm:
        raise SystemExit(f"the cli is not the requested {args.algorithm}")

    exp2 = load_exp2()
    exp1 = load("exp1", "EXP-0001/analysis.py")
    tier1 = json.loads((EXPERIMENTS / "EXP-0005" / "results.json").read_text(encoding="utf-8"))
    budget = Budget(BUDGET)

    ids, data = {}, {}
    for v in VARIANTS:
        ids[v], data[v] = run_variant(exp2, v, budget)
    if len(set(ids.values())) != 1 or tier1["algorithm_id"] != ids["champion"]:
        raise SystemExit("the two rules and EXP-0005 must run on one build")
    algorithm_id = ids["champion"]

    rng = np.random.default_rng(seed(SEED_NS, "bootstrap/paired"))
    idx = rng.integers(0, PLAYERS, size=(BOOTSTRAP, PLAYERS))  # one draw of players, every cell
    keys, gs = list(exp2.KEYS), exp2.CHECKPOINTS
    prim_cells = cells_of(exp2, exp2.PRIMARY_RHOS)
    z_bonf = float(stats.norm.ppf(1 - 0.05 / (2 * len(keys) * len(gs))))

    # Primary: D, the worst of the 20 coverages' distance from 0.80, per rule; Delta D paired.
    per_rule = {}
    for v in VARIANTS:
        cov = data[v]["covered"]
        cells, point, boots = [], [], []
        for k in keys:
            for g in gs:
                est = float(stat(cov, [(k, g)], prim_cells, None))
                bs = stat(cov, [(k, g)], prim_cells, idx)
                se = float(bs.std(ddof=1))
                lo, hi = est - z_bonf * se, est + z_bonf * se
                cls = "within the band" if BAND[0] <= lo and hi <= BAND[1] else "outside the band" if hi < BAND[0] or lo > BAND[1] else "need more data"
                miss = float(stat(data[v]["high_miss"], [(k, g)], prim_cells, None))
                cells.append({"key": k, "games": g, "coverage": r6(est), "ci95": ci(bs, 0.95), "ci_bonferroni": [r6(lo), r6(hi)], "classification": cls, "true_rate_above": r6(miss)})
                point.append(est)
                boots.append(bs)
        dev = np.abs(np.array(point) - NOMINAL)
        dboot = np.abs(np.array(boots) - NOMINAL).max(axis=0)
        worst = int(dev.argmax())
        per_rule[v] = {
            "D": r6(dev.max()),
            "D_ci95": ci(dboot, 0.95),
            "worst": cells[worst],
            "cells": cells,
            "within": sum(c["classification"] == "within the band" for c in cells),
            "outside": sum(c["classification"] == "outside the band" for c in cells),
            "_boot": dboot,
        }
    delta_boot = per_rule["champion"]["_boot"] - per_rule["challenger"]["_boot"]
    delta = per_rule["champion"]["D"] - per_rule["challenger"]["D"]
    primary = {
        "estimand": "Delta D = D(champion) - D(challenger), D the largest |coverage - 0.80| over the 20 key x checkpoint coverages (rho in {2, 4, 8}, two games a day); positive is better",
        "delta_D": r6(delta),
        "ci80": ci(delta_boot, 0.80),
        "ci95": ci(delta_boot, 0.95),
        "ci97_5": ci(delta_boot, SHIP_LEVEL),
    }
    for v in VARIANTS:
        per_rule[v].pop("_boot")

    # Guards from the profile simulation, paired on the same resamples.
    all_kg = [(k, g) for k in keys for g in gs]
    rho1 = cells_of(exp2, [1.0])
    low = cells_of(exp2, [0.5, 1.0])
    guards = {}
    g1 = {v: (float(stat(data[v]["covered"], all_kg, rho1, None)), stat(data[v]["covered"], all_kg, rho1, idx)) for v in VARIANTS}
    d1 = abs(g1["challenger"][0] - NOMINAL) - abs(g1["champion"][0] - NOMINAL)
    d1b = np.abs(g1["challenger"][1] - NOMINAL) - np.abs(g1["champion"][1] - NOMINAL)
    guards["rho1_deviation"] = {
        "champion": r6(g1["champion"][0]), "challenger": r6(g1["challenger"][0]),
        "worse_by": r6(d1), "ci80": ci(d1b, GUARD_LEVEL), "margin": MARGINS["rho1_deviation"],
    }
    g2 = {v: (float(stat(data[v]["credible"], all_kg, low, None)), stat(data[v]["credible"], all_kg, low, idx)) for v in VARIANTS}
    guards["false_credible"] = {
        "champion": r6(g2["champion"][0]), "challenger": r6(g2["challenger"][0]),
        "worse_by": r6(g2["challenger"][0] - g2["champion"][0]),
        "ci80": ci(g2["challenger"][1] - g2["champion"][1], GUARD_LEVEL), "margin": MARGINS["false_credible"],
    }
    # Tier 1, from EXP-0005's side-by-side run (pilot and smoke, 80 games, flawed players).
    t = tier1["primary"]["table"]
    d, lo, hi = exp1.newcombe_paired(t["n11"], t["n10"], t["n01"], t["n00"], exp1.z_of(GUARD_LEVEL))
    guards["recall_at_2"] = {
        "players": t["n11"] + t["n10"] + t["n01"] + t["n00"], "table": {k: t[k] for k in ("n11", "n10", "n01", "n00")},
        "worse_by": r6(-float(d)), "ci80_difference": [r6(float(lo)), r6(float(hi))], "margin": MARGINS["recall_at_2"],
    }
    ref_alarms = sum(c["reference_rank1"] is not None for c in tier1["clean"])
    cand_alarms = sum(c["candidate_rank1"] is not None for c in tier1["clean"])
    guards["clean_rank1"] = {"checkpoints": len(tier1["clean"]), "champion": ref_alarms, "challenger": cand_alarms, "margin": 0}
    sm = tier1["scorecards"]["smoke"]
    guards["top2_agreement"] = {
        "champion": sm["reference"]["top2_agreement"], "challenger": sm["candidate"]["top2_agreement"],
        "worse_by": r6(sm["reference"]["top2_agreement"] - sm["candidate"]["top2_agreement"]),
        "margin": MARGINS["top2_agreement"],
    }
    holds = {
        "rho1_deviation": guards["rho1_deviation"]["ci80"][1] <= MARGINS["rho1_deviation"],
        "false_credible": guards["false_credible"]["ci80"][1] <= MARGINS["false_credible"],
        "recall_at_2": guards["recall_at_2"]["ci80_difference"][0] >= -MARGINS["recall_at_2"],
        "clean_rank1": cand_alarms - ref_alarms <= 0,
        "top2_agreement": guards["top2_agreement"]["worse_by"] <= MARGINS["top2_agreement"],
    }
    beyond = {
        "rho1_deviation": guards["rho1_deviation"]["ci80"][0] > MARGINS["rho1_deviation"],
        "false_credible": guards["false_credible"]["ci80"][0] > MARGINS["false_credible"],
        "recall_at_2": guards["recall_at_2"]["ci80_difference"][1] < -MARGINS["recall_at_2"],
        "clean_rank1": cand_alarms - ref_alarms > 0,
        "top2_agreement": guards["top2_agreement"]["worse_by"] > MARGINS["top2_agreement"],
    }
    for k in guards:
        guards[k]["holds"] = holds[k]
        guards[k]["wholly_beyond"] = beyond[k]

    lo97, hi97 = primary["ci97_5"]
    if lo97 > 0 and all(holds.values()):
        decision = "ship"
    elif hi97 < MIN_EFFECT or any(beyond.values()):
        decision = "reject"
    else:
        decision = "need more data"

    # Secondary (confirmatory, decides #13 only): every challenger coverage within the band, and
    # #13's own Tier 1 acceptance on the smoke suite.
    smoke_cov = sm["candidate"]["coverage"]
    hang = next((p for p in tier1["players"] if p["name"] == "bench-hang"), None)
    hang_rank1 = next((c["rank1"] for c in hang["candidate"] if c["games"] == 80), None) if hang else None
    fixed = {
        "all_20_within_band": per_rule["challenger"]["within"] == 20,
        "smoke_coverage": smoke_cov,
        "smoke_coverage_in_0_65_0_95": smoke_cov is not None and 0.65 <= smoke_cov <= 0.95,
        "bench_hang_rank1_at_80": hang_rank1,
    }
    fixed["fixes_13"] = fixed["all_20_within_band"] and fixed["smoke_coverage_in_0_65_0_95"] and hang_rank1 == "W1"

    # Exploratory: by key over the primary rhos, by rho over every key, and the #13 cells.
    explore = {"by_rho": [], "issue13": []}
    for rho in exp2.RHOS:
        cs = cells_of(exp2, [rho])
        explore["by_rho"].append({"rho": rho, **{v: r6(stat(data[v]["covered"], all_kg, cs, None)) for v in VARIANTS},
                                  **{f"{v}_credible": r6(stat(data[v]["credible"], all_kg, cs, None)) for v in VARIANTS}})
    z = 1.959964
    for disp in exp2.DISPERSIONS:
        cell = (exp2.ISSUE13["band"], "issue13", disp)
        for g in gs:
            row = {"disp": disp, "games": g}
            for v in VARIANTS:
                arr = data[v]["covered"][("W1", g, cell)]
                lo_w, hi_w = wilson(int(arr.sum()), arr.size, z)
                row[v] = {"coverage": r6(arr.mean()), "ci95": [r6(lo_w), r6(hi_w)]}
            explore["issue13"].append(row)

    results = {
        "experiment": EXP,
        "algorithm_id": algorithm_id,
        "variants": list(VARIANTS),
        "design": {"players_per_cell": PLAYERS, "seed_namespace": SEED_NS, "seed_names": SEED_NAMES, "bootstrap": BOOTSTRAP, "z_bonferroni": r6(z_bonf)},
        "primary": primary,
        "per_rule": per_rule,
        "guards": guards,
        "decision": decision,
        "fixed": fixed,
        "exploratory": explore,
        "compute": budget.record(),
    }
    write_results(out_dir, results)
    figures(out_dir / "figures", results, keys, gs)
    return 0


def figures(fig_dir: Path, res: dict, keys: list[str], gs: list[int]) -> None:
    import matplotlib

    matplotlib.use("Agg")
    import matplotlib.pyplot as plt

    fig, axes = plt.subplots(1, len(keys), figsize=(16, 3.6), sharey=True)
    for ax, key in zip(axes, keys, strict=True):
        for v, style in zip(VARIANTS, ("--", "-"), strict=True):
            cs = [c for c in res["per_rule"][v]["cells"] if c["key"] == key]
            ys = [c["coverage"] for c in cs]
            lo = [c["ci_bonferroni"][0] for c in cs]
            hi = [c["ci_bonferroni"][1] for c in cs]
            ax.errorbar(gs, ys, yerr=[np.subtract(ys, lo), np.subtract(hi, ys)], ls=style, marker="o", ms=3, capsize=2, label=v)
        ax.axhspan(*BAND, color="grey", alpha=0.15)
        ax.set_xscale("log")
        ax.set_title(key)
        ax.set_xlabel("games")
    axes[0].set_ylabel("coverage of the 80% range")
    axes[-1].legend(fontsize=7)
    fig.tight_layout()
    fig.savefig(fig_dir / "coverage-champion-challenger.png", dpi=120, metadata={"Software": None})
    plt.close(fig)


if __name__ == "__main__":
    raise SystemExit(main())

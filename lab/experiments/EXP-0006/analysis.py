"""EXP-0006: does the challenger's prior rule bring the 80% ranges into [0.75, 0.85]?
(preregistration.md, issues #101 and #13)

Two parts, both comparing the champion (production) with the challenger (`ALGO_VARIANT=
challenger`, frozen before registration):

1. The profile simulation: fresh draws of EXP-0002's design (its frozen generator, imported, on
   the seed namespace EXP-0006), paired, scored against the true rates.
2. The Tier 1 run: EXP-0001's pilot players, the smoke suite and 20 new clean controls through the
   real pipeline, with the challenger side by side on the same games (`cli bench run` with
   `ALGO_VARIANT`). It gives the Tier 1 guards and, descriptively, the churn EXP-0001 assumed.

    python analysis.py --out DIR    # DIR/results.json and DIR/figures/
"""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import math
import re
import tempfile
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
VARIANT = "challenger"
VARIANTS = ("champion", VARIANT)
# Players a cell: 200 where the primary reads (rho 2, 4, 8), 800 where the false-weakness and
# rho = 1 guards read (rho 0.5, 1) and in the #13 cell (section 6).
PRIMARY_PLAYERS = 200
GUARD_PLAYERS = 800
ISSUE13_PLAYERS = 800
WORKERS = 6
CHUNK = 200
BOOTSTRAP = 2000
NOMINAL = 0.80
BAND = (0.75, 0.85)
TOLERANCE = 0.07  # D(challenger): the band's half-width plus EXP-0002's Bonferroni resolution
SHIP_LEVEL = 0.975
GUARD_LEVEL = 0.80
MARGINS = {
    "rho1_deviation": 0.05,
    "false_credible": 0.01,
    "recall_at_2": 0.10,
    "clean_alarm": 0.10,
    "top2_agreement": 0.25,
}
# The Tier 1 run (section 7).
ENGINES = 8
G = 80
CLEAN_PLAYERS = 20
CLEAN_SUITE = {
    "name": "exp-0006-clean",
    "checkpoints": [30, 45, 60, 80],
    "nodes": 12_000,
    "opponents": 16,
    "perturb": False,
}
SMOKE_GAMES = 1280
BUDGET = {"profile_calls": 118_400, "games": 6_720}
SEED_NAMES = [
    "players/<band>/<rho>/<disp> (30 cells)",
    "issue13/none",
    "issue13/de1.5",
    "clean/<j> (j = 0..19)",
    "bootstrap/paired",
]
# The sha256 of sections 0 to 9 of preregistration.md as registered after the critic's review
# (section 10 holds this file's own sha256); the run stops if they change (review concern 5).
PREREG_SHA = "0" * 64


def r6(x):
    return None if x is None or not math.isfinite(float(x)) else round(float(x), 6)


def load(name: str, rel: str):
    spec = importlib.util.spec_from_file_location(name, EXPERIMENTS / rel)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def prereg_sha() -> str:
    text = (HERE / "preregistration.md").read_text(encoding="utf-8")
    marks = list(re.finditer(r"^## (\d+)\.", text, re.MULTILINE))
    parts = [
        text[m.start() : marks[i + 1].start() if i + 1 < len(marks) else None]
        for i, m in enumerate(marks)
        if int(m.group(1)) <= 9
    ]
    return hashlib.sha256("".join(parts).encode()).hexdigest()


# ---- part 1: the profile simulation ------------------------------------------------------------


def players_of_cell(rho) -> int:
    if rho == "issue13":
        return ISSUE13_PLAYERS
    return GUARD_PLAYERS if rho in (0.5, 1.0) else PRIMARY_PLAYERS


def load_exp2(seed_ns: str, players: int):
    exp2 = load("exp2", "EXP-0002/analysis.py")
    exp2.SEED_NS = seed_ns
    exp2.PLAYERS_PER_CELL = players
    return exp2


def run_cell(job: tuple) -> tuple[str, list[tuple]]:
    """One cell's players under one rule, in a worker process: the cell is drawn from its own
    named seed, so the worker needs only the cell's name and size."""
    seed_ns, variant, band, rho, disp = job
    n = players_of_cell(rho)
    exp2 = load_exp2(seed_ns, n)
    data = exp2.simulate_cell(band, rho, disp)
    out, ids = [], set()
    batch = [
        (exp2.facts(band, f"{band}-{rho}-{disp}-{i:03d}-{g}", data, i, g, "spread", rho), i, g)
        for i in range(n)
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
                out.append(
                    (key, g, i, iv.low <= theta <= iv.high, e.credible, iv.high < theta,
                     (iv.high - iv.low) / theta)
                )
    if len(ids) != 1:
        raise SystemExit("the algorithm_id changed during the run")
    return ids.pop(), out


def plan_of(exp2) -> list[tuple]:
    return [(b, r, d) for b, r, d in exp2.CELLS] + [
        (exp2.ISSUE13["band"], "issue13", d) for d in exp2.DISPERSIONS
    ]


def profile_calls(exp2) -> int:
    return sum(players_of_cell(r) * len(exp2.CHECKPOINTS) for _, r, _ in plan_of(exp2))


def run_profiles(seed_ns: str, budget: Budget | None) -> tuple[str, dict, list[tuple]]:
    """Both rules on every cell: data[variant][store][(key, games, cell)] = array over players."""
    exp2 = load_exp2(seed_ns, PRIMARY_PLAYERS)
    plan = plan_of(exp2)
    data, ids = {}, set()
    for v in VARIANTS:
        if budget is not None:
            budget.spend("profile_calls", profile_calls(exp2))
        with ProcessPoolExecutor(WORKERS) as ex:
            parts = list(ex.map(run_cell, [(seed_ns, v, *c) for c in plan]))
        stores = {"covered": {}, "credible": {}, "high_miss": {}, "width": {}}
        for cell, (aid, rows) in zip(plan, parts, strict=True):
            ids.add(aid)
            n = players_of_cell(cell[1])
            for key, g, i, cov, cred, hm, w in rows:
                k = (key, g, cell)
                for name, val in (("covered", cov), ("credible", cred), ("high_miss", hm), ("width", w)):
                    arr = stores[name].setdefault(k, np.zeros(n, dtype=float))
                    arr[i] = val
        data[v] = stores
    if len(ids) != 1:
        raise SystemExit("the two rules ran on different builds")
    return ids.pop(), data, plan


def cells_of(plan, rhos) -> list[tuple]:
    return [c for c in plan if c[1] in rhos]


def stat(arrs: dict, keys_g: list[tuple], cells: list[tuple], idx: dict | None) -> np.ndarray:
    """The equal-weighted mean over cells of the mean over players (each player's share of the
    intervals in keys_g); with idx (per cell, B x players), one value per resample."""
    per_cell = []
    for c in cells:
        player = np.mean([arrs[(k, g, c)] for k, g in keys_g], axis=0)
        per_cell.append(player[idx[c]].mean(axis=-1) if idx is not None else player.mean())
    return np.mean(per_cell, axis=0)


def ci(boot: np.ndarray, level: float) -> list:
    a = (1 - level) / 2
    return [r6(np.quantile(boot, a)), r6(np.quantile(boot, 1 - a))]


def resamples(plan, rng) -> dict:
    """One draw of player indices per cell (in plan order), shared by both rules."""
    return {c: rng.integers(0, players_of_cell(c[1]), size=(BOOTSTRAP, players_of_cell(c[1]))) for c in plan}


def profile_statistics(exp2, data: dict, plan, idx: dict) -> dict:
    """Every profile quantity of the rule, with its bootstrap replicates (`_boot`)."""
    keys, gs = list(exp2.KEYS), exp2.CHECKPOINTS
    prim = cells_of(plan, exp2.PRIMARY_RHOS)
    z_bonf = float(stats.norm.ppf(1 - 0.05 / (2 * len(keys) * len(gs))))
    out = {"per_rule": {}}
    for v in VARIANTS:
        cov = data[v]["covered"]
        cells, point, boots = [], [], []
        for k in keys:
            for g in gs:
                est = float(stat(cov, [(k, g)], prim, None))
                bs = stat(cov, [(k, g)], prim, idx)
                se = float(bs.std(ddof=1))
                lo, hi = est - z_bonf * se, est + z_bonf * se
                cls = (
                    "within the band" if BAND[0] <= lo and hi <= BAND[1]
                    else "outside the band" if hi < BAND[0] or lo > BAND[1]
                    else "need more data"
                )
                cells.append(
                    {
                        "key": k, "games": g, "coverage": r6(est), "ci95": ci(bs, 0.95),
                        "ci_bonferroni": [r6(lo), r6(hi)], "classification": cls,
                        "true_rate_above": r6(stat(data[v]["high_miss"], [(k, g)], prim, None)),
                        "median_width_ratio": r6(np.median(np.concatenate([data[v]["width"][(k, g, c)] for c in prim]))),
                    }
                )
                point.append(est)
                boots.append(bs)
        dev = np.abs(np.array(point) - NOMINAL)
        dboot = np.abs(np.array(boots) - NOMINAL).max(axis=0)
        out["per_rule"][v] = {
            "D": r6(dev.max()), "D_ci95": ci(dboot, 0.95), "D_ci97_5": ci(dboot, SHIP_LEVEL),
            "worst": cells[int(dev.argmax())], "cells": cells,
            "within": sum(c["classification"] == "within the band" for c in cells),
            "outside": sum(c["classification"] == "outside the band" for c in cells),
            "_D": float(dev.max()), "_boot": dboot,
        }
    ch, ce = out["per_rule"]["champion"], out["per_rule"][VARIANT]
    out["primary"] = {
        "estimand": "D(challenger), the largest |coverage - 0.80| over the 20 key x checkpoint "
        "coverages (rho in {2, 4, 8}, two games a day); smaller is better",
        "D": ce["D"], "ci80": ci(ce["_boot"], 0.80), "ci95": ce["D_ci95"], "ci97_5": ce["D_ci97_5"],
        "tolerance": TOLERANCE,
    }
    delta = ch["_boot"] - ce["_boot"]
    out["delta_D"] = {
        "estimand": "D(champion) - D(challenger); positive is better",
        "delta_D": r6(ch["_D"] - ce["_D"]), "ci80": ci(delta, 0.80), "ci95": ci(delta, 0.95),
        "ci97_5": ci(delta, SHIP_LEVEL),
    }
    all_kg = [(k, g) for k in keys for g in gs]
    rho1 = cells_of(plan, [1.0])
    low = cells_of(plan, [0.5, 1.0])
    g1 = {v: (float(stat(data[v]["covered"], all_kg, rho1, None)), stat(data[v]["covered"], all_kg, rho1, idx)) for v in VARIANTS}
    d1b = np.abs(g1[VARIANT][1] - NOMINAL) - np.abs(g1["champion"][1] - NOMINAL)
    out["rho1_deviation"] = {
        "champion": r6(g1["champion"][0]), "challenger": r6(g1[VARIANT][0]),
        "worse_by": r6(abs(g1[VARIANT][0] - NOMINAL) - abs(g1["champion"][0] - NOMINAL)),
        "ci80": ci(d1b, GUARD_LEVEL), "margin": MARGINS["rho1_deviation"],
        "per_key": {
            k: {v: r6(stat(data[v]["covered"], [(k, g) for g in gs], rho1, None)) for v in VARIANTS}
            for k in keys
        },
    }
    g2 = {v: (float(stat(data[v]["credible"], all_kg, low, None)), stat(data[v]["credible"], all_kg, low, idx)) for v in VARIANTS}
    out["false_credible"] = {
        "champion": r6(g2["champion"][0]), "challenger": r6(g2[VARIANT][0]),
        "worse_by": r6(g2[VARIANT][0] - g2["champion"][0]),
        "ci80": ci(g2[VARIANT][1] - g2["champion"][1], GUARD_LEVEL), "margin": MARGINS["false_credible"],
        "per_key": {
            k: {v: r6(stat(data[v]["credible"], [(k, g) for g in gs], low, None)) for v in VARIANTS}
            for k in keys
        },
    }
    out["rho_half_coverage"] = {
        v: r6(stat(data[v]["covered"], all_kg, cells_of(plan, [0.5]), None)) for v in VARIANTS
    }
    # The #13 cell, pooled over the two dispersions, per checkpoint: Wilson with a Bonferroni z for 4.
    z4 = float(stats.norm.ppf(1 - 0.05 / 8))
    issue = []
    for g in gs:
        row = {"games": g}
        for v in VARIANTS:
            arr = np.concatenate([data[v]["covered"][("W1", g, c)] for c in cells_of(plan, ["issue13"])])
            lo, hi = wilson(int(arr.sum()), arr.size, z4)
            row[v] = {"coverage": r6(arr.mean()), "ci_bonferroni_wilson": [r6(lo), r6(hi)], "players": int(arr.size)}
        issue.append(row)
    out["issue13"] = issue
    return out


def wilson(x: int, n: int, z: float) -> tuple[float, float]:
    p = x / n
    den = 1 + z * z / n
    c = (p + z * z / (2 * n)) / den
    h = z * math.sqrt(p * (1 - p) / n + z * z / (4 * n * n)) / den
    return c - h, c + h


def jeffreys(x: int, n: int, level: float) -> list:
    if n == 0:
        return [None, None]
    a = (1 - level) / 2
    lo = 0.0 if x == 0 else float(stats.beta.ppf(a, x + 0.5, n - x + 0.5))
    hi = 1.0 if x == n else float(stats.beta.ppf(1 - a, x + 0.5, n - x + 0.5))
    return [r6(lo), r6(hi)]


# ---- part 2: the Tier 1 run --------------------------------------------------------------------


def side_rows(player: dict, checkpoints: list[dict], truth) -> list[dict]:
    flaw = player.get("flaw")
    target = flaw["target_key"] if flaw else None
    out = []
    for c in checkpoints:
        ranks = {r["key"]: r["rank"] for r in c["ranked"]}
        est = next((e for e in c["estimates"] if e["key"] == target), None) if target else None
        out.append(
            {
                "games": c["games"],
                "top": [r["key"] for r in c["ranked"][:3]],
                "rank1": c["ranked"][0]["key"] if c["ranked"] else None,
                "target_rank": ranks.get(target) if target else None,
                "hit2": (ranks.get(target, 99) <= 2) if target else None,
                "estimate": None if est is None else [r6(est["mean"]), r6(est["low"]), r6(est["high"])],
                "covered": None if est is None or truth is None else bool(est["low"] <= truth <= est["high"]),
            }
        )
    return out


def tier1_players(run: dict, suite: str) -> list[dict]:
    """Both sides of every player, after the integrity checks of review concern 4."""
    truth = {c["player"]: c.get("truth") for c in run["scorecard"]["cases"]}
    out = []
    for p in run["players"]:
        v = p.get("variant")
        if v is None or v["name"] != VARIANT:
            raise SystemExit(f"{p['name']}: the side-by-side variant is not {VARIANT}")
        if len(v["checkpoints"]) != len(p["checkpoints"]):
            raise SystemExit(f"{p['name']}: the two sides have different checkpoints")
        for a, b in zip(p["checkpoints"], v["checkpoints"], strict=True):
            facts_a = {(e["key"], e["events"], e["opportunities"]) for e in a["estimates"]}
            facts_b = {(e["key"], e["events"], e["opportunities"]) for e in b["estimates"]}
            if a["games"] != b["games"] or facts_a != facts_b:
                raise SystemExit(f"{p['name']} at {a['games']} games: the two sides saw different facts")
        t = truth.get(p["name"])
        out.append(
            {
                "suite": suite, "name": p["name"],
                "flaw": p["flaw"]["id"] if p.get("flaw") else None,
                "target": p["flaw"]["target_key"] if p.get("flaw") else None,
                "truth": r6(t) if t is not None else None,
                "deterministic": [p["deterministic"], v["deterministic"]],
                "reference": side_rows(p, p["checkpoints"], t),
                "candidate": side_rows(p, v["checkpoints"], t),
            }
        )
    return out


def moved(run: dict) -> int:
    """Estimates whose interval differs between the sides (the positive control)."""
    n = 0
    for p in run["players"]:
        for a, b in zip(p["checkpoints"], p["variant"]["checkpoints"], strict=True):
            ea = {e["key"]: (e["low"], e["high"]) for e in a["estimates"]}
            n += sum(ea.get(e["key"]) != (e["low"], e["high"]) for e in b["estimates"])
    return n


def at(rows: list[dict], g: int):
    return next((r for r in rows if r["games"] == g), None)


def table(players: list[dict], g: int, field: str, pred) -> dict:
    """The paired 2 x 2 of a per-player indicator at checkpoint g: n11 both, n10 candidate only,
    n01 reference only, n00 neither."""
    t = {"n11": 0, "n10": 0, "n01": 0, "n00": 0}
    for p in players:
        if not pred(p):
            continue
        a, b = at(p["reference"], g), at(p["candidate"], g)
        if a is None or b is None:
            continue
        ra, rb = bool(a[field]), bool(b[field])
        t[{(True, True): "n11", (False, True): "n10", (True, False): "n01", (False, False): "n00"}[(ra, rb)]] += 1
    return t


def run_tier1(exp1, budget: Budget) -> dict:
    pilot = exp1.pilot_suite()
    ref = json.loads((EXPERIMENTS / "EXP-0001" / "results.json").read_text(encoding="utf-8"))
    if pilot != ref["pilot"]["suite"]:
        raise SystemExit("the pilot suite differs from EXP-0001's")
    clean = {
        **CLEAN_SUITE,
        "players": [
            {"name": f"exp6-clean-{j}", "flaw": None, "games": G, "seed": seed(SEED_NS, f"clean/{j}")}
            for j in range(CLEAN_PLAYERS)
        ],
    }
    seeds = [p["seed"] for p in clean["players"]] + [p["seed"] for p in pilot["players"]]
    if len(set(seeds)) != len(seeds) or set(seeds) & exp1.NAMED_SUITE_SEEDS:
        raise SystemExit("the clean controls' seeds must be new")
    runs = {}
    with tempfile.TemporaryDirectory() as tmp:
        for name, suite in (("pilot", pilot), ("clean", clean)):
            path = Path(tmp) / f"{name}.json"
            path.write_text(json.dumps(suite), encoding="utf-8")
            budget.spend("games", sum(p["games"] for p in suite["players"]))
            runs[name] = lab.bench(suite_file=path, out_dir=Path(tmp) / name, engines=ENGINES, variant=VARIANT)
        budget.spend("games", SMOKE_GAMES)
        runs["smoke"] = lab.bench("smoke", out_dir=Path(tmp) / "smoke", engines=ENGINES, variant=VARIANT)
    if sum(p["games"] for p in runs["smoke"]["suite"]["players"]) != SMOKE_GAMES:
        raise SystemExit("the smoke suite is not EXP-0001's 1,280 games")
    players = [q for s in ("pilot", "smoke", "clean") for q in tier1_players(runs[s], s)]
    if not all(all(p["deterministic"]) for p in players):
        raise SystemExit("a side was not deterministic")
    positive = {s: moved(runs[s]) for s in runs}
    if sum(positive.values()) == 0:
        raise SystemExit("the challenger moved no estimate: it was not applied")
    # Between runs: the reference side against EXP-0001's stored pilot rows, field by field.
    stored = {p["name"]: p for p in ref["pilot"]["players"]}
    fields = ("rank1", "target_rank", "hit2", "estimate", "covered")
    mismatches = [
        {"player": p["name"], "games": a["games"], "field": f}
        for p in players
        if p["suite"] == "pilot"
        for a, b in zip(p["reference"], stored[p["name"]]["checkpoints"], strict=True)
        for f in fields
        if a[f] != b[f]
    ]
    if mismatches:
        raise SystemExit(f"the reference side differs from EXP-0001's stored pilot: {mismatches[:5]}")
    return {
        "runs": {s: {"scorecard": runs[s]["scorecard"]["metrics"], "variant_scorecard": runs[s]["variant"]["scorecard"]["metrics"]} for s in runs},
        "players": players,
        "positive_control_moved_estimates": positive,
        "between_runs": {"pilot_fields_compared": len(fields) * sum(len(p["reference"]) for p in players if p["suite"] == "pilot"), "mismatches": 0},
        "rank1_field": "rank1",
    }


def tier1_statistics(exp1, t1: dict) -> dict:
    players = t1["players"]
    flawed = lambda p: p["flaw"] is not None and p["suite"] in ("pilot", "smoke")  # noqa: E731
    clean = lambda p: p["flaw"] is None  # noqa: E731
    rec = table(players, G, "hit2", flawed)
    d, lo, hi = exp1.newcombe_paired(rec["n11"], rec["n10"], rec["n01"], rec["n00"], exp1.z_of(GUARD_LEVEL))
    # Clean alarm: a clean player with any rank-1 weakness at 80 games (pilot, smoke and the 20).
    for p in players:
        for side in ("reference", "candidate"):
            for c in p[side]:
                c["alarm"] = c["rank1"] is not None
    cl = table(players, G, "alarm", clean)
    dc, loc, hic = exp1.newcombe_paired(cl["n11"], cl["n10"], cl["n01"], cl["n00"], exp1.z_of(GUARD_LEVEL))
    sm = t1["runs"]["smoke"]
    out = {
        "recall_at_2": {
            "players": sum(rec.values()), "table": rec, "difference": r6(float(d)),
            "ci80_difference": [r6(float(lo)), r6(float(hi))], "margin": MARGINS["recall_at_2"],
        },
        "clean_alarm": {
            "players": sum(cl.values()), "table": cl, "difference": r6(float(dc)),
            "ci80_difference": [r6(float(loc)), r6(float(hic))], "margin": MARGINS["clean_alarm"],
        },
        "top2_agreement": {
            "champion": sm["scorecard"]["top2_agreement"], "challenger": sm["variant_scorecard"]["top2_agreement"],
            "worse_by": r6(sm["scorecard"]["top2_agreement"] - sm["variant_scorecard"]["top2_agreement"]),
            "margin": MARGINS["top2_agreement"],
        },
    }
    # Descriptive: the churn EXP-0001 assumed (EXP-0005, closed unrun, review concern 1).
    churn = []
    for g in sorted({c["games"] for p in players for c in p["reference"]}):
        t = table(players, g, "hit2", flawed)
        hits, misses = t["n11"] + t["n01"], t["n10"] + t["n00"]
        churn.append(
            {
                "games": g, "table": t, "reference_hits": hits, "lost": t["n01"],
                "churn": r6(t["n01"] / hits) if hits else None, "churn_ci95_jeffreys": jeffreys(t["n01"], hits, 0.95),
                "gained": t["n10"], "gain": r6(t["n10"] / misses) if misses else None,
                "gain_ci95_jeffreys": jeffreys(t["n10"], misses, 0.95),
            }
        )
    per_flaw = {}
    for p in players:
        if flawed(p):
            a, b = at(p["reference"], G), at(p["candidate"], G)
            f = per_flaw.setdefault(p["flaw"], {"n11": 0, "n10": 0, "n01": 0, "n00": 0})
            f[{(True, True): "n11", (False, True): "n10", (True, False): "n01", (False, False): "n00"}[(bool(a["hit2"]), bool(b["hit2"]))]] += 1
    rank_churn = []
    for s in ("pilot", "smoke", "clean"):
        for g in sorted({c["games"] for p in players if p["suite"] == s for c in p["reference"]}):
            pairs = [(at(p["reference"], g), at(p["candidate"], g)) for p in players if p["suite"] == s]
            pairs = [(a, b) for a, b in pairs if a and b]
            rank_churn.append(
                {
                    "suite": s, "games": g, "players": len(pairs),
                    "top2_set_changed": sum(set(a["top"][:2]) != set(b["top"][:2]) for a, b in pairs),
                    "top2_order_changed": sum(a["top"][:2] != b["top"][:2] for a, b in pairs),
                    "rank1_changed": sum(a["rank1"] != b["rank1"] for a, b in pairs),
                }
            )
    out["churn"] = {"by_checkpoint": churn, "per_flaw_at_80": per_flaw, "rank_churn": rank_churn}
    return out


def decide(prof: dict, t1s: dict) -> tuple[str, dict]:
    holds = {
        "rho1_deviation": prof["rho1_deviation"]["ci80"][1] <= MARGINS["rho1_deviation"],
        "false_credible": prof["false_credible"]["ci80"][1] <= MARGINS["false_credible"],
        "recall_at_2": t1s["recall_at_2"]["ci80_difference"][0] >= -MARGINS["recall_at_2"],
        "clean_alarm": t1s["clean_alarm"]["ci80_difference"][1] <= MARGINS["clean_alarm"],
        "top2_agreement": t1s["top2_agreement"]["worse_by"] <= MARGINS["top2_agreement"],
    }
    beyond = {
        "rho1_deviation": prof["rho1_deviation"]["ci80"][0] > MARGINS["rho1_deviation"],
        "false_credible": prof["false_credible"]["ci80"][0] > MARGINS["false_credible"],
        "recall_at_2": t1s["recall_at_2"]["ci80_difference"][1] < -MARGINS["recall_at_2"],
        "clean_alarm": t1s["clean_alarm"]["ci80_difference"][0] > MARGINS["clean_alarm"],
        "top2_agreement": t1s["top2_agreement"]["worse_by"] > MARGINS["top2_agreement"],
    }
    p_lo, p_hi = prof["primary"]["ci97_5"]
    d_lo = prof["delta_D"]["ci97_5"][0]
    if p_hi <= TOLERANCE and d_lo > 0 and all(holds.values()):
        decision = "ship"
    elif p_lo > TOLERANCE or any(beyond.values()):
        decision = "reject"
    else:
        decision = "need more data"
    return decision, {"holds": holds, "wholly_beyond": beyond}


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", required=True)
    ap.add_argument("--algorithm", default=None)
    args = ap.parse_args()
    out_dir = Path(args.out)
    (out_dir / "figures").mkdir(parents=True, exist_ok=True)
    if prereg_sha() != PREREG_SHA:
        raise SystemExit("sections 0 to 9 of preregistration.md changed after the review's fixes")
    if args.algorithm is not None and lab.algorithm()["id"] != args.algorithm:
        raise SystemExit(f"the cli is not the requested {args.algorithm}")
    budget = Budget(BUDGET)
    exp1 = load("exp1", "EXP-0001/analysis.py")
    exp2 = load_exp2(SEED_NS, PRIMARY_PLAYERS)

    t1 = run_tier1(exp1, budget)
    algorithm_id, data, plan = run_profiles(SEED_NS, budget)
    if lab.algorithm()["id"] != algorithm_id:
        raise SystemExit("the Tier 1 run and the profiles ran on different builds")
    idx = resamples(plan, np.random.default_rng(seed(SEED_NS, "bootstrap/paired")))
    prof = profile_statistics(exp2, data, plan, idx)
    t1s = tier1_statistics(exp1, t1)
    decision, guards = decide(prof, t1s)

    # Secondary (confirmatory, decides #13 only): W1's four coverages within the band, the #13
    # cell within it at every checkpoint, and #13's own smoke acceptance.
    w1 = [c for c in prof["per_rule"][VARIANT]["cells"] if c["key"] == "W1"]
    hang = next((p for p in t1["players"] if p["name"] == "bench-hang"), None)
    smoke_cov = t1["runs"]["smoke"]["variant_scorecard"]["coverage"]
    fixed = {
        "w1_within_band": all(c["classification"] == "within the band" for c in w1),
        "issue13_cell_within_band": all(
            BAND[0] <= r[VARIANT]["ci_bonferroni_wilson"][0] and r[VARIANT]["ci_bonferroni_wilson"][1] <= BAND[1]
            for r in prof["issue13"]
        ),
        "smoke_coverage": smoke_cov,
        "smoke_coverage_in_0_65_0_95": smoke_cov is not None and 0.65 <= smoke_cov <= 0.95,
        "bench_hang_rank1_at_80": at(hang["candidate"], G)["rank1"] if hang else None,
        "all_20_within_band": prof["per_rule"][VARIANT]["within"] == 20,
    }
    fixed["fixes_13"] = (
        fixed["w1_within_band"] and fixed["issue13_cell_within_band"]
        and fixed["smoke_coverage_in_0_65_0_95"] and fixed["bench_hang_rank1_at_80"] == "W1"
    )
    for v in VARIANTS:
        for k in ("_D", "_boot"):
            prof["per_rule"][v].pop(k)

    results = {
        "experiment": EXP,
        "algorithm_id": algorithm_id,
        "variant": VARIANT,
        "design": {
            "players_per_cell": {"rho 2, 4, 8": PRIMARY_PLAYERS, "rho 0.5, 1": GUARD_PLAYERS, "issue13": ISSUE13_PLAYERS},
            "seed_namespace": SEED_NS, "seed_names": SEED_NAMES, "bootstrap": BOOTSTRAP,
            "tolerance": TOLERANCE, "margins": MARGINS, "preregistration_sha256_0_9": PREREG_SHA,
        },
        "primary": prof["primary"],
        "delta_D": prof["delta_D"],
        "per_rule": prof["per_rule"],
        "guards": {
            "rho1_deviation": prof["rho1_deviation"], "false_credible": prof["false_credible"],
            "recall_at_2": t1s["recall_at_2"], "clean_alarm": t1s["clean_alarm"],
            "top2_agreement": t1s["top2_agreement"], **guards,
        },
        "decision": decision,
        "fixed": fixed,
        "exploratory": {"rho_half_coverage": prof["rho_half_coverage"], "issue13": prof["issue13"], "churn": t1s["churn"]},
        "tier1": {"scorecards": t1["runs"], "positive_control_moved_estimates": t1["positive_control_moved_estimates"], "between_runs": t1["between_runs"], "players": t1["players"]},
        "compute": budget.record(),
    }
    write_results(out_dir, results)
    figures(out_dir / "figures", results, list(exp2.KEYS), exp2.CHECKPOINTS)
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

    rows = res["exploratory"]["churn"]["by_checkpoint"]
    fig, ax = plt.subplots(figsize=(5, 3.4))
    xs = [r["games"] for r in rows]
    ax.fill_between(xs, [r["churn_ci95_jeffreys"][0] for r in rows], [r["churn_ci95_jeffreys"][1] for r in rows], color="grey", alpha=0.25, label="95% Jeffreys")
    ax.plot(xs, [r["churn"] if r["churn"] is not None else np.nan for r in rows], marker="o", label="churn")
    ax.axhline(0.10, color="black", lw=0.8, ls="--", label="EXP-0001's assumption")
    ax.set_xlabel("games")
    ax.set_ylabel("reference top-2 hits lost")
    ax.set_ylim(0, 1)
    ax.legend(fontsize=7)
    fig.tight_layout()
    fig.savefig(fig_dir / "churn-by-checkpoint.png", dpi=120, metadata={"Software": None})
    plt.close(fig)


if __name__ == "__main__":
    raise SystemExit(main())

"""EXP-0005: how much do the rankings move between two real algorithm versions?
(preregistration.md, issue #100)

One run of the real pipeline on EXP-0001's registered pilot players (rebuilt by EXP-0001's
frozen `pilot_suite`) and on the smoke suite, with EXP-0006's challenger side by side
(`ALGO_VARIANT=challenger`: the same games in a second account whose profiles are built under the
challenger's prior rule). The reference side is checked against EXP-0001's stored results
(between runs: it must not move at all); the two sides give the churn between versions, the
share of the reference's top-2 hits the challenger loses, with the player as the unit. Then
EXP-0001's own power simulation is rerun at the measured churn by the rule of section 9.

    python analysis.py --out DIR              # DIR/results.json and DIR/figures/
    python analysis.py --design               # the section 6 table only, no engine
"""

from __future__ import annotations

import argparse
import importlib.util
import json
import math
import sys
import tempfile
from pathlib import Path

import numpy as np
from scipy import stats

import rematch_lab as lab
from rematch_lab.experiments import Budget, seed, write_results

EXP = "EXP-0005"
SEED_NS = EXP
HERE = Path(__file__).resolve().parent
EXP1_DIR = HERE.parent / "EXP-0001"

VARIANT = "challenger"
ENGINES = 8
G = 80  # the checkpoint the suites are sized at (EXP-0001 section 9)
CHURN_ASSUMED = 0.10  # EXP-0001's registered churn
LEVEL = 0.95  # the decision interval (a measurement, TEMPLATE section 9)
SIMS = 4000  # simulated suites per cell, as EXP-0001
FULL_BUDGET_GAMES = 9000
FULL_CLEAN = 2
SMOKE_M = 3
SMOKE_GAMES = 1280
SMOKE_TOLERANCE = 0.10  # bench/baseline.json, derived by EXP-0001
# Compute budget (section 0): the two suites' games, and the simulated suites of the re-sizing.
BUDGET = {"games": 5120, "simulated_suites": 172_000}

SEED_NAMES = ["power/resize", "power/mde", "power/false_alarm", "bootstrap/churn"]


def r6(x):
    return None if x is None or (isinstance(x, float) and not math.isfinite(x)) else round(float(x), 6)


def load_exp1():
    spec = importlib.util.spec_from_file_location("exp1", EXP1_DIR / "analysis.py")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod  # its SEED_NS stays EXP-0001: pilot_suite() is the registered pilot


def jeffreys(x: int, n: int, level: float) -> list:
    """The Jeffreys interval of a share (Brown, Cai and DasGupta 2001), ends at 0 and 1 kept."""
    if n == 0:
        return [None, None]
    a = (1 - level) / 2
    lo = 0.0 if x == 0 else float(stats.beta.ppf(a, x + 0.5, n - x + 0.5))
    hi = 1.0 if x == n else float(stats.beta.ppf(1 - a, x + 0.5, n - x + 0.5))
    return [r6(lo), r6(hi)]


def classify(ci: list) -> str:
    lo, hi = ci
    if hi < CHURN_ASSUMED:
        return "below"
    if lo > CHURN_ASSUMED:
        return "above"
    return "straddles"


# ---- the run ---------------------------------------------------------------------------------


def side_rows(player: dict, checkpoints: list[dict], truth: float | None) -> list[dict]:
    flaw = player.get("flaw")
    target = flaw["target_key"] if flaw else None
    out = []
    for c in checkpoints:
        ranks = {r["key"]: r["rank"] for r in c["ranked"]}
        est = next((e for e in c["estimates"] if e["key"] == target), None) if target else None
        covered = None
        if est is not None and truth is not None:
            covered = bool(est["low"] <= truth <= est["high"])
        out.append(
            {
                "games": c["games"],
                "top": [r["key"] for r in c["ranked"][:3]],
                "rank1": c["ranked"][0]["key"] if c["ranked"] else None,
                "target_rank": ranks.get(target) if target else None,
                "hit2": (ranks.get(target, 99) <= 2) if target else None,
                "estimate": None if est is None else [r6(est["mean"]), r6(est["low"]), r6(est["high"])],
                "covered": covered,
            }
        )
    return out


def players_of(run: dict, suite: str) -> list[dict]:
    truth = {c["player"]: c.get("truth") for c in run["scorecard"]["cases"]}
    out = []
    for p in run["players"]:
        v = p["variant"]
        t = truth.get(p["name"])
        out.append(
            {
                "suite": suite,
                "name": p["name"],
                "flaw": p["flaw"]["id"] if p.get("flaw") else None,
                "target": p["flaw"]["target_key"] if p.get("flaw") else None,
                "truth": r6(t),
                "deterministic": [p["deterministic"], v["deterministic"]],
                "reference": side_rows(p, p["checkpoints"], t),
                "candidate": side_rows(p, v["checkpoints"], t),
                "perturbed_top2": [
                    [r["key"] for r in c["ranked"][:2]] if c else None
                    for c in (p.get("perturbed"), v.get("perturbed"))
                ],
            }
        )
    return out


def at(rows: list[dict], g: int) -> dict | None:
    return next((r for r in rows if r["games"] == g), None)


# ---- the analysis ----------------------------------------------------------------------------


def tables(players: list[dict], g: int) -> dict:
    """The paired 2 x 2 of top-2 hits at checkpoint g over the flawed players."""
    n11 = n10 = n01 = n00 = 0
    per_flaw: dict[str, dict[str, int]] = {}
    for p in players:
        if p["flaw"] is None:
            continue
        a, b = at(p["reference"], g), at(p["candidate"], g)
        if a is None or b is None:
            continue
        ref, cand = bool(a["hit2"]), bool(b["hit2"])
        cell = {(True, True): "n11", (False, True): "n10", (True, False): "n01", (False, False): "n00"}[(ref, cand)]
        f = per_flaw.setdefault(p["flaw"], {"n11": 0, "n10": 0, "n01": 0, "n00": 0})
        f[cell] += 1
        n11 += cell == "n11"
        n10 += cell == "n10"
        n01 += cell == "n01"
        n00 += cell == "n00"
    return {"n11": n11, "n10": n10, "n01": n01, "n00": n00, "per_flaw": per_flaw}


def churn_of(t: dict) -> dict:
    hits = t["n11"] + t["n01"]
    misses = t["n10"] + t["n00"]
    return {
        "reference_hits": hits,
        "lost": t["n01"],
        "churn": r6(t["n01"] / hits) if hits else None,
        "churn_ci95_jeffreys": jeffreys(t["n01"], hits, 0.95),
        "churn_ci80_jeffreys": jeffreys(t["n01"], hits, 0.80),
        "reference_misses": misses,
        "gained": t["n10"],
        "gain": r6(t["n10"] / misses) if misses else None,
        "gain_ci95_jeffreys": jeffreys(t["n10"], misses, 0.95),
    }


def design_power() -> list[dict]:
    """Section 6: the probability of each classification for a true churn b with n reference
    hits, exactly from the binomial (a Jeffreys 95% interval per outcome)."""
    rows = []
    for n in (19, 28, 40):
        cls = {x: classify(jeffreys(x, n, LEVEL)) for x in range(n + 1)}
        for b in (0.0, 0.02, 0.05, 0.10, 0.20, 0.30):
            pmf = stats.binom.pmf(range(n + 1), n, b)
            p = {k: float(sum(pmf[x] for x in range(n + 1) if cls[x] == k)) for k in ("below", "straddles", "above")}
            rows.append({"hits": n, "churn": b, **{k: r6(v) for k, v in p.items()}})
    return rows


def resize(exp1, ref_rows: list[dict], churn: float, budget: Budget) -> dict:
    """EXP-0001's section 9 rule at `churn`, with EXP-0001's own simulation (its frozen code)."""
    rng = np.random.default_rng(seed(SEED_NS, "power/resize"))
    curves = {}
    for g in exp1.FULL_GAMES_OPTIONS:
        hits = exp1.hits_at(ref_rows, g)
        curve = []
        for m in exp1.M_GRID:
            budget.spend("simulated_suites", SIMS)
            s = exp1.simulate_recall(rng, hits, m, exp1.DELTA, churn, SIMS)
            curve.append({"players_per_flaw": m, "ship": r6(s["ship"]), "infeasible": r6(s["infeasible"])})
        curves[g] = curve
    options = []
    for g, curve in curves.items():
        m = exp1.required_m(curve)
        if m is not None:
            games = 5 * m * g + FULL_CLEAN * g
            options.append({"games": g, "players_per_flaw": m, "total_games": games, "fits": games <= FULL_BUDGET_GAMES})
    fitting = sorted((o for o in options if o["fits"]), key=lambda o: (o["total_games"], -o["games"]))
    if fitting:
        choice = {"outcome": "resize", **fitting[0]}
    else:
        best = {g: max(m for m in range(1, 200) if 5 * m * g + FULL_CLEAN * g <= FULL_BUDGET_GAMES) for g in curves}
        choice = {"outcome": "budget-bound", "players_per_flaw_at_budget": best}
    return {"churn": r6(churn), "curves": {str(g): c for g, c in curves.items()}, "required": options, "choice": choice}


def mde_at(exp1, ref_rows: list[dict], churn: float, m: int, g: int, budget: Budget) -> dict:
    """The smallest change in EXP-0001's grid shipped 80% of the time by a suite of m a flaw."""
    rng = np.random.default_rng(seed(SEED_NS, "power/mde"))
    hits = exp1.hits_at(ref_rows, g)
    curve = []
    for d in exp1.DELTA_GRID:
        budget.spend("simulated_suites", SIMS)
        s = exp1.simulate_recall(rng, hits, m, d, churn, SIMS)
        curve.append({"delta": d, "ship": r6(s["ship"])})
    mde = next((c["delta"] for c in curve if c["ship"] >= exp1.TARGET_POWER), None)
    return {"churn": r6(churn), "players_per_flaw": m, "games": g, "curve": curve, "mde": mde}


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--out")
    ap.add_argument("--design", action="store_true", help="print the section 6 table and stop")
    ap.add_argument("--algorithm", default=None)
    args = ap.parse_args()
    if args.design:
        print(json.dumps(design_power(), indent=1))
        return 0
    if not args.out:
        ap.error("--out is required")
    out_dir = Path(args.out)
    (out_dir / "figures").mkdir(parents=True, exist_ok=True)
    if args.algorithm is not None and lab.algorithm()["id"] != args.algorithm:
        raise SystemExit(f"the cli is not the requested {args.algorithm}")

    budget = Budget(BUDGET)
    exp1 = load_exp1()
    ref = json.loads((EXP1_DIR / "results.json").read_text(encoding="utf-8"))
    pilot = exp1.pilot_suite()
    if pilot != ref["pilot"]["suite"]:
        raise SystemExit("the pilot suite differs from EXP-0001's")

    with tempfile.TemporaryDirectory() as tmp:
        suite_path = Path(tmp) / "suite.json"
        suite_path.write_text(json.dumps(pilot), encoding="utf-8")
        budget.spend("games", sum(p["games"] for p in pilot["players"]))
        run_pilot = lab.bench(suite_file=suite_path, out_dir=Path(tmp) / "pilot", engines=ENGINES, variant=VARIANT)
        budget.spend("games", SMOKE_GAMES)
        run_smoke = lab.bench("smoke", out_dir=Path(tmp) / "smoke", engines=ENGINES, variant=VARIANT)
    if sum(p["games"] for p in run_smoke["suite"]["players"]) != SMOKE_GAMES:
        raise SystemExit("the smoke suite is not EXP-0001's 1,280 games")
    algorithm_id = lab.algorithm()["id"]
    players = players_of(run_pilot, "pilot") + players_of(run_smoke, "smoke")

    # Between runs: the reference side against EXP-0001's stored pilot (the smoke scorecard is
    # compared with bench/baseline.json in decision.md, since the baseline moves when a version
    # ships). Determinism of each side is the CoachingInput built twice.
    stored = {p["name"]: p for p in ref["pilot"]["players"]}
    fields = ("rank1", "target_rank", "hit2", "estimate", "covered")
    mismatches = []
    for p in (q for q in players if q["suite"] == "pilot"):
        s = stored[p["name"]]
        for mine, theirs in zip(p["reference"], s["checkpoints"], strict=True):
            for f in fields:
                if mine[f] != theirs[f]:
                    mismatches.append({"player": p["name"], "games": mine["games"], "field": f, "now": mine[f], "stored": theirs[f]})
    smoke_metrics = run_smoke["scorecard"]["metrics"]
    between_runs = {
        "pilot_mismatches": mismatches,
        "pilot_fields_compared": len(fields) * sum(len(p["reference"]) for p in players if p["suite"] == "pilot"),
        "deterministic": all(all(p["deterministic"]) for p in players),
    }

    # Between versions. Primary: the churn at 80 games over every flawed player of both suites.
    primary_t = tables(players, G)
    primary = {**churn_of(primary_t), "games": G, "players": sum(primary_t[k] for k in ("n11", "n10", "n01", "n00")), "table": primary_t}
    primary["classification"] = classify(primary["churn_ci95_jeffreys"])
    by_suite = {s: churn_of(tables([p for p in players if p["suite"] == s], G)) for s in ("pilot", "smoke")}
    by_checkpoint = []
    for g in sorted({c["games"] for p in players for c in p["reference"]}):
        t = tables(players, g)
        by_checkpoint.append({"games": g, **churn_of(t), "table": {k: t[k] for k in ("n11", "n10", "n01", "n00")}})
    # Churn pooled over the checkpoints, the player as the unit: a cluster bootstrap of the ratio.
    per_player = []
    for p in players:
        if p["flaw"] is None:
            continue
        pairs = [(a["hit2"], b["hit2"]) for a, b in zip(p["reference"], p["candidate"], strict=True)]
        per_player.append((sum(bool(a) for a, _ in pairs), sum(bool(a) and not b for a, b in pairs)))
    arr = np.array(per_player, dtype=float)
    rng = np.random.default_rng(seed(SEED_NS, "bootstrap/churn"))
    boots = []
    for _ in range(2000):
        s = arr[rng.integers(0, len(arr), len(arr))]
        boots.append(s[:, 1].sum() / s[:, 0].sum() if s[:, 0].sum() else 0.0)
    pooled = {
        "reference_hit_checkpoints": int(arr[:, 0].sum()),
        "lost": int(arr[:, 1].sum()),
        "churn": r6(arr[:, 1].sum() / arr[:, 0].sum()),
        "ci95_cluster_bootstrap": [r6(float(np.quantile(boots, 0.025))), r6(float(np.quantile(boots, 0.975)))],
    }

    # Ranking churn among every player and checkpoint (clean controls included).
    rank_churn = []
    for s in ("pilot", "smoke"):
        for g in sorted({c["games"] for p in players if p["suite"] == s for c in p["reference"]}):
            pairs = [(at(p["reference"], g), at(p["candidate"], g)) for p in players if p["suite"] == s]
            pairs = [(a, b) for a, b in pairs if a and b]
            rank_churn.append(
                {
                    "suite": s,
                    "games": g,
                    "players": len(pairs),
                    "top2_set_changed": sum(set(a["top"][:2]) != set(b["top"][:2]) for a, b in pairs),
                    "top2_order_changed": sum(a["top"][:2] != b["top"][:2] for a, b in pairs),
                    "rank1_changed": sum(a["rank1"] != b["rank1"] for a, b in pairs),
                }
            )
    clean = [
        {
            "suite": p["suite"],
            "name": p["name"],
            "games": a["games"],
            "reference_rank1": a["rank1"],
            "candidate_rank1": b["rank1"],
        }
        for p in players
        if p["flaw"] is None
        for a, b in zip(p["reference"], p["candidate"], strict=True)
    ]

    # Section 9: the decision, and EXP-0001's sizing at the measured churn.
    ref_rows = ref["pilot"]["players"]
    lo, hi = primary["churn_ci95_jeffreys"]
    cls = primary["classification"]
    if cls == "below":
        sizing = {"rule": "resize at the upper end of the 95% interval", **resize(exp1, ref_rows, hi, budget)}
    elif cls == "above":
        sizing = {"rule": "re-register the weekly MDE at the lower end", **mde_at(exp1, ref_rows, lo, 22, G, budget)}
    else:
        sizing = {"rule": "the suites stay; the MDE at the point estimate is reported", **mde_at(exp1, ref_rows, primary["churn"], 22, G, budget)}
    fa_rng = np.random.default_rng(seed(SEED_NS, "power/false_alarm"))
    false_alarm = []
    for b in sorted({r6(primary["churn"]), r6(hi), CHURN_ASSUMED}):
        budget.spend("simulated_suites", SIMS)
        s = exp1.simulate_recall(fa_rng, exp1.hits_at(ref_rows, G), SMOKE_M, 0.0, b, SIMS, tolerance=SMOKE_TOLERANCE)
        false_alarm.append({"churn": b, "smoke_ci_rule_false_alarm": r6(s["ci_flagged"])})

    results = {
        "experiment": EXP,
        "algorithm_id": algorithm_id,
        "variant": VARIANT,
        "design": {
            "suites": {"pilot": pilot["name"], "smoke": run_smoke["suite"]["name"]},
            "checkpoint": G,
            "assumed_churn": CHURN_ASSUMED,
            "seed_names": SEED_NAMES,
            "power": design_power(),
        },
        "between_runs": between_runs,
        "primary": primary,
        "by_suite": by_suite,
        "by_checkpoint": by_checkpoint,
        "pooled_checkpoints": pooled,
        "rank_churn": rank_churn,
        "clean": clean,
        "coverage": {
            side: {
                "covered": sum(bool(c["covered"]) for p in players for c in p[side] if c["covered"] is not None and c["games"] <= G),
                "intervals": sum(c["covered"] is not None for p in players for c in p[side] if c["games"] <= G),
            }
            for side in ("reference", "candidate")
        },
        "scorecards": {
            "pilot": {"reference": run_pilot["scorecard"]["metrics"], "candidate": run_pilot["variant"]["scorecard"]["metrics"]},
            "smoke": {"reference": smoke_metrics, "candidate": run_smoke["variant"]["scorecard"]["metrics"]},
        },
        "sizing": sizing,
        "false_alarm": false_alarm,
        "players": players,
        "compute": budget.record(),
    }
    write_results(out_dir, results)
    figures(out_dir / "figures", results)
    return 0


def figures(fig_dir: Path, res: dict) -> None:
    import matplotlib

    matplotlib.use("Agg")
    import matplotlib.pyplot as plt

    rows = res["by_checkpoint"]
    fig, ax = plt.subplots(figsize=(5, 3.4))
    xs = [r["games"] for r in rows]
    ys = [r["churn"] if r["churn"] is not None else np.nan for r in rows]
    lo = [r["churn_ci95_jeffreys"][0] for r in rows]
    hi = [r["churn_ci95_jeffreys"][1] for r in rows]
    ax.fill_between(xs, lo, hi, color="grey", alpha=0.25, label="95% Jeffreys")
    ax.plot(xs, ys, marker="o", label="churn")
    ax.axhline(CHURN_ASSUMED, color="black", lw=0.8, ls="--", label="EXP-0001's assumption")
    ax.set_xlabel("games")
    ax.set_ylabel("reference top-2 hits lost")
    ax.set_ylim(0, 1)
    ax.legend(fontsize=7)
    fig.tight_layout()
    fig.savefig(fig_dir / "churn-by-checkpoint.png", dpi=120, metadata={"Software": None})
    plt.close(fig)


if __name__ == "__main__":
    sys.exit(main())

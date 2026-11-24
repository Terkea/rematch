"""EXP-0006 development runs: the candidate prior rules on development seeds only.

Not the registered analysis. It draws EXP-0002's players (its frozen generator, imported from
`../../EXP-0002/analysis.py`) on the seed namespace `EXP-0006-dev`, which no confirmatory run
uses, at 100 players a cell on the two-games-a-day schedule, and scores each rule of the grid
in `README.md` by the selection rule written there before the first run.

    python dev.py --out DIR    # DIR/dev-results.json
"""

from __future__ import annotations

import argparse
import importlib.util
import json
import math
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

import numpy as np

import rematch_lab as lab
from rematch_lab.models import ProfileRequest

HERE = Path(__file__).resolve().parent
SEED_NS = "EXP-0006-dev"
PLAYERS = 100
WORKERS = 6
GRID = [
    "champion",
    *[
        f"dev:{','.join(o for o in (g, c) if o)}"
        for c in ("", "ess", "ess,disp")
        for g in ("games=8", "games=4", "games=2", "games=1", "")
        if g or c
    ],
]
BAND = (0.75, 0.85)


def load_exp2():
    spec = importlib.util.spec_from_file_location("exp2", HERE.parent.parent / "EXP-0002" / "analysis.py")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    mod.SEED_NS = SEED_NS
    mod.PLAYERS_PER_CELL = PLAYERS
    return mod


def rows_for(exp2, variant: str) -> list[dict]:
    jobs = []
    for band, rho, disp in exp2.CELLS:
        data = exp2.simulate_cell(band, rho, disp)
        batch = []
        for i in range(PLAYERS):
            for g in exp2.CHECKPOINTS:
                pid = f"{band}-{rho}-{disp}-{i:03d}-{g}"
                batch.append((exp2.facts(band, pid, data, i, g, "spread", rho), band, rho, disp, i, g))
        jobs += [(batch[j : j + 200], list(data)) for j in range(0, len(batch), 200)]

    def run(job):
        batch, keys = job
        resp = lab.profile(ProfileRequest(players=[b[0] for b in batch]), variant=variant)
        out = []
        for (f, band, rho, disp, i, g), p in zip(batch, resp.players, strict=True):
            assert p.id == f.id
            by = {e.key: e for e in p.estimates}
            for key in keys:
                e = by[key]
                theta = exp2.theta_of(key, band, rho)
                out.append(
                    {
                        "band": band, "rho": rho, "disp": disp, "player": i, "games": g,
                        "key": key, "covered": e.posterior.interval.low <= theta <= e.posterior.interval.high,
                        "credible": e.credible, "high_miss": e.posterior.interval.high < theta,
                    }
                )
        return out

    with ThreadPoolExecutor(WORKERS) as ex:
        return [r for part in ex.map(run, jobs) for r in part]


def coverage(rows, pred) -> float:
    cells: dict[tuple, list[bool]] = {}
    for r in rows:
        if pred(r):
            cells.setdefault((r["band"], r["rho"], r["disp"]), []).append(r["covered"])
    return float(np.mean([np.mean(v) for v in cells.values()]))


def score(exp2, rows) -> dict:
    prim = {
        f"{k}@{g}": coverage(rows, lambda r, k=k, g=g: r["rho"] in exp2.PRIMARY_RHOS and r["key"] == k and r["games"] == g)
        for k in exp2.KEYS
        for g in exp2.CHECKPOINTS
    }
    worst = max(prim, key=lambda c: abs(prim[c] - 0.8))
    rho1 = coverage(rows, lambda r: r["rho"] == 1.0)
    low = [r for r in rows if r["rho"] in (0.5, 1.0)]
    return {
        "primary": {c: round(v, 4) for c, v in prim.items()},
        "worst_cell": worst,
        "D": round(abs(prim[worst] - 0.8), 4),
        "inside_band": sum(BAND[0] <= v <= BAND[1] for v in prim.values()),
        "rho1_coverage": round(rho1, 4),
        "rho1_deviation": round(abs(rho1 - 0.8), 4),
        "false_credible": round(sum(r["credible"] for r in low) / len(low), 5),
    }


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", required=True)
    ap.add_argument("--only", nargs="*")
    args = ap.parse_args()
    exp2 = load_exp2()
    out = {"seed_namespace": SEED_NS, "players_per_cell": PLAYERS, "variants": {}}
    for v in args.only or GRID:
        out["variants"][v] = score(exp2, rows_for(exp2, v))
        print(v, json.dumps({k: x for k, x in out["variants"][v].items() if k != "primary"}), flush=True)
    champ = out["variants"].get("champion")
    if champ:
        ok = {
            v: s
            for v, s in out["variants"].items()
            if s["rho1_deviation"] <= champ["rho1_deviation"] + 0.05
            and s["false_credible"] <= champ["false_credible"] + 0.01
        }
        best = min(s["D"] for s in ok.values())
        tied = [v for v, s in ok.items() if s["D"] <= best + 0.01]
        out["eligible"] = sorted(ok)
        out["tied_within_0_01"] = tied
    Path(args.out).mkdir(parents=True, exist_ok=True)
    (Path(args.out) / "dev-results.json").write_text(json.dumps(out, indent=2, sort_keys=True) + "\n")
    return 0 if all(math.isfinite(s["D"]) for s in out["variants"].values()) else 1


if __name__ == "__main__":
    raise SystemExit(main())

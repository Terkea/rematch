"""EXP-0006 section 6: the power of the whole section 9 rule, before the run (review concern 2).

A power pilot of the registered profile design on its own seed namespace, `EXP-0006-power`
(disjoint from the development seeds and the confirmatory ones), at the registered sizes. Its
bootstrap replicates stand for the sampling distribution of every profile statistic the rule
reads; in each replicate the rule is applied with the interval half-widths the registered sizes
give (the replicates' standard deviations), so the share of replicates that ship, reject or need
more data is the rule's power at the pilot's values. The Tier 1 guards are simulated from the
binomial at no true change (section 6 says why).

    python power.py --out DIR    # DIR/power.json
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path

import numpy as np
from scipy import stats

import analysis as a
from rematch_lab.experiments import seed

POWER_NS = "EXP-0006-power"


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", required=True)
    args = ap.parse_args()
    exp1 = a.load("exp1", "EXP-0001/analysis.py")
    exp2 = a.load_exp2(POWER_NS, a.PRIMARY_PLAYERS)
    _, data, plan = a.run_profiles(POWER_NS, None)
    idx = a.resamples(plan, np.random.default_rng(seed(POWER_NS, "bootstrap/paired")))
    prof = a.profile_statistics(exp2, data, plan, idx)

    keys, gs = list(exp2.KEYS), exp2.CHECKPOINTS
    all_kg = [(k, g) for k in keys for g in gs]
    rho1, low = a.cells_of(plan, [1.0]), a.cells_of(plan, [0.5, 1.0])
    d_ch = prof["per_rule"][a.VARIANT]["_boot"]
    delta = prof["per_rule"]["champion"]["_boot"] - d_ch
    r1 = {v: a.stat(data[v]["covered"], all_kg, rho1, idx) for v in a.VARIANTS}
    g1 = np.abs(r1[a.VARIANT] - a.NOMINAL) - np.abs(r1["champion"] - a.NOMINAL)
    fc = a.stat(data[a.VARIANT]["credible"], all_kg, low, idx) - a.stat(data["champion"]["credible"], all_kg, low, idx)
    z975, z80 = stats.norm.ppf(0.5 + a.SHIP_LEVEL / 2), stats.norm.ppf(0.5 + a.GUARD_LEVEL / 2)
    sd = {k: float(np.std(v, ddof=1)) for k, v in {"D": d_ch, "delta": delta, "rho1": g1, "fc": fc}.items()}
    ship_prof = (
        (d_ch + z975 * sd["D"] <= a.TOLERANCE)
        & (delta - z975 * sd["delta"] > 0)
        & (g1 + z80 * sd["rho1"] <= a.MARGINS["rho1_deviation"])
        & (fc + z80 * sd["fc"] <= a.MARGINS["false_credible"])
    )
    reject_prof = (d_ch - z975 * sd["D"] > a.TOLERANCE) | (g1 - z80 * sd["rho1"] > a.MARGINS["rho1_deviation"]) | (
        fc - z80 * sd["fc"] > a.MARGINS["false_credible"]
    )

    # Tier 1 at no true change: 45 flawed players with EXP-0001's hit rate at 80 games (0.63) and
    # 23 clean players with a rank-1 alarm rate of 0.2 (EXP-0001: 2 of 10 clean checkpoints),
    # each side discordant from the other with probability 0.05 in each direction.
    rng = np.random.default_rng(seed(POWER_NS, "tier1"))
    sims = 4000

    def guard(n: int, base: float, worse_is_down: bool, margin: float) -> tuple[float, float]:
        ref = rng.random((sims, n)) < base
        flip = rng.random((sims, n)) < 0.05
        cand = np.where(flip, ~ref, ref)
        n11 = (ref & cand).sum(1)
        n10 = (~ref & cand).sum(1)
        n01 = (ref & ~cand).sum(1)
        n00 = (~ref & ~cand).sum(1)
        _, lo, hi = exp1.newcombe_paired(n11, n10, n01, n00, exp1.z_of(a.GUARD_LEVEL))
        if worse_is_down:
            return float((lo >= -margin).mean()), float((hi < -margin).mean())
        return float((hi <= margin).mean()), float((lo > margin).mean())

    rec_hold, rec_beyond = guard(45, 0.63, True, a.MARGINS["recall_at_2"])
    cl_hold, cl_beyond = guard(23, 0.2, False, a.MARGINS["clean_alarm"])
    out = {
        "namespace": POWER_NS,
        "pilot": {
            "D_champion": prof["per_rule"]["champion"]["D"],
            "D_challenger": prof["primary"]["D"],
            "worst_challenger": {k: prof["per_rule"][a.VARIANT]["worst"][k] for k in ("key", "games", "coverage")},
            "delta_D": prof["delta_D"]["delta_D"],
            "rho1_worse_by": prof["rho1_deviation"]["worse_by"],
            "false_credible_worse_by": prof["false_credible"]["worse_by"],
            "sd": {k: round(v, 6) for k, v in sd.items()},
        },
        "profile_rule": {"ship": round(float(ship_prof.mean()), 4), "reject": round(float(reject_prof.mean()), 4)},
        "tier1_no_change": {
            "recall_holds": round(rec_hold, 4), "recall_beyond": round(rec_beyond, 4),
            "clean_alarm_holds": round(cl_hold, 4), "clean_alarm_beyond": round(cl_beyond, 4),
        },
        "ship_power_approx": round(float(ship_prof.mean()) * rec_hold * cl_hold, 4),
    }
    Path(args.out).mkdir(parents=True, exist_ok=True)
    (Path(args.out) / "power.json").write_text(json.dumps(out, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(json.dumps(out, indent=1))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

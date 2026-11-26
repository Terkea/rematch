"""EXP-0030's power analysis (preregistration.md, section 6): the section 9 rule simulated on
counts, for the number of settled players N read at the three lengths.

A candidate's six co-primary cells (told "falling" and told "rising", at 60, 120 and 250 rated
games) must each have the upper end of a Wilson interval at 1 - 0.05/12 at most 0.15 (two
candidates x six cells). At each length the falling and rising counts of the same players are
one multinomial draw (a player cannot be told both); the three lengths are drawn independently,
though they are the same players read three times and so positively correlated, which makes
"every cell within" more likely than simulated: the simulation is the harder case. The guards
are sized the same way: detection at a 200-point fall (300 players a length, the lower end of the
80% Wilson interval at least 0.50), the breaks-and-streaks world (N_ROUGH players, a cell above
the bound when the lower end at 1 - 0.05/12 exceeds 0.15). No algorithm is called.

    python power.py > power.json
"""

from __future__ import annotations

import json

import numpy as np
from scipy import stats

BOUND, FAMILY, SIMS = 0.15, 12, 20_000
GRID_N = [700, 1000, 1500, 2000, 2500]
GRID_P = [0.08, 0.10, 0.11, 0.12, 0.13, 0.14, 0.15, 0.16]
DETECT_N = 300
GRID_D = [0.45, 0.50, 0.55, 0.60, 0.65, 0.70]
N_ROUGH = 1000
GRID_R = [0.10, 0.15, 0.18, 0.20, 0.25]


def wilson(k: np.ndarray, n: int, level: float) -> tuple[np.ndarray, np.ndarray]:
    z = stats.norm.ppf(1 - (1 - level) / 2)
    p = k / n
    den = 1 + z * z / n
    c = (p + z * z / (2 * n)) / den
    h = z * np.sqrt(p * (1 - p) / n + z * z / (4 * n * n)) / den
    return c - h, c + h


def main() -> None:
    rng = np.random.default_rng(int.from_bytes(b"EXP-0030/power", "big") % 2**32)
    level = 1 - 0.05 / FAMILY
    primary = []
    for n in GRID_N:
        for p in GRID_P:
            within = np.ones(SIMS, dtype=bool)
            for _ in range(3):
                k = rng.multinomial(n, [p, p, 1 - 2 * p], size=SIMS)
                for side in (0, 1):
                    _, hi = wilson(k[:, side], n, level)
                    within &= hi <= BOUND
            primary.append({"players": n, "true_rate_each_cell": p, "all_six_within": round(float(within.mean()), 4)})
    detection = []
    for d in GRID_D:
        ok = np.ones(SIMS, dtype=bool)
        for _ in range(3):
            lo, _ = wilson(rng.binomial(DETECT_N, d, SIMS), DETECT_N, 0.80)
            ok &= lo >= 0.50
        detection.append({"players_a_length": DETECT_N, "true_detection_each_length": d, "guard_passes": round(float(ok.mean()), 4)})
    rough = []
    for r in GRID_R:
        trip = np.zeros(SIMS, dtype=bool)
        lo, _ = wilson(rng.binomial(N_ROUGH, r, SIMS), N_ROUGH, level)
        trip |= lo > BOUND
        rough.append({"players": N_ROUGH, "true_rate_one_cell": r, "guard_trips": round(float(trip.mean()), 4)})
    out = {
        "rule": "Wilson at 1 - 0.05/12 per cell; a candidate's primary clears when all six upper ends "
        "are at most 0.15",
        "sims": SIMS,
        "primary": primary,
        "detection_guard": detection,
        "rough_guard_one_cell": rough,
    }
    print(json.dumps(out, indent=2))


if __name__ == "__main__":
    main()

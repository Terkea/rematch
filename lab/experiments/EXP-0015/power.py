"""EXP-0015's power analysis (preregistration.md, section 6): the section 9 rule simulated on
binomial counts, for the number of settled players N read at the three checkpoints.

The three cells are the same players read at 60, 120 and 250 games, so they are correlated; the
simulation draws them independently, which is the harder case for "keep" (all three must clear)
and the same for the other reads, which need one cell. No algorithm is called: this sizes the
count, it does not estimate anything.

    python power.py > power.json
"""

from __future__ import annotations

import json
import math

import numpy as np
from scipy import stats

BOUND, HIDE, FAMILY, SIMS = 0.15, 0.20, 3, 20_000
GRID_N = [200, 300, 400, 500, 600, 700, 800]
GRID_P = [0.05, 0.10, 0.12, 0.13, 0.14, 0.20, 0.25, 0.30, 0.35, 0.40, 0.45, 0.55, 0.65, 0.75, 0.85]


def wilson(k: np.ndarray, n: int, level: float) -> tuple[np.ndarray, np.ndarray]:
    z = stats.norm.ppf(1 - (1 - level) / 2)
    p = k / n
    den = 1 + z * z / n
    c = (p + z * z / (2 * n)) / den
    h = z * np.sqrt(p * (1 - p) / n + z * z / (4 * n * n)) / den
    return c - h, c + h


def main() -> None:
    rng = np.random.default_rng(int.from_bytes(b"EXP-0015/power", "big") % 2**32)
    level = 1 - 0.05 / FAMILY
    rows = []
    for n in GRID_N:
        for p in GRID_P:
            k = rng.binomial(n, p, size=(SIMS, FAMILY))
            lo, hi = wilson(k, n, level)
            keep = np.all(hi <= BOUND, axis=1)
            hide = np.any(lo > HIDE, axis=1)
            change = np.any(lo > BOUND, axis=1) & ~hide
            rows.append(
                {
                    "players": n,
                    "true_rate_all_cells": p,
                    "keep": round(float(keep.mean()), 4),
                    "change": round(float(change.mean()), 4),
                    "hide": round(float(hide.mean()), 4),
                }
            )
    # One cell off: the true rate p in one cell, 0.10 in the other two (a problem at one n).
    single = []
    for n in GRID_N:
        for p in (0.20, 0.30, 0.40):
            k = np.column_stack(
                [rng.binomial(n, p, SIMS), rng.binomial(n, 0.10, (SIMS, FAMILY - 1))]
            )
            lo, _ = wilson(k, n, level)
            single.append(
                {
                    "players": n,
                    "one_cell_rate": p,
                    "change_or_hide": round(float(np.any(lo > BOUND, axis=1).mean()), 4),
                    "hide": round(float(np.any(lo > HIDE, axis=1).mean()), 4),
                }
            )
    out = {
        "rule": "Wilson at 1 - 0.05/3 per cell; keep: every upper <= 0.15; change: a lower > "
        "0.15; hide: a lower > 0.20",
        "sims": SIMS,
        "grid": rows,
        "one_cell": single,
        "half_width_at_0_10": {
            str(n): round(float((wilson(np.array(0.1 * n), n, level)[1]
                                 - wilson(np.array(0.1 * n), n, level)[0]) / 2), 4)
            for n in GRID_N
        },
    }
    assert all(math.isfinite(r["keep"]) for r in rows)
    print(json.dumps(out, indent=2))


if __name__ == "__main__":
    main()

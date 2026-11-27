"""EXP-0029 development data: activity measured on EXP-0009's own cohort, never on a month of
EXP-0029 (section 0 and section 6). It reads only EXP-0009's players files, which EXP-0009
already read and whose checksums are in its inputs.sha256, and writes exp09_activity.json:

    python -I dev/exp09_activity.py DATA_DIR/exp09-cohort > dev/exp09_activity.json

1. The coordinator's pre-check: EXP-0009's eligibility chain on its confirmatory hash slice
   [0, 0.3) (July rapid games >= 20; August and September analysed games >= 10) and October's
   rapid games among those with 5 or more.
2. g0, the October rapid games of EXP-0009's players with an outcome (the chain, an August end
   rating below 2000 and 5 or more rapid games in October): the denominator of the memory ratio
   (N1). Its distribution, and E[phi^g0 - 1] at phi 0.97 and 0.98.
3. A proxy for EXP-0029's April (N5): the same players' October games standing in for April and
   for each of May and June, E[phi^g (phi^(2g) - 1)].
4. A proxy for one three-month feature window (N3, S4): August + September + October analysed
   games of players with 20 rapid games in July and 30 analysed games in the three months; for
   each per-window cap (read, kept) E[min(m, read)] and E[1 / min(m, kept)]; and EXP-0009's own
   E[1 / min(m, 40)] over August and September of its chain.
Nothing here is random.
"""

from __future__ import annotations

import json
import statistics
import sys
from pathlib import Path

CAPS = ((144, 120), (96, 80), (72, 60))
PHI = (0.97, 0.98)


def frac(h: str) -> float:
    return int(h[:16], 16) / 2**64


def load(path: Path, keep) -> dict[str, dict]:
    out = {}
    with path.open(encoding="utf-8") as fh:
        for line in fh:
            p = json.loads(line)
            if keep(p["h"], p):
                out[p["h"]] = p
    return out


def r_end(p: dict) -> float:
    return statistics.fmean(g[3] for g in p["last"])


def quantiles(xs: list[int]) -> dict[str, float]:
    s = sorted(xs)
    q = lambda x: s[int(x * (len(s) - 1))]  # noqa: E731
    return {"n": len(s), "p10": q(0.1), "p25": q(0.25), "median": statistics.median(s),
            "p75": q(0.75), "mean": statistics.fmean(s),
            **{f"share_below_{t}": sum(x < t for x in s) / len(s) for t in (5, 10, 20, 25, 40)}}


def main() -> None:
    root = Path(sys.argv[1])
    jul = load(root / "2026-07/players.jsonl", lambda h, p: p["rapid_games"] >= 20 and frac(h) < 0.3)
    aug = load(root / "2026-08/players.jsonl", lambda h, p: h in jul)
    sep = load(root / "2026-09/players.jsonl", lambda h, p: h in jul)
    octo = load(root / "2026-10/players.jsonl", lambda h, p: h in jul)
    chain = [h for h in jul if aug.get(h, {}).get("eval_games", 0) >= 10
             and sep.get(h, {}).get("eval_games", 0) >= 10]
    pre = [octo[h]["rapid_games"] for h in chain if h in octo and octo[h]["rapid_games"] >= 5]
    outcome = [h for h in chain if r_end(aug[h]) < 2000 and h in octo
               and octo[h]["rapid_games"] >= 5]
    g0 = [octo[h]["rapid_games"] for h in outcome]
    out = {
        "precheck": {"july_ge20": len(jul), "chain": len(chain), "oct_ge5": len(pre),
                     "october_rapid": quantiles(pre)},
        "g0": {"players": len(outcome), "october_rapid": quantiles(g0),
               **{f"E_share_exp0009_phi{phi:g}": statistics.fmean(phi ** g - 1 for g in g0)
                  for phi in PHI},
               **{f"E_share_A_proxy_phi{phi:g}":
                  statistics.fmean(phi ** g * (phi ** (2 * g) - 1) for g in g0) for phi in PHI}},
    }
    m0 = [min(p[h]["eval_games"], 40) for h in chain for p in (aug, sep)]
    window = [aug.get(h, {}).get("eval_games", 0) + sep.get(h, {}).get("eval_games", 0)
              + octo.get(h, {}).get("eval_games", 0) for h in jul]
    window = [m for m in window if m >= 30]
    out["window_proxy"] = {
        "players_per_unit_hash": len(window) / 0.3,
        "exp0009_players_per_unit_hash": len(chain) / 0.3,
        "E_inv_m_exp0009_cap40": statistics.fmean(1 / m for m in m0),
        "mean_m_exp0009_cap48": statistics.fmean(
            min(p[h]["eval_games"], 48) for h in chain for p in (aug, sep)),
        "analysed": quantiles(window),
        "caps": {f"{r}/{k}": {"E_min_read": statistics.fmean(min(m, r) for m in window),
                              "E_inv_min_kept": statistics.fmean(1 / min(m, k) for m in window)}
                 for r, k in CAPS},
    }
    json.dump(out, sys.stdout, indent=2, sort_keys=True)
    print()


if __name__ == "__main__":
    main()

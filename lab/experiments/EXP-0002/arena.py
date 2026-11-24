"""EXP-0002's champion/challenger rule (rematch_lab.arena, ADR 0109).

EXP-0002 is a measurement of one version: each of the 20 key × checkpoint coverages of the 80%
range is classified by its Bonferroni interval against [0.75, 0.85] (preregistration.md, section
9). It registered no comparison of two versions. The comparison here is the one drafted in the
EXP-0006 issue (primary: the worst of the 20 coverages; guarded: the coverage at the peers'
rate), adopted by ADR 0109 for the arena and not pre-registered; EXP-0006's own registration
will supersede it for that experiment's decision.

- worst coverage (primary): the largest distance from 0.8 among the 20 cells. Its interval is
  the interval of the maximum, [max of the cells' lower bounds, max of their upper bounds], from
  the simultaneous Bonferroni intervals (family of 20), so a challenger whose worst cell moves is
  still compared fairly. The change is the MOVER interval of the two versions' maxima. Better
  when it lies wholly below 0, worse when wholly above.
- coverage at ρ = 1 (guard): the same, from the ρ margin's Bonferroni interval (family of 21).

The intervals are unpaired: results.json keeps the coverages, not each player's flags, so the
positive correlation of two versions on the same seeds is ignored. That errs towards "no clear
difference", never towards a verdict.
"""

from __future__ import annotations

from typing import Any

from rematch_lab import stats

TARGET = 0.8

METRICS = [
    {
        "name": "worst_coverage",
        "role": "primary",
        "label": "Worst of the 20 coverages of the 80% range",
        "better": "closer to 0.8",
    },
    {
        "name": "rho1_coverage",
        "role": "guard",
        "label": "Coverage at the peers' rate (ρ = 1)",
        "better": "closer to 0.8",
    },
]


def _r(x: float) -> float:
    return round(x, 6)


def measure(results: dict[str, Any]) -> dict[str, Any]:
    cells = [
        {
            "key": c["key"],
            "games": c["games"],
            "coverage": c["coverage"],
            "interval": c["ci_bonferroni"],
        }
        for c in results["primary"]["cells"]
    ]
    rho1 = next(m for m in results["margins"] if m["factor"] == "rho" and m["level"] == 1.0)
    return {
        "cells": sorted(cells, key=lambda c: (c["key"], c["games"])),
        "rho1": {"coverage": rho1["coverage"], "interval": rho1["ci_bonferroni"]},
    }


def _worst(m: dict[str, Any]) -> dict[str, Any]:
    return max(m["cells"], key=lambda c: (abs(c["coverage"] - TARGET), c["key"], -c["games"]))


def _max_distance(m: dict[str, Any]) -> tuple[float, tuple[float, float]]:
    """The largest distance from 0.8 among the cells, with the interval of the maximum."""
    dists = [stats.distance_interval(c["coverage"], c["interval"], TARGET) for c in m["cells"]]
    return (
        max(d for d, _ in dists),
        (max(ci[0] for _, ci in dists), max(ci[1] for _, ci in dists)),
    )


def metrics(m: dict[str, Any]) -> dict[str, Any]:
    w = _worst(m)
    d, (lo, hi) = _max_distance(m)
    within = sum(1 for c in m["cells"] if 0.75 <= c["interval"][0] and c["interval"][1] <= 0.85)
    return {
        "worst_coverage": {
            "value": _r(w["coverage"]),
            "interval": [_r(x) for x in w["interval"]],
            "level": "Bonferroni 95%, 20 cells",
            "at": f"{w['key']} at {w['games']} games",
            "distance": _r(d),
            "distance_interval": [_r(lo), _r(hi)],
            "within_band": within,
        },
        "rho1_coverage": {
            "value": _r(m["rho1"]["coverage"]),
            "interval": [_r(x) for x in m["rho1"]["interval"]],
            "level": "Bonferroni 95%, 21 margins",
        },
    }


def _change(
    champion: tuple[float, tuple[float, float]], challenger: tuple[float, tuple[float, float]]
) -> dict[str, Any]:
    d, lo, hi = stats.mover(challenger[0], challenger[1], champion[0], champion[1])
    return {
        "effect": _r(d),
        "interval": [_r(lo), _r(hi)],
        "level": "Bonferroni 95%",
        "effect_is": "the change in the distance from 0.8",
        "verdict": stats.verdict(lo, hi, "lower"),
    }


def _rho1(m: dict[str, Any]) -> tuple[float, tuple[float, float]]:
    return stats.distance_interval(m["rho1"]["coverage"], m["rho1"]["interval"], TARGET)


def compare(champion: dict[str, Any], challenger: dict[str, Any]) -> dict[str, Any]:
    def cells(m: dict[str, Any]) -> list[tuple[str, int]]:
        return [(c["key"], c["games"]) for c in m["cells"]]

    if cells(champion) != cells(challenger):
        raise ValueError("EXP-0002: the two runs do not have the same 20 cells")
    return {
        "worst_coverage": _change(_max_distance(champion), _max_distance(challenger)),
        "rho1_coverage": _change(_rho1(champion), _rho1(challenger)),
    }

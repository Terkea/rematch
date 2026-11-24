"""EXP-0002's champion/challenger rule (rematch_lab.arena, ADR 0109).

EXP-0002 is a measurement of one version: each of the 20 key × checkpoint coverages of the 80%
range is classified by its Bonferroni interval against [0.75, 0.85] (preregistration.md, section
9). Its follow-up, EXP-0006, registered the comparison of two versions on this design: the
primary is the worst of the 20 coverages, and the coverage at the peers' rate (ρ = 1) is
guarded. That is the rule here.

- worst coverage (primary): the distance from 0.8 of the cell furthest from it, champion and
  challenger each; the change in that distance with the MOVER interval from the two cells'
  Bonferroni intervals (family of 20). Better when the interval lies wholly below 0.
- coverage at ρ = 1 (guard): the same, from the ρ margin's Bonferroni interval (family of 21).
  Worse when the interval lies wholly above 0.

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


def metrics(m: dict[str, Any]) -> dict[str, Any]:
    w = _worst(m)
    within = sum(1 for c in m["cells"] if 0.75 <= c["interval"][0] and c["interval"][1] <= 0.85)
    return {
        "worst_coverage": {
            "value": _r(w["coverage"]),
            "interval": [_r(x) for x in w["interval"]],
            "level": "Bonferroni 95%, 20 cells",
            "at": f"{w['key']} at {w['games']} games",
            "within_band": within,
        },
        "rho1_coverage": {
            "value": _r(m["rho1"]["coverage"]),
            "interval": [_r(x) for x in m["rho1"]["interval"]],
            "level": "Bonferroni 95%, 21 margins",
        },
    }


def _distance_change(champion: dict[str, Any], challenger: dict[str, Any]) -> dict[str, Any]:
    d1, ci1 = stats.distance_interval(challenger["coverage"], challenger["interval"], TARGET)
    d2, ci2 = stats.distance_interval(champion["coverage"], champion["interval"], TARGET)
    d, lo, hi = stats.mover(d1, ci1, d2, ci2)
    return {
        "effect": _r(d),
        "interval": [_r(lo), _r(hi)],
        "level": "Bonferroni 95%",
        "effect_is": "the change in the distance from 0.8",
        "verdict": stats.verdict(lo, hi, "lower"),
    }


def compare(champion: dict[str, Any], challenger: dict[str, Any]) -> dict[str, Any]:
    cells = lambda m: [(c["key"], c["games"]) for c in m["cells"]]  # noqa: E731
    if cells(champion) != cells(challenger):
        raise ValueError("EXP-0002: the two runs do not have the same 20 cells")
    return {
        "worst_coverage": _distance_change(_worst(champion), _worst(challenger)),
        "rho1_coverage": _distance_change(champion["rho1"], challenger["rho1"]),
    }

"""EXP-0001's champion/challenger rule (rematch_lab.arena, ADR 0109).

The comparison is the one this experiment registered (preregistration.md, sections 5, 8 and 9,
and the analysis.py docstring): two versions on the same pilot players, the player as the unit.

- recall at 2 at 80 games (primary): the paired difference of hits. Better by the registered ship
  rule, the two-sided 97.5% Newcombe interval (method 10) wholly above 0; worse by ADR 0104's
  guard, the 80% interval wholly below 0.
- coverage of the target's 80% intervals up to 80 games (guard): the change in the distance from
  0.8, with an 80% paired cluster bootstrap over players, resampled within flaw as section 8
  registered. Worse only when the interval lies wholly above the registered tolerance of 0.05.
- clean controls with a rank-1 weakness at any checkpoint up to 80 games (report): two clean
  players cannot decide anything, so it is shown and never gates.

The bootstraps draw from `seed("EXP-0001", "arena/...")`, so a comparison is byte-stable.
"""

from __future__ import annotations

import random
from typing import Any

from rematch_lab import stats
from rematch_lab.experiments import seed

GAMES = 80
# The synthetic players come from the bench crate: a change there is a new experiment.
SOURCE_PATHS = ["backend/crates/bench/src"]
TARGET = 0.8
SHIP_LEVEL = 0.975
GUARD_LEVEL = 0.80
COVERAGE_MARGIN = 0.05  # the registered coverage tolerance (decision.md, the CI rule)
BOOTSTRAP = 2000

METRICS = [
    {
        "name": "recall_at_2",
        "role": "primary",
        "label": "Recall at 2, 80 games",
        "better": "higher",
    },
    {
        "name": "coverage",
        "role": "guard",
        "label": "Coverage of the 80% ranges, up to 80 games",
        "better": "closer to 0.8",
    },
    {
        "name": "clean_false_alarms",
        "role": "report",
        "label": "Clean controls with a rank-1 weakness, up to 80 games",
        "better": "lower",
    },
]


def measure(results: dict[str, Any]) -> dict[str, Any]:
    """Per player, what the comparison needs: the flaw, the hit at 80 games and the coverage
    flags up to 80 games; per clean control, whether anything was ranked first by 80 games."""
    flawed: dict[str, Any] = {}
    clean: dict[str, bool] = {}
    for p in results["pilot"]["players"]:
        cps = [c for c in p["checkpoints"] if c["games"] <= GAMES]
        if p["flaw"] is None:
            clean[p["name"]] = any(c["rank1"] is not None for c in cps)
        else:
            at = next(c for c in cps if c["games"] == GAMES)
            flawed[p["name"]] = {
                "flaw": p["flaw"],
                "hit2": bool(at["hit2"]),
                "covered": [bool(c["covered"]) for c in cps],
            }
    return {"flawed": dict(sorted(flawed.items())), "clean": dict(sorted(clean.items()))}


def _r(x: float) -> float:
    return round(x, 6)


def _coverage(players: list[dict[str, Any]]) -> float:
    flags = [f for p in players for f in p["covered"]]
    return sum(flags) / len(flags)


def _strata(names: list[str], flawed: dict[str, Any]) -> list[list[int]]:
    """The players' indices, by flaw."""
    by: dict[str, list[int]] = {}
    for i, n in enumerate(names):
        by.setdefault(flawed[n]["flaw"], []).append(i)
    return [by[k] for k in sorted(by)]


def _resample(rng: random.Random, strata: list[list[int]]) -> list[int]:
    return [s[rng.randrange(len(s))] for s in strata for _ in s]


def _percentiles(values: list[float], level: float) -> tuple[float, float]:
    values = sorted(values)
    tail = (1 - level) / 2
    return values[int(tail * len(values))], values[int((1 - tail) * len(values)) - 1]


def metrics(m: dict[str, Any]) -> dict[str, Any]:
    names = list(m["flawed"])
    flawed = [m["flawed"][n] for n in names]
    hits = sum(p["hit2"] for p in flawed)
    recall_ci = stats.wilson(hits, len(flawed), 0.95)
    rng = random.Random(seed("EXP-0001", "arena/coverage"))
    strata = _strata(names, m["flawed"])
    boots = [
        _coverage([flawed[i] for i in _resample(rng, strata)]) for _ in range(BOOTSTRAP)
    ]
    lo, hi = _percentiles(boots, 0.95)
    alarms = sum(m["clean"].values())
    clean_ci = stats.wilson(alarms, len(m["clean"]), GUARD_LEVEL)
    return {
        "recall_at_2": {
            "value": _r(hits / len(flawed)),
            "interval": [_r(x) for x in recall_ci],
            "level": 0.95,
        },
        "coverage": {"value": _r(_coverage(flawed)), "interval": [_r(lo), _r(hi)], "level": 0.95},
        "clean_false_alarms": {
            "value": _r(alarms / len(m["clean"])),
            "interval": [_r(x) for x in clean_ci],
            "level": GUARD_LEVEL,
        },
    }


def _pairs(a: list[bool], b: list[bool]) -> tuple[int, int, int, int]:
    """(n11, n10, n01, n00) with the challenger first: n10 is a challenger-only hit."""
    n11 = sum(1 for x, y in zip(a, b, strict=True) if x and y)
    n10 = sum(1 for x, y in zip(a, b, strict=True) if y and not x)
    n01 = sum(1 for x, y in zip(a, b, strict=True) if x and not y)
    return n11, n10, n01, len(a) - n11 - n10 - n01


def compare(champion: dict[str, Any], challenger: dict[str, Any]) -> dict[str, Any]:
    if champion["flawed"].keys() != challenger["flawed"].keys() or (
        champion["clean"].keys() != challenger["clean"].keys()
    ):
        raise ValueError("EXP-0001: the two runs are not on the same players")
    names = list(champion["flawed"])
    a = [champion["flawed"][n] for n in names]
    b = [challenger["flawed"][n] for n in names]

    pairs = _pairs([p["hit2"] for p in a], [p["hit2"] for p in b])
    d, lo, hi = stats.newcombe_paired(*pairs, level=SHIP_LEVEL)
    _, glo, ghi = stats.newcombe_paired(*pairs, level=GUARD_LEVEL)
    verdict = stats.BETTER if lo > 0 else stats.WORSE if ghi < 0 else stats.NO_CLEAR
    recall = {
        "effect": _r(d),
        "interval": [_r(lo), _r(hi)],
        "level": SHIP_LEVEL,
        "guard_interval": [_r(glo), _r(ghi)],
        "guard_level": GUARD_LEVEL,
        "verdict": verdict,
    }

    def gap(rows: list[dict[str, Any]]) -> float:
        return abs(_coverage(rows) - TARGET)

    rng = random.Random(seed("EXP-0001", "arena/coverage-paired"))
    strata = _strata(names, champion["flawed"])
    boots = []
    for _ in range(BOOTSTRAP):
        idx = _resample(rng, strata)
        boots.append(gap([b[i] for i in idx]) - gap([a[i] for i in idx]))
    lo, hi = _percentiles(boots, GUARD_LEVEL)
    coverage_verdict = (
        stats.WORSE if lo > COVERAGE_MARGIN else stats.BETTER if hi < 0 else stats.NO_CLEAR
    )
    coverage = {
        "effect": _r(gap(b) - gap(a)),
        "interval": [_r(lo), _r(hi)],
        "level": GUARD_LEVEL,
        "effect_is": "the change in the distance from 0.8",
        "margin": COVERAGE_MARGIN,
        "verdict": coverage_verdict,
    }

    clean_names = list(champion["clean"])
    cp = _pairs(
        [champion["clean"][n] for n in clean_names], [challenger["clean"][n] for n in clean_names]
    )
    d, lo, hi = stats.newcombe_paired(*cp, level=GUARD_LEVEL)
    clean = {
        "effect": _r(d),
        "interval": [_r(lo), _r(hi)],
        "level": GUARD_LEVEL,
        "verdict": stats.verdict(lo, hi, "lower"),
    }
    return {"recall_at_2": recall, "coverage": coverage, "clean_false_alarms": clean}

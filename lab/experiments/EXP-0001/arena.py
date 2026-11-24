"""EXP-0001's champion/challenger rule (rematch_lab.arena, ADR 0108).

The comparison is the one this experiment registered (preregistration.md, sections 5 and 9, and
the analysis.py docstring): a paired comparison of two versions on the same pilot players, the
player as the unit.

- recall at 2 at 80 games (primary): the paired difference with the two-sided 97.5% Newcombe
  interval (method 10); better when it lies wholly above 0, worse when wholly below.
- coverage of the target's 80% intervals up to 80 games (guard): the change in the distance from
  0.8, with an 80% paired cluster bootstrap over players (ADR 0104's guard level); worse when the
  interval lies wholly above 0.
- clean checkpoints with a rank-1 weakness up to 80 games (guard): the paired difference with the
  80% Newcombe interval; worse when it lies wholly above 0.

The bootstrap draws from `seed("EXP-0001", "arena/coverage")`, so a comparison is byte-stable.
"""

from __future__ import annotations

import random
from typing import Any

from rematch_lab import stats
from rematch_lab.experiments import seed

GAMES = 80
TARGET = 0.8
SHIP_LEVEL = 0.975
GUARD_LEVEL = 0.80
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
        "role": "guard",
        "label": "Clean checkpoints with a rank-1 weakness, up to 80 games",
        "better": "lower",
    },
]


def measure(results: dict[str, Any]) -> dict[str, Any]:
    """Per player, what the comparison needs: the hit at 80 games, the coverage flags and the
    clean controls' rank-1 flags up to 80 games."""
    flawed: dict[str, Any] = {}
    clean: dict[str, list[bool]] = {}
    for p in results["pilot"]["players"]:
        cps = [c for c in p["checkpoints"] if c["games"] <= GAMES]
        if p["flaw"] is None:
            clean[p["name"]] = [c["rank1"] is not None for c in cps]
        else:
            at = next(c for c in cps if c["games"] == GAMES)
            flawed[p["name"]] = {"hit2": bool(at["hit2"]), "covered": [bool(c["covered"]) for c in cps]}
    return {"flawed": dict(sorted(flawed.items())), "clean": dict(sorted(clean.items()))}


def _r(x: float) -> float:
    return round(x, 6)


def _coverage(players: list[dict[str, Any]]) -> float:
    flags = [f for p in players for f in p["covered"]]
    return sum(flags) / len(flags)


def metrics(m: dict[str, Any]) -> dict[str, Any]:
    flawed = list(m["flawed"].values())
    hits = sum(p["hit2"] for p in flawed)
    recall_ci = stats.wilson(hits, len(flawed), 0.95)
    rng = random.Random(seed("EXP-0001", "arena/coverage"))
    boots = sorted(
        _coverage([flawed[rng.randrange(len(flawed))] for _ in flawed]) for _ in range(BOOTSTRAP)
    )
    clean = [f for flags in m["clean"].values() for f in flags]
    clean_ci = stats.wilson(sum(clean), len(clean), GUARD_LEVEL)
    return {
        "recall_at_2": {
            "value": _r(hits / len(flawed)),
            "interval": [_r(x) for x in recall_ci],
            "level": 0.95,
        },
        "coverage": {
            "value": _r(_coverage(flawed)),
            "interval": [_r(boots[int(0.025 * BOOTSTRAP)]), _r(boots[int(0.975 * BOOTSTRAP) - 1])],
            "level": 0.95,
        },
        "clean_false_alarms": {
            "value": _r(sum(clean) / len(clean)),
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

    d, lo, hi = stats.newcombe_paired(
        *_pairs([p["hit2"] for p in a], [p["hit2"] for p in b]), level=SHIP_LEVEL
    )
    recall = {"effect": _r(d), "interval": [_r(lo), _r(hi)], "level": SHIP_LEVEL}
    recall["verdict"] = stats.verdict(lo, hi, "higher")

    def gap(rows: list[dict[str, Any]]) -> float:
        return abs(_coverage(rows) - TARGET)

    rng = random.Random(seed("EXP-0001", "arena/coverage-paired"))
    boots = []
    for _ in range(BOOTSTRAP):
        idx = [rng.randrange(len(names)) for _ in names]
        boots.append(gap([b[i] for i in idx]) - gap([a[i] for i in idx]))
    boots.sort()
    tail = (1 - GUARD_LEVEL) / 2
    lo, hi = boots[int(tail * BOOTSTRAP)], boots[int((1 - tail) * BOOTSTRAP) - 1]
    effect = gap(b) - gap(a)
    coverage = {
        "effect": _r(effect),
        "interval": [_r(lo), _r(hi)],
        "level": GUARD_LEVEL,
        "effect_is": "the change in the distance from 0.8",
        "verdict": stats.verdict(lo, hi, "lower"),
    }

    clean_names = list(champion["clean"])
    ca = [f for n in clean_names for f in champion["clean"][n]]
    cb = [f for n in clean_names for f in challenger["clean"][n]]
    d, lo, hi = stats.newcombe_paired(*_pairs(ca, cb), level=GUARD_LEVEL)
    clean = {
        "effect": _r(d),
        "interval": [_r(lo), _r(hi)],
        "level": GUARD_LEVEL,
        "verdict": stats.verdict(lo, hi, "lower"),
    }
    return {"recall_at_2": recall, "coverage": coverage, "clean_false_alarms": clean}

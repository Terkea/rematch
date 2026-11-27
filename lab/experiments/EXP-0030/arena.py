"""EXP-0030's champion/challenger rule (rematch_lab.arena, ADR 0109), from section 9 of
preregistration.md: two builds on the same players, the player as the unit, paired.

`analysis.py` names its rules in the request, so a build is measured on the rule it ships,
`memory-halves` (production since algo-6.0.0), with its flags in results.json (`flags`):

- false alarms (six primaries): the share of constant-strength players told `falling`, and told
  `rising`, at 60, 120 and 250 rated games. The change, challenger minus champion, with the paired
  Newcombe interval (method 10) at the registered level 1 - 0.025/2. Better when it lies wholly
  below 0, worse when wholly above.
- detection at a 200-point fall (three guards, one a length): the challenger's own share with
  its 80% Wilson interval; worse when the lower end falls below 0.50, the registered floor.
- detection at a 100-point fall, pooled across lengths (report): the paired change, never gating.

The rough and chess.com-like worlds and the cohort check are not re-compared here: they guarded
the decision, and the cohort is an external input a hosted runner does not have.
"""

from __future__ import annotations

from typing import Any

from rematch_lab import stats

RULE = "memory-halves"
CHECKPOINTS = ["60", "120", "250"]
SIDES = ["falling", "rising"]
PRIMARY_LEVEL = 1 - 0.025 / 2
GUARD_LEVEL = 0.80
DETECTION_FLOOR = 0.50

METRICS = [
    *[
        {
            "name": f"false_{side}_{g}",
            "role": "primary",
            "label": f'Constant-strength players told "{side}", {g} games',
            "better": "lower",
        }
        for side in SIDES
        for g in CHECKPOINTS
    ],
    *[
        {
            "name": f"detection_200_{g}",
            "role": "guard",
            "label": f'Players falling 200 points told "falling", {g} games',
            "better": "higher",
        }
        for g in CHECKPOINTS
    ],
    {
        "name": "detection_100",
        "role": "report",
        "label": 'Players falling 100 points told "falling"',
        "better": "higher",
    },
]


def _r(x: float) -> float:
    return round(x, 6)


def measure(results: dict[str, Any]) -> dict[str, Any]:
    f = results["flags"][RULE]
    return {
        "false": {side: {g: f[side][g] for g in CHECKPOINTS} for side in SIDES},
        "detection_200": {g: f["detection"][f"200/{g}"] for g in CHECKPOINTS},
        "detection_100": "".join(f["detection"][f"100/{g}"] for g in CHECKPOINTS),
    }


def _share(bits: str, level: float) -> dict[str, Any]:
    k, n = bits.count("1"), len(bits)
    return {
        "value": _r(k / n),
        "interval": [_r(x) for x in stats.wilson(k, n, level)],
        "level": level,
    }


def metrics(m: dict[str, Any]) -> dict[str, Any]:
    out = {
        f"false_{side}_{g}": _share(m["false"][side][g], PRIMARY_LEVEL)
        for side in SIDES
        for g in CHECKPOINTS
    }
    for g in CHECKPOINTS:
        out[f"detection_200_{g}"] = _share(m["detection_200"][g], GUARD_LEVEL)
    out["detection_100"] = _share(m["detection_100"], GUARD_LEVEL)
    return out


def _pairs(champion: str, challenger: str) -> tuple[int, int, int, int]:
    """(n11, n10, n01, n00) with the challenger first: n10 is a challenger-only flag."""
    if len(champion) != len(challenger):
        raise ValueError("EXP-0030: the two runs are not on the same players")
    n11 = sum(1 for a, b in zip(champion, challenger, strict=True) if a == "1" and b == "1")
    n10 = sum(1 for a, b in zip(champion, challenger, strict=True) if a == "0" and b == "1")
    n01 = sum(1 for a, b in zip(champion, challenger, strict=True) if a == "1" and b == "0")
    return n11, n10, n01, len(champion) - n11 - n10 - n01


def _paired(champion: str, challenger: str, level: float, better: str) -> dict[str, Any]:
    d, lo, hi = stats.newcombe_paired(*_pairs(champion, challenger), level=level)
    return {
        "effect": _r(d),
        "interval": [_r(lo), _r(hi)],
        "level": level,
        "verdict": stats.verdict(lo, hi, better),
    }


def compare(champion: dict[str, Any], challenger: dict[str, Any]) -> dict[str, Any]:
    out: dict[str, Any] = {}
    for side in SIDES:
        for g in CHECKPOINTS:
            out[f"false_{side}_{g}"] = _paired(
                champion["false"][side][g], challenger["false"][side][g], PRIMARY_LEVEL, "lower"
            )
    for g in CHECKPOINTS:
        entry = _paired(
            champion["detection_200"][g], challenger["detection_200"][g], GUARD_LEVEL, "higher"
        )
        bits = challenger["detection_200"][g]
        own = stats.wilson(bits.count("1"), len(bits), GUARD_LEVEL)
        entry["challenger_interval"] = [_r(x) for x in own]
        entry["floor"] = DETECTION_FLOOR
        entry["verdict"] = stats.WORSE if own[0] < DETECTION_FLOOR else stats.NO_CLEAR
        out[f"detection_200_{g}"] = entry
    out["detection_100"] = _paired(
        champion["detection_100"], challenger["detection_100"], GUARD_LEVEL, "higher"
    )
    return out

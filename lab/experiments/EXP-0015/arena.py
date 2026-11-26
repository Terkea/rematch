"""EXP-0015's champion/challenger rule (rematch_lab.arena, ADR 0109), as registered in section 9
of preregistration.md: two versions on the same players, the player as the unit, paired.

- false alarms at 60, 120 and 250 rated games (three primaries): the change in the share of
  constant-strength players told `falling`, challenger minus champion, with the paired Newcombe
  interval (method 10) at 1 - 0.05/3. Better when it lies wholly below 0, worse when wholly above.
  A challenger beats the champion only when all three are better (ADR 0109: on the primaries);
  whether it ships is its own experiment's rule, which also asks for its cells within the bound.
- detection at a 200-point decline (guard): the 600 players of its three cells pooled. The
  champion's detection is inflated by its false alarms, so the guard is an absolute floor, not a
  change: worse when the upper end of the challenger's own 80% Wilson interval lies below 0.50
  (it misses most real 200-point falls). The paired change (Newcombe, 80%) is reported with it.
- detection at a 100-point decline (report): the paired change, never gating.

The per-player flags come from results.json (`flags`), so the comparison is exactly paired.
"""

from __future__ import annotations

from typing import Any

from rematch_lab import stats

CHECKPOINTS = ["60", "120", "250"]
PRIMARY_LEVEL = 1 - 0.05 / 3
GUARD_LEVEL = 0.80
DETECTION_FLOOR = 0.50

METRICS = [
    *[
        {
            "name": f"false_alarm_{g}",
            "role": "primary",
            "label": f'Constant-strength players told "falling", {g} games',
            "better": "lower",
        }
        for g in CHECKPOINTS
    ],
    {
        "name": "detection_200",
        "role": "guard",
        "label": 'Players falling 200 points told "falling"',
        "better": "higher",
    },
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
    f = results["flags"]
    return {
        "false_alarm": {g: f["false_alarm"][g] for g in CHECKPOINTS},
        "detection": {
            d: "".join(f["detection"][f"{d}/{g}"] for g in CHECKPOINTS) for d in ("100", "200")
        },
    }


def _share(bits: str, level: float) -> dict[str, Any]:
    k, n = bits.count("1"), len(bits)
    return {
        "value": _r(k / n),
        "interval": [_r(x) for x in stats.wilson(k, n, level)],
        "level": level,
    }


def metrics(m: dict[str, Any]) -> dict[str, Any]:
    out = {f"false_alarm_{g}": _share(m["false_alarm"][g], PRIMARY_LEVEL) for g in CHECKPOINTS}
    out["detection_200"] = _share(m["detection"]["200"], GUARD_LEVEL)
    out["detection_100"] = _share(m["detection"]["100"], GUARD_LEVEL)
    return out


def _pairs(champion: str, challenger: str) -> tuple[int, int, int, int]:
    """(n11, n10, n01, n00) with the challenger first: n10 is a challenger-only flag."""
    if len(champion) != len(challenger):
        raise ValueError("EXP-0015: the two runs are not on the same players")
    n11 = sum(1 for a, b in zip(champion, challenger, strict=True) if a == "1" and b == "1")
    n10 = sum(1 for a, b in zip(champion, challenger, strict=True) if a == "0" and b == "1")
    n01 = sum(1 for a, b in zip(champion, challenger, strict=True) if a == "1" and b == "0")
    return n11, n10, n01, len(champion) - n11 - n10 - n01


def compare(champion: dict[str, Any], challenger: dict[str, Any]) -> dict[str, Any]:
    out: dict[str, Any] = {}
    for g in CHECKPOINTS:
        pairs = _pairs(champion["false_alarm"][g], challenger["false_alarm"][g])
        d, lo, hi = stats.newcombe_paired(*pairs, level=PRIMARY_LEVEL)
        out[f"false_alarm_{g}"] = {
            "effect": _r(d),
            "interval": [_r(lo), _r(hi)],
            "level": PRIMARY_LEVEL,
            "verdict": stats.verdict(lo, hi, "lower"),
        }
    for dec, role in (("200", "guard"), ("100", "report")):
        pairs = _pairs(champion["detection"][dec], challenger["detection"][dec])
        d, lo, hi = stats.newcombe_paired(*pairs, level=GUARD_LEVEL)
        entry = {
            "effect": _r(d),
            "interval": [_r(lo), _r(hi)],
            "level": GUARD_LEVEL,
            "verdict": stats.verdict(lo, hi, "higher"),
        }
        if role == "guard":
            bits = challenger["detection"][dec]
            own = stats.wilson(bits.count("1"), len(bits), GUARD_LEVEL)
            entry["challenger_interval"] = [_r(x) for x in own]
            entry["floor"] = DETECTION_FLOOR
            entry["verdict"] = stats.WORSE if own[1] < DETECTION_FLOOR else stats.NO_CLEAR
        out[f"detection_{dec}"] = entry
    return out

"""EXP-0009's champion/challenger rule (rematch_lab.arena, ADR 0109).

EXP-0009 registered a measurement on one version, not a comparison, so this rule is adopted
with its decision, as ADR 0109 adopted EXP-0002's, and was not pre-registered. On the same
cohort (the same four dumps, the same salt and the same confirmatory slice), a version changes
these numbers only by changing how a game is evaluated, judged or observed: the eight
evaluation-only detectors (W4, W5, W6, W7, W9a, W9b), accuracy, or the judgement rule.

- how reliably the version measures a player (primary): the split-half reliability of each
  key's August level (results.json `level`, Spearman-Brown on odd against even games), on
  about 16,000 players, where its standard error is near 0.008. Worse when any key's falls by
  more than 0.05 (about six standard errors); better when their mean rises by more than 0.02 and
  none falls by more than 0.05; otherwise no clear difference. A detector that measures a player
  more consistently is a better instrument whatever it later links to.
- the keys whose fall precedes climbing (report): the set that reads "precedes climbing" under
  section 9. Reported, never a verdict: on the champion it is empty, and a detector more
  entangled with results could gain a spurious key.
- each quantity's effect, next month's Elo per SD of improvement (report): the change in the
  estimate and its MOVER interval from the two 95% intervals. Both versions read the same
  players, so that interval ignores their correlation and errs towards "no clear difference";
  no verdict rests on it.

The inputs are external: the four cohort files of ADR 0114 in the data directory (ADR 0106
Amendment 1). Without them the arena does not run this experiment and says so.
"""

from __future__ import annotations

from typing import Any

from rematch_lab import stats

SEED_NAMESPACE = "EXP-0009"
QUANTITIES = ("W4.missed", "W4.allowed", "W5", "W6", "W7.traps", "W7.prep", "W9a", "W9b",
              "accuracy")
LEVEL = "95%, HC3 over players"

KEYS = QUANTITIES[:8]
FALL = 0.05
RISE = 0.02

METRICS = [
    {
        "name": "level_reliability",
        "role": "primary",
        "label": "Mean split-half reliability of the eight keys' August level",
        "better": "higher, no key 0.05 lower",
    },
    {
        "name": "keys_preceding_climbing",
        "role": "report",
        "label": "Keys whose fall precedes climbing (section 9)",
        "better": "report only",
    },
    *[
        {
            "name": f"effect:{q}",
            "role": "report",
            "label": f"{q}: next month's Elo per SD of improvement",
            "better": "higher",
        }
        for q in QUANTITIES
    ],
]


def _r(x: float) -> float:
    return round(x, 6)


def measure(results: dict[str, Any]) -> dict[str, Any]:
    """The decision and each quantity's effect with its 95% interval."""
    primary = results["primary"]
    return {
        "reliability": {q: (None if results["level"][q].get("reliability") is None
                            else _r(results["level"][q]["reliability"])) for q in KEYS},
        "outcome": results["decision"]["outcome"],
        "climbing": sorted(results["decision"]["climbing"]),
        "effects": {
            q: (
                {"estimate": _r(primary[q]["estimate"]),
                 "interval": [_r(x) for x in primary[q]["ci95"]]}
                if primary[q].get("estimate") is not None else None
            )
            for q in QUANTITIES
        },
    }


def metrics(m: dict[str, Any]) -> dict[str, Any]:
    rel = [v for v in m["reliability"].values() if v is not None]
    out: dict[str, Any] = {
        "level_reliability": {
            "value": _r(sum(rel) / len(rel)) if rel else None,
            "interval": None,
            "level": "point estimates, SE about 0.008 each",
        },
        "keys_preceding_climbing": {
            "value": float(len(m["climbing"])),
            "interval": None,
            "level": "section 9's rule",
            "at": f"{m['outcome']}: {', '.join(m['climbing']) or 'none'}",
        }
    }
    for q, e in m["effects"].items():
        out[f"effect:{q}"] = (
            {"value": e["estimate"], "interval": e["interval"], "level": LEVEL}
            if e else {"value": None, "interval": None, "level": "not estimable"}
        )
    return out


def compare(champion: dict[str, Any], challenger: dict[str, Any]) -> dict[str, Any]:
    both = [q for q in KEYS if champion["reliability"].get(q) is not None
            and challenger["reliability"].get(q) is not None]
    diffs = [challenger["reliability"][q] - champion["reliability"][q] for q in both]
    mean = sum(diffs) / len(diffs) if diffs else 0.0
    fell = any(d < -FALL for d in diffs)
    a, b = set(champion["climbing"]), set(challenger["climbing"])
    out: dict[str, Any] = {
        "level_reliability": {
            "effect": _r(mean),
            "interval": None,
            "level": f"any key below -{FALL}, mean above +{RISE}",
            "verdict": stats.WORSE if fell else stats.BETTER if mean > RISE else stats.NO_CLEAR,
        },
        "keys_preceding_climbing": {
            "effect": float(len(b) - len(a)),
            "interval": None,
            "level": "section 9's rule",
            "verdict": stats.NO_CLEAR,
        },
    }
    for q in QUANTITIES:
        x, y = champion["effects"].get(q), challenger["effects"].get(q)
        if not x or not y:
            out[f"effect:{q}"] = {"effect": None, "interval": None, "level": LEVEL,
                                  "verdict": stats.NO_CLEAR}
            continue
        d, lo, hi = stats.mover(y["estimate"], tuple(y["interval"]), x["estimate"],
                                tuple(x["interval"]))
        out[f"effect:{q}"] = {"effect": _r(d), "interval": [_r(lo), _r(hi)], "level": LEVEL,
                              "verdict": stats.verdict(lo, hi, "higher")}
    return out

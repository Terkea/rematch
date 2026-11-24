"""EXP-0007's champion/challenger rule (rematch_lab.arena, ADR 0109).

The comparison is the one this experiment registered (preregistration.md, section 9): per tag,
precision and recall against Lichess's generated puzzle themes on the same 54,987 puzzles, with
the simultaneous Bonferroni intervals over 30 (`ci_decision`, z 3.144), and the floors 0.80 for
precision and 0.50 for recall.

- trusted tags (primary): the tags section 9 trusts, both intervals wholly at or above their
  floors, read from the frozen analysis's own `decision`. Better when the challenger trusts every
  tag the champion trusts and at least one more; worse when it stops trusting any of them, by
  precision or by recall (shown as the share of the 15). A tag clears only by the registered
  rule, never by a rise in its estimate alone.
- precision of each tag (guard): worse when the champion trusts the tag and the lower end of the
  challenger's interval falls below 0.80, so a trusted tag is never kept on a weaker precision.
  For a tag the champion does not trust it is shown and cannot fail the run.
- recall of each tag (report): shown with the change and its MOVER interval; a trusted tag whose
  recall falls below 0.50 already fails the primary.

The effect of each per-tag metric is the change in the estimate, with the MOVER interval of the
two Bonferroni intervals. Both versions tag the same puzzles, so that interval ignores their
correlation and errs towards "no clear difference"; no verdict here rests on it alone.

The input is external: the pinned Lichess puzzle snapshot (ADR 0106 Amendment 1). Without it the
arena does not run this experiment and says so.
"""

from __future__ import annotations

from typing import Any

from rematch_lab import stats

# analysis.py was frozen before the arena and takes no --algorithm; the arena checks the
# algorithm_id the run records instead.
ALGORITHM_FLAG = False
# Its confirmatory run draws nothing: it reads the committed sample (`sample.csv`, drawn with the
# seed `sample/confirmatory` before registration and checked by `inputs.sha256`), so its results
# record no seed namespace and the experiment's own id stands for it.
SEED_NAMESPACE = "EXP-0007"
PRECISION_FLOOR = 0.80
RECALL_FLOOR = 0.50
TRUSTED = "trusted"
LEVEL = "Bonferroni 95%, 30 intervals"
# The 15 scored tags of section 4, in the order of the decision table.
TAGS = [
    "fork",
    "discoveredAttack",
    "promotion",
    "smotheredMate",
    "skewer",
    "backRankMate",
    "hangingPiece",
    "deflection",
    "sacrifice",
    "pin",
    "mate",
    "mateIn1",
    "mateIn2",
    "mateIn3",
    "mateIn4Plus",
]

METRICS = [
    {
        "name": "trusted_tags",
        "role": "primary",
        "label": "Share of the 15 tags trusted by the registered rule",
        "better": "more, losing none",
    },
    *(
        {
            "name": f"precision_{t}",
            "role": "guard",
            "label": f"Precision of `{t}`",
            "better": "higher; a trusted tag stays at or above 0.80",
        }
        for t in TAGS
    ),
    *(
        {
            "name": f"recall_{t}",
            "role": "report",
            "label": f"Recall of `{t}`",
            "better": "higher",
        }
        for t in TAGS
    ),
]


def _r(x: float) -> float:
    return round(x, 6)


def _side(pr: dict[str, Any]) -> dict[str, Any]:
    ci = pr.get("ci_decision")
    return {"estimate": pr.get("estimate"), "interval": list(ci) if ci is not None else None}


def measure(results: dict[str, Any]) -> dict[str, Any]:
    """Per tag, the frozen decision and the two estimates with their decision intervals."""
    tags = results["tags"]
    missing = [t for t in TAGS if t not in tags]
    if missing:
        raise ValueError(f"EXP-0007: results.json has no {', '.join(missing)}")
    return {
        t: {
            "decision": tags[t]["decision"],
            "precision": _side(tags[t]["precision"]),
            "recall": _side(tags[t]["recall"]),
        }
        for t in TAGS
    }


def _trusted(m: dict[str, Any]) -> list[str]:
    return [t for t in TAGS if m[t]["decision"] == TRUSTED]


def _value(side: dict[str, Any]) -> dict[str, Any]:
    # An unmeasured estimate (a tag we never fire) is shown as 0 with no interval.
    est = side["estimate"]
    iv = side["interval"]
    return {
        "value": _r(est) if est is not None else 0.0,
        "interval": [_r(x) for x in iv] if iv is not None else None,
        "level": LEVEL,
    }


def metrics(m: dict[str, Any]) -> dict[str, Any]:
    trusted = _trusted(m)
    out: dict[str, Any] = {
        "trusted_tags": {
            "value": _r(len(trusted) / len(TAGS)),
            "interval": None,
            "level": "section 9's rule",
            "at": f"{len(trusted)} of {len(TAGS)}: {', '.join(trusted)}",
            "tags": trusted,
        }
    }
    for t in TAGS:
        out[f"precision_{t}"] = _value(m[t]["precision"])
        out[f"recall_{t}"] = _value(m[t]["recall"])
    return out


def _change(a: dict[str, Any], b: dict[str, Any]) -> dict[str, Any]:
    """The change in the estimate, with the MOVER interval when both sides are measured."""
    if a["estimate"] is None or b["estimate"] is None or not a["interval"] or not b["interval"]:
        return {"effect": 0.0, "interval": None, "level": LEVEL, "measured": False}
    d, lo, hi = stats.mover(b["estimate"], tuple(b["interval"]), a["estimate"], tuple(a["interval"]))
    return {"effect": _r(d), "interval": [_r(lo), _r(hi)], "level": LEVEL, "measured": True}


def _clears(side: dict[str, Any]) -> bool:
    return bool(side["interval"]) and side["interval"][0] >= PRECISION_FLOOR


def _precision_verdict(trusted_by_champion: bool, a: dict[str, Any], b: dict[str, Any]) -> str:
    """Worse when a tag the champion trusts drops below the floor; better when a tag below the
    floor under the champion clears it (which alone does not trust it: the primary decides)."""
    if trusted_by_champion and not _clears(b):
        return stats.WORSE
    if not _clears(a) and _clears(b):
        return stats.BETTER
    return stats.NO_CLEAR


def compare(champion: dict[str, Any], challenger: dict[str, Any]) -> dict[str, Any]:
    if set(champion) != set(challenger):
        raise ValueError("EXP-0007: the two runs do not score the same tags")
    before, after = set(_trusted(champion)), set(_trusted(challenger))
    lost, gained = sorted(before - after, key=TAGS.index), sorted(after - before, key=TAGS.index)
    out: dict[str, Any] = {
        "trusted_tags": {
            "effect": _r((len(after) - len(before)) / len(TAGS)),
            "interval": None,
            "level": "section 9's rule",
            "lost": lost,
            "gained": gained,
            "verdict": stats.WORSE if lost else stats.BETTER if gained else stats.NO_CLEAR,
        }
    }
    for t in TAGS:
        a, b = champion[t], challenger[t]
        out[f"precision_{t}"] = {
            **_change(a["precision"], b["precision"]),
            "floor": PRECISION_FLOOR,
            "verdict": _precision_verdict(t in before, a["precision"], b["precision"]),
        }
        recall = _change(a["recall"], b["recall"])
        iv = recall["interval"]
        out[f"recall_{t}"] = {
            **recall,
            "verdict": stats.verdict(iv[0], iv[1], "higher") if iv else stats.NO_CLEAR,
        }
    return out

"""EXP-0008's champion/challenger rule (rematch_lab.arena, ADR 0109).

The comparison is the one this experiment registered (preregistration.md, section 9): on the
same sampled Lichess games, each node count's label disagreement with the 30-million-node
reference, read as paired contrasts with their 95% intervals against the tolerance of 0.01 of
moves. The frozen analysis searches fixed node counts (50,000 to 3 million), so a version changes
these numbers only by changing how a game's positions are chosen, scored or judged.

- the node count the version ships (primary): does the version's own `ENGINE_NODES` (results.json
  `production_nodes`) equal the count section 9 chooses on that version's run? Better when the
  challenger's does and the champion's did not; worse when the champion's did and the
  challenger's does not. A "need more data" keeps 300,000, as section 9 says.
- the step down, D(100,000) − D(300,000), and the step up, D(300,000) − D(1 million) (report):
  each with the change in the estimate and its MOVER interval. Both versions judge the same
  games, so that interval ignores their correlation and errs towards "no clear difference"; no
  verdict rests on it.

The input is external: the pinned Lichess extract (ADR 0106 Amendment 1). Without it the arena
does not run this experiment and says so. Every search is cached by Stockfish build, so a re-run
on the same engine searches nothing again.
"""

from __future__ import annotations

from typing import Any

from rematch_lab import stats

SEED_NAMESPACE = "EXP-0008"
DELTA = 0.01
LEVEL = "95%, t over games"
NODES = {"n50k": 50_000, "n100k": 100_000, "n300k": 300_000, "n1m": 1_000_000, "n3m": 3_000_000}
CONTRASTS = {"step_down": "n100k-n300k", "step_up": "n300k-n1m"}

METRICS = [
    {
        "name": "ships_the_chosen_count",
        "role": "primary",
        "label": "The version's node count is the one section 9 chooses",
        "better": "yes, where the champion's was not",
    },
    {
        "name": "step_down",
        "role": "report",
        "label": "D(100,000) − D(300,000), share of moves",
        "better": "lower",
    },
    {
        "name": "step_up",
        "role": "report",
        "label": "D(300,000) − D(1 million), share of moves",
        "better": "lower",
    },
]


def _r(x: float) -> float:
    return round(x, 6)


def measure(results: dict[str, Any]) -> dict[str, Any]:
    """The decision, the version's node count and the two contrasts with their intervals."""
    primary = results["primary"]
    return {
        "production_nodes": int(results["production_nodes"]),
        "outcome": results["decision"]["outcome"],
        "chosen_nodes": NODES[results["decision"]["setting"]],
        **{
            name: {
                "estimate": _r(primary[key]["estimate"]),
                "interval": [_r(x) for x in primary[key]["ci95"]],
            }
            for name, key in CONTRASTS.items()
        },
    }


def _ships(m: dict[str, Any]) -> bool:
    return m["production_nodes"] == m["chosen_nodes"]


def metrics(m: dict[str, Any]) -> dict[str, Any]:
    return {
        "ships_the_chosen_count": {
            "value": 1.0 if _ships(m) else 0.0,
            "interval": None,
            "level": "section 9's rule",
            "at": f"{m['outcome']}: {m['chosen_nodes']:,} nodes; the version uses "
            f"{m['production_nodes']:,}",
        },
        **{
            name: {"value": m[name]["estimate"], "interval": m[name]["interval"], "level": LEVEL}
            for name in CONTRASTS
        },
    }


def compare(champion: dict[str, Any], challenger: dict[str, Any]) -> dict[str, Any]:
    a, b = _ships(champion), _ships(challenger)
    out: dict[str, Any] = {
        "ships_the_chosen_count": {
            "effect": float(b) - float(a),
            "interval": None,
            "level": "section 9's rule",
            "verdict": stats.WORSE if a and not b else stats.BETTER if b and not a else stats.NO_CLEAR,
        }
    }
    for name in CONTRASTS:
        x, y = champion[name], challenger[name]
        d, lo, hi = stats.mover(y["estimate"], tuple(y["interval"]), x["estimate"], tuple(x["interval"]))
        out[name] = {
            "effect": _r(d),
            "interval": [_r(lo), _r(hi)],
            "level": LEVEL,
            "verdict": stats.verdict(lo, hi, "lower"),
        }
    return out

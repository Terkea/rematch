"""EXP-0019: a blind human audit of the motif tags the Lichess themes cannot judge.

The pre-registered analysis (preregistration.md, sections 8 and 9). A person judged, blind to our
tags and to Lichess's themes, whether each of four motifs is in each audited puzzle's solution
(`answers.json`, written by the audit page that `make_audit.py` generates). The hidden key
(`key.json`, also from `make_audit.py`) says which stratum each item was drawn from; our tags in
it come from the production Rust code through `cli lab tag-motifs`. Python here counts and
decides; it computes no tag.

    python analysis.py power            # section 6: power.json (before registration)
    python analysis.py --out DIR        # the registered run: DIR/results.json

The registered run reads its inputs where `inputs.sha256` says they are: the key and the answers
start as external inputs in the data directory and move into this folder once the audit is done
(section 7). The answers' line holds a placeholder hash until then, so the lab check says the
experiment is not run rather than running it on no answers.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import sys
from pathlib import Path
from typing import Any

import numpy as np
from scipy.stats import beta, norm

from rematch_lab.experiments import EXTERNAL, Budget, external_input, seed, write_results

EXP = "EXP-0019"
HERE = Path(__file__).resolve().parent

# Section 4: the four audited tags, in the order the page asks about them.
TAGS = ("hangingPiece", "skewer", "sacrifice", "backRankMate")
ANSWERS = ("yes", "no", "unsure")
KEY_FORMAT = "exp19-key-v1"
ANSWERS_FORMAT = "exp19-answers-v1"

# Section 9: the floor, the family and the guard.
FLOOR = 0.80
ALPHA = 0.05  # two-sided, over the four tags and the two looks (Bonferroni)
LOOKS = 2  # this session and at most one follow-up session that pools with it
ALPHA_TAG_ONE_SIDED = ALPHA / len(TAGS) / LOOKS / 2  # 0.003125: one direction, one tag, one look
# A bound has two parts: pi (EXP-0007, thousands of puzzles) and b (the audit, tens). Union bound.
ALPHA_PI = ALPHA_TAG_ONE_SIDED * 0.1  # 0.0003125
ALPHA_PART = ALPHA_TAG_ONE_SIDED * 0.9  # 0.0028125, the audit's part
# The follow-up session runs for an inconclusive tag whose estimate is at least or at most these.
FOLLOW_UP_IF_AT_LEAST = 0.88
FOLLOW_UP_IF_AT_MOST = 0.72
# The guards a clear also needs (section 9):
MAX_LABELLED_NOT_YES = 1  # 2 or more of a tag's labelled items not judged "yes": no clear
# The person's false "yes": question t on items with neither our tag t nor Lichess's theme t. A
# tag cannot clear while the upper 97.5% bound of that share is above FALSE_YES_MAX.
FALSE_YES_MAX = 0.15
FALSE_YES_ALPHA = 0.025

# Section 7: the design. Each block of 22 items, 6 blocks.
BLOCKS = 6
PER_BLOCK = {
    "U:hangingPiece": 5,
    "U:sacrifice": 5,
    "U:backRankMate": 4,
    "U:skewer": 2,
    # Labelled: hangingPiece and sacrifice every block, backRankMate and skewer in turn.
    "L": 3,
    "D": 3,  # distractors: none of the four tags fires
}
MIN_BLOCKS = 3
SECONDS_PER_ITEM = 40  # the planning assumption of section 6, checked by the page's timings

# Section 0: the compute budget of the registered run.
N_ITEMS = BLOCKS * sum(PER_BLOCK.values())
BUDGET = {"items_read": N_ITEMS}

EXP7_RESULTS = HERE.parent / "EXP-0007" / "results.json"
POWER_REPS = 20000


# ---- inputs -----------------------------------------------------------------------------------


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def registered_inputs() -> dict[str, tuple[str, Path]]:
    """name -> (sha256, path) for every line of inputs.sha256, by the file's base name."""
    out: dict[str, tuple[str, Path]] = {}
    for line in (HERE / "inputs.sha256").read_text(encoding="utf-8").splitlines():
        if not line.strip() or line.startswith("#"):
            continue
        want, _, rel = line.partition("  ")
        rel = rel.strip()
        path = external_input(rel[len(EXTERNAL):]) if rel.startswith(EXTERNAL) else HERE / rel
        out[Path(rel).name] = (want.strip(), path)
    return out


def read_input(name: str) -> Any:
    want, path = registered_inputs()[name]
    if not path.is_file():
        raise SystemExit(f"{name} is not at {path}: the audit is not done, so there is no run")
    if sha256_file(path) != want:
        raise SystemExit(f"{path} does not match its sha256 in inputs.sha256")
    return json.loads(path.read_text(encoding="utf-8"))


# ---- the share of tagged puzzles that carry the theme (EXP-0007) ------------------------------


def exp7_precision(results: dict[str, Any]) -> dict[str, dict[str, float]]:
    """pi_t: EXP-0007's estimate of the share of audience puzzles we tag t that carry Lichess's
    theme, with the smaller of its two effective sample sizes (the more cautious)."""
    out = {}
    for t in TAGS:
        p = results["tags"][t]["precision"]
        out[t] = {"estimate": p["estimate"], "n_eff": min(p["n_eff"], p["n_eff_kish"])}
    return out


def wilson(p: float, n: float, z: float) -> tuple[float, float]:
    denom = 1 + z * z / n
    centre = (p + z * z / (2 * n)) / denom
    half = z * math.sqrt(p * (1 - p) / n + z * z / (4 * n * n)) / denom
    return max(0.0, centre - half), min(1.0, centre + half)


def pi_bounds(pi: dict[str, float], alpha: float = ALPHA_PI) -> tuple[float, float]:
    return wilson(pi["estimate"], pi["n_eff"], float(norm.isf(alpha)))


def cp_lower(k: int, n: int, alpha: float = ALPHA_PART) -> float:
    """One-sided Clopper-Pearson lower bound."""
    if n == 0 or k == 0:
        return 0.0
    return float(beta.ppf(alpha, k, n - k + 1))


def cp_upper(k: int, n: int, alpha: float = ALPHA_PART) -> float:
    if n == 0 or k == n:
        return 1.0
    return float(beta.ppf(1 - alpha, k + 1, n - k))


def combine(pi: float, b: float) -> float:
    """Audited precision: the theme decides where it names the motif, the person where it is
    silent. P = pi + (1 - pi) b = 1 - (1 - pi)(1 - b), increasing in both."""
    return 1 - (1 - pi) * (1 - b)


# ---- the answers ------------------------------------------------------------------------------


def done_items(key: dict[str, Any], answers: dict[str, Any]) -> list[dict[str, Any]]:
    """The items of the whole blocks the person worked through, with each question's answer. A
    block is done when every item in it lies at or before the last item with any answer (the
    page shows them in order); a question left blank there counts as "unsure" (section 8)."""
    if answers.get("format") != ANSWERS_FORMAT:
        raise SystemExit(f"answers.json is not {ANSWERS_FORMAT}")
    if answers.get("page_id") != key["page_id"]:
        raise SystemExit("answers.json comes from another page than the registered one")
    given = answers.get("answers", {})
    numbers = [int(n) for n, a in given.items() if any(a.get(t) in ANSWERS for t in TAGS)]
    last = max(numbers, default=0)
    ends: dict[int, int] = {}
    for item in key["items"]:
        ends[item["block"]] = max(ends.get(item["block"], 0), item["item"])
    whole = {b for b, end in ends.items() if end <= last}
    out = []
    for item in key["items"]:
        if item["block"] not in whole:
            continue
        a = given.get(str(item["item"]), {})
        out.append({**item, "answers": {t: a.get(t) if a.get(t) in ANSWERS else "unsure"
                                        for t in TAGS}})
    return out


def tally(items: list[dict[str, Any]], stratum: str, tag: str) -> dict[str, int]:
    c = {"n": 0, "yes": 0, "no": 0, "unsure": 0}
    for it in items:
        if stratum in it["strata"]:
            c["n"] += 1
            c[it["answers"][tag]] += 1
    return c


def negatives(items: list[dict[str, Any]], tag: str) -> dict[str, int]:
    """Question `tag` on the items with neither our tag nor Lichess's theme: a "yes" there is,
    most likely, a false one."""
    c = {"n": 0, "yes": 0, "no": 0, "unsure": 0}
    for it in items:
        if tag not in it["tags"] and tag not in it["themes"]:
            c["n"] += 1
            c[it["answers"][tag]] += 1
    return c


def human_point(pi: float, u: dict[str, int], lab: dict[str, int]) -> float | None:
    """The precision with the labelled items judged too: pi a + (1 - pi) b, "unsure" as no."""
    if not u["n"] or not lab["n"]:
        return None
    return pi * lab["yes"] / lab["n"] + (1 - pi) * u["yes"] / u["n"]


def decide_tag(pi: dict[str, float], u: dict[str, int], lab: dict[str, int],
               neg: dict[str, int]) -> dict[str, Any]:
    """Section 9 for one tag: u counts the person's answers on the tagged puzzles without the
    theme, lab on the tagged puzzles with it, neg on the puzzles with neither."""
    pi_low, pi_up = pi_bounds(pi)
    b_low = cp_lower(u["yes"], u["n"])  # "unsure" counted as no
    b_up = cp_upper(u["yes"] + u["unsure"], u["n"])  # "unsure" counted as yes
    low, up = combine(pi_low, b_low), combine(pi_up, b_up)
    not_yes = lab["no"] + lab["unsure"]
    human = human_point(pi["estimate"], u, lab)
    false_yes_up = cp_upper(neg["yes"], neg["n"], FALSE_YES_ALPHA)
    guards = {
        "labelled": not_yes <= MAX_LABELLED_NOT_YES,
        "labelled_judged_too": human is not None and human >= FLOOR,
        "false_yes": false_yes_up <= FALSE_YES_MAX,
    }
    if low >= FLOOR and all(guards.values()):
        decision = "clears"
    elif up < FLOOR:
        decision = "fails"
    elif low >= FLOOR:
        failed = ", ".join(g for g, ok in guards.items() if not ok)
        decision = f"inconclusive: the bound clears, the guards do not ({failed})"
    else:
        decision = "inconclusive"
    b_hat = u["yes"] / u["n"] if u["n"] else None
    return {
        "decision": decision,
        "bound_low": round(low, 4),
        "bound_up": round(up, 4),
        "estimate": None if b_hat is None else round(combine(pi["estimate"], b_hat), 4),
        "b_estimate": None if b_hat is None else round(b_hat, 4),
        "b_bounds": [round(b_low, 4), round(b_up, 4)],
        "pi": round(pi["estimate"], 4),
        "pi_bounds": [round(pi_low, 4), round(pi_up, 4)],
        "labelled_not_yes": not_yes,
        "labelled_judged_too": None if human is None else round(human, 4),
        "false_yes": neg,
        "false_yes_upper": round(false_yes_up, 4),
        "guards": guards,
    }


def follow_up(tag: dict[str, Any]) -> bool:
    """Section 9's continuation rule: a second session is worth it for an inconclusive tag whose
    estimate ("unsure" as no) is near enough to a decision to reach one with twice the puzzles."""
    est = tag["estimate"]
    return (tag["decision"] == "inconclusive" and est is not None
            and (est >= FOLLOW_UP_IF_AT_LEAST or est <= FOLLOW_UP_IF_AT_MOST))


def secondary(items: list[dict[str, Any]], pis: dict[str, dict[str, float]]) -> dict[str, Any]:
    """Exploratory (section 4): never decides."""
    out: dict[str, Any] = {}
    for t in TAGS:
        u = tally(items, f"U:{t}", t)
        lab = tally(items, f"L:{t}", t)
        pi = pis[t]["estimate"]
        unsure_excluded = None
        if u["yes"] + u["no"]:
            unsure_excluded = round(combine(pi, u["yes"] / (u["yes"] + u["no"])), 4)
        all_human = None
        if u["n"] and lab["n"]:
            all_human = round(pi * lab["yes"] / lab["n"] + (1 - pi) * u["yes"] / u["n"], 4)
        dmiss = [it for it in items if "D:miss" in it["strata"] and t in it["themes"]]
        dnone = [it for it in items if "D:none" in it["strata"]]
        out[t] = {
            "estimate_unsure_excluded": unsure_excluded,
            "estimate_labelled_judged_too": all_human,
            "labelled": lab,
            "distractors_with_theme_yes": [sum(it["answers"][t] == "yes" for it in dmiss),
                                           len(dmiss)],
            "distractors_without_any_yes": [sum(it["answers"][t] == "yes" for it in dnone),
                                            len(dnone)],
        }
    return out


def cmd_run(out: Path) -> None:
    budget = Budget(BUDGET)
    key = read_input("key.json")
    answers = read_input("answers.json")
    if key.get("format") != KEY_FORMAT:
        raise SystemExit(f"key.json is not {KEY_FORMAT}")
    pis = exp7_precision(read_input("results.json"))
    items = done_items(key, answers)
    budget.spend("items_read", len(items))
    blocks_done = len({it["block"] for it in items})
    tags = {}
    for t in TAGS:
        u = tally(items, f"U:{t}", t)
        lab = tally(items, f"L:{t}", t)
        tags[t] = {"unlabelled": u, "labelled": lab,
                   **decide_tag(pis[t], u, lab, negatives(items, t))}
    # A diagnostic: the frame's share of tagged puzzles carrying the theme, against EXP-0007's pi.
    frame = key["frame"]["strata"]
    pi_frame = {t: round(frame[f"L:{t}"] / (frame[f"L:{t}"] + frame[f"U:{t}"]), 4) for t in TAGS}
    failed = blocks_done < MIN_BLOCKS
    results = {
        "experiment": EXP,
        "algorithm_id": key["algorithm_id"],
        "page_id": key["page_id"],
        "items_done": len(items),
        "blocks_done": blocks_done,
        "failed": failed,
        "failed_reason": f"fewer than {MIN_BLOCKS} whole blocks" if failed else None,
        "tags": tags,
        "decisions": {t: ("not decided: a failed result" if failed else tags[t]["decision"])
                      for t in TAGS},
        "secondary": secondary(items, pis),
        "diagnostics": {"pi_in_frame": pi_frame},
        "follow_up": {t: follow_up(tags[t]) for t in TAGS},
        "alpha": {"family": ALPHA, "looks": LOOKS, "tag_one_sided": ALPHA_TAG_ONE_SIDED,
                  "pi": ALPHA_PI, "audit": ALPHA_PART},
        "compute": budget.record(),
    }
    write_results(out, results)


# ---- section 6: the power of the rule ---------------------------------------------------------


def n_unlabelled(blocks: int) -> dict[str, int]:
    return {t: blocks * PER_BLOCK[f"U:{t}"] for t in TAGS}


def n_labelled(blocks: int) -> dict[str, int]:
    """hangingPiece and sacrifice get one a block; backRankMate and skewer one every other block,
    the odd blocks' turn going to backRankMate."""
    return {"hangingPiece": blocks, "sacrifice": blocks, "backRankMate": (blocks + 1) // 2,
            "skewer": blocks // 2}


def n_negatives(blocks: int, tag: str) -> int:
    """About how many items carry neither our tag nor the theme: the block's items less the
    tag's own unlabelled and labelled ones, less one distractor that may carry the theme."""
    per = sum(PER_BLOCK.values())
    return blocks * per - n_unlabelled(blocks)[tag] - n_labelled(blocks)[tag] - blocks // 2


def _session(n_u: int, n_l: int, n_neg: int, b: float, unsure: float, a_true: float,
             false_yes: float, rng: np.random.Generator, reps: int):
    """One session's counts. A tagged puzzle without the theme is the motif with probability b.
    The person answers "unsure" with probability `unsure` whatever the truth; otherwise "yes" on
    a motif, and on a puzzle without it "yes" with probability `false_yes` (a lenient rater),
    else "no". Labelled puzzles are judged "yes" with probability a_true; the puzzles with
    neither our tag nor the theme are taken to lack the motif."""
    p_yes = (1 - unsure) * (b + (1 - b) * false_yes)
    draws = rng.multinomial(n_u, [p_yes, 1 - unsure - p_yes, unsure], size=reps)
    lab_yes = rng.binomial(n_l, a_true, size=reps)
    neg_yes = rng.binomial(n_neg, (1 - unsure) * false_yes, size=reps)
    return draws[:, 0], draws[:, 2], lab_yes, neg_yes


def _decide_np(pi: dict[str, float], yes, uns, lab_yes, neg_yes, n_u: int, n_l: int,
               n_neg: int):
    pi_low, pi_up = pi_bounds(pi)
    lows = np.array([cp_lower(k, n_u) for k in range(n_u + 1)])
    ups = np.array([cp_upper(k, n_u) for k in range(n_u + 1)])
    fy = np.array([cp_upper(k, n_neg, FALSE_YES_ALPHA) for k in range(n_neg + 1)])
    low = 1 - (1 - pi_low) * (1 - lows[yes])
    up = 1 - (1 - pi_up) * (1 - ups[yes + uns])
    human = pi["estimate"] * lab_yes / n_l + (1 - pi["estimate"]) * yes / n_u
    guards = ((n_l - lab_yes) <= MAX_LABELLED_NOT_YES) & (human >= FLOOR) & (
        fy[neg_yes] <= FALSE_YES_MAX)
    return (low >= FLOOR) & guards, up < FLOOR


def simulate(pi: dict[str, float], p_true: float, n_u: int, n_l: int, n_neg: int,
             unsure: float, a_true: float, rng: np.random.Generator, false_yes: float = 0.0,
             reps: int = POWER_REPS) -> dict[str, float]:
    """The section 9 rule on simulated answers, with its follow-up: b is chosen so that
    pi + (1 - pi) b = p_true (b = 0 when p_true <= pi). Look 1 is this session; a tag the
    continuation rule sends on gets a second session of the same size, and look 2 decides on
    both sessions pooled, at the same level. Its error claims hold for the rater modelled here."""
    b = max(0.0, (p_true - pi["estimate"]) / (1 - pi["estimate"]))
    yes, uns, ly, ny = _session(n_u, n_l, n_neg, b, unsure, a_true, false_yes, rng, reps)
    clears1, fails1 = _decide_np(pi, yes, uns, ly, ny, n_u, n_l, n_neg)
    est = 1 - (1 - pi["estimate"]) * (1 - yes / n_u)
    go_on = ~clears1 & ~fails1 & ((est >= FOLLOW_UP_IF_AT_LEAST) | (est <= FOLLOW_UP_IF_AT_MOST))
    yes2, uns2, ly2, ny2 = _session(n_u, n_l, n_neg, b, unsure, a_true, false_yes, rng, reps)
    clears2, fails2 = _decide_np(pi, yes + yes2, uns + uns2, ly + ly2, ny + ny2, 2 * n_u,
                                 2 * n_l, 2 * n_neg)
    clears = clears1 | (go_on & clears2)
    fails = fails1 | (go_on & fails2)
    return {
        "b": round(b, 4),
        "look1_clears": float(clears1.mean()),
        "look1_fails": float(fails1.mean()),
        "follow_up_share": float(go_on.mean()),
        "clears": float(clears.mean()),
        "fails": float(fails.mean()),
        "inconclusive": float(1 - clears.mean() - fails.mean()),
    }


def cmd_power() -> None:
    pis = exp7_precision(json.loads(EXP7_RESULTS.read_text(encoding="utf-8")))
    rng = np.random.default_rng(seed(EXP, "power/simulation"))
    truths = (0.70, 0.75, 0.80, 0.85, 0.90, 0.95, 0.98)
    result: dict[str, Any] = {
        "reps": POWER_REPS,
        "design": {"blocks": BLOCKS, "per_block": PER_BLOCK, "items": N_ITEMS,
                   "labelled": n_labelled(BLOCKS),
                   "negatives": {t: n_negatives(BLOCKS, t) for t in TAGS},
                   "minutes_at_40s": N_ITEMS * SECONDS_PER_ITEM / 60},
        "threshold_b": {},
        "main": {},
        "sensitivity": {},
        "lenient_rater": {},
        "n_curve": {},
    }
    for t in TAGS:
        pi_low, _ = pi_bounds(pis[t])
        result["threshold_b"][t] = round((FLOOR - pi_low) / (1 - pi_low), 4)
    for blocks, label in ((BLOCKS, "all blocks"), (MIN_BLOCKS, "minimum blocks")):
        nu, nl = n_unlabelled(blocks), n_labelled(blocks)
        result["main"][label] = {
            t: {str(p): simulate(pis[t], p, nu[t], nl[t], n_negatives(blocks, t), 0.05, 1.0, rng)
                for p in truths}
            for t in TAGS
        }
    nu, nl = n_unlabelled(BLOCKS), n_labelled(BLOCKS)
    for unsure in (0.0, 0.10, 0.20):
        for a_true in (1.0, 0.95, 0.85):
            k = f"unsure {unsure}, labelled yes {a_true}"
            result["sensitivity"][k] = {
                t: {str(p): simulate(pis[t], p, nu[t], nl[t], n_negatives(BLOCKS, t), unsure,
                                     a_true, rng)["clears"]
                    for p in (0.80, 0.90, 0.95)}
                for t in TAGS
            }
    for fy in (0.05, 0.10, 0.20):
        result["lenient_rater"][f"false yes {fy}"] = {
            t: {str(p): simulate(pis[t], p, nu[t], nl[t], n_negatives(BLOCKS, t), 0.05, 1.0,
                                 rng, false_yes=fy)["clears"]
                for p in (0.70, 0.75, 0.80, 0.90)}
            for t in TAGS
        }
    for t in TAGS:
        result["n_curve"][t] = {
            str(n): {str(p): simulate(pis[t], p, n, 6, 100, 0.05, 1.0, rng)["look1_clears"]
                     for p in (0.80, 0.90, 0.95)}
            for n in (5, 10, 15, 20, 25, 30, 40, 50, 60, 80)
        }
    (HERE / "power.json").write_text(json.dumps(result, indent=2, sort_keys=True) + "\n",
                                     encoding="utf-8")
    print(json.dumps({"threshold_b": result["threshold_b"], "main": result["main"],
                      "lenient_rater": result["lenient_rater"]}, indent=1))


def main(argv: list[str] | None = None) -> int:
    args = list(sys.argv[1:] if argv is None else argv)
    if args and args[0] == "power":
        cmd_power()
        return 0
    p = argparse.ArgumentParser(prog="analysis.py")
    p.add_argument("--out", type=Path, required=True)
    cmd_run(p.parse_args(args).out)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

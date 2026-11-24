"""The champion/challenger arena (rematch_lab.arena, ADR 0109): the interval rules, each live
experiment's own comparison, the plan and its budget, the leaderboard and the overrides."""

from __future__ import annotations

import copy
import hashlib
import json
from pathlib import Path

import pytest

from rematch_lab import arena, experiments, stats

LAB = Path(__file__).resolve().parent.parent
CHAMPION = "algo-1.1.0+fa5e2186e1dd"
CHALLENGER = "algo-2.0.0+222222222222"


# ---- the interval rules -----------------------------------------------------------------------


def test_wilson_matches_the_committed_recall_interval() -> None:
    # EXP-0001's recall at 2 at 80 games: 19 of 30, Wilson 95% [0.455136, 0.781261].
    lo, hi = stats.wilson(19, 30, 0.95)
    assert (round(lo, 6), round(hi, 6)) == (0.455136, 0.781261)


def test_newcombe_paired_of_a_version_with_itself_straddles_zero() -> None:
    d, lo, hi = stats.newcombe_paired(n11=19, n10=0, n01=0, n00=11, level=0.975)
    assert d == 0 and lo < 0 < hi


def test_newcombe_paired_sees_a_clear_gain() -> None:
    # The challenger finds 12 flaws the champion missed and loses none.
    d, lo, hi = stats.newcombe_paired(n11=15, n10=12, n01=0, n00=3, level=0.975)
    assert d == pytest.approx(0.4) and lo > 0


def test_mover_difference_of_equal_estimates_straddles_zero() -> None:
    d, lo, hi = stats.mover(0.3, (0.25, 0.35), 0.3, (0.25, 0.35))
    assert d == 0 and lo < 0 < hi
    d, lo, hi = stats.mover(0.6, (0.55, 0.65), 0.3, (0.25, 0.35))
    assert d == pytest.approx(0.3) and lo > 0


def test_distance_interval_from_a_coverage_interval() -> None:
    d, ci = stats.distance_interval(0.1, (0.08, 0.12), 0.8)
    assert (d, *ci) == pytest.approx((0.7, 0.68, 0.72))
    d, ci = stats.distance_interval(0.87, (0.86, 0.88), 0.8)
    assert (d, *ci) == pytest.approx((0.07, 0.06, 0.08))
    d, (lo, hi) = stats.distance_interval(0.79, (0.75, 0.83), 0.8)
    assert d == pytest.approx(0.01) and lo == 0 and hi == pytest.approx(0.05)


def test_verdicts_follow_the_better_direction() -> None:
    assert stats.verdict(0.01, 0.2, "higher") == "better"
    assert stats.verdict(-0.2, -0.01, "higher") == "worse"
    assert stats.verdict(-0.1, 0.1, "higher") == "no clear difference"
    assert stats.verdict(0.01, 0.2, "lower") == "worse"
    assert stats.verdict(-0.2, -0.01, "lower") == "better"
    assert stats.verdict(0.0, 0.0, "higher") == "no clear difference"


# ---- EXP-0001 and EXP-0002's own rules on their committed results -----------------------------


def committed(exp_id: str) -> dict:
    return json.loads((LAB / "experiments" / exp_id / "results.json").read_text(encoding="utf-8"))


@pytest.mark.parametrize("exp_id", ["EXP-0001", "EXP-0002"])
def test_a_version_against_itself_shows_no_clear_difference(exp_id: str) -> None:
    rule = arena.load_rule(LAB / "experiments" / exp_id)
    m = rule.measure(committed(exp_id))
    result = rule.compare(m, m)
    assert {r["verdict"] for r in result.values()} == {"no clear difference"}
    assert set(result) == {x["name"] for x in rule.METRICS}
    assert any(x["role"] == "primary" for x in rule.METRICS)


def test_exp_0001_metrics_reproduce_the_decision() -> None:
    rule = arena.load_rule(LAB / "experiments" / "EXP-0001")
    metrics = rule.metrics(rule.measure(committed("EXP-0001")))
    assert metrics["recall_at_2"]["value"] == pytest.approx(19 / 30)
    assert metrics["recall_at_2"]["interval"] == pytest.approx([0.455136, 0.781261], abs=1e-6)
    assert metrics["coverage"]["value"] == pytest.approx(0.575)
    assert metrics["clean_false_alarms"]["value"] == pytest.approx(2 / 8)


def test_exp_0001_a_challenger_that_loses_hits_is_worse() -> None:
    rule = arena.load_rule(LAB / "experiments" / "EXP-0001")
    champ = rule.measure(committed("EXP-0001"))
    chall = copy.deepcopy(champ)
    for p in chall["flawed"].values():
        p["hit2"] = False
    result = rule.compare(champ, chall)
    assert result["recall_at_2"]["verdict"] == "worse"
    assert result["recall_at_2"]["effect"] == pytest.approx(-19 / 30)


def test_exp_0001_refuses_players_that_do_not_pair() -> None:
    rule = arena.load_rule(LAB / "experiments" / "EXP-0001")
    champ = rule.measure(committed("EXP-0001"))
    chall = copy.deepcopy(champ)
    chall["flawed"].pop(next(iter(chall["flawed"])))
    with pytest.raises(ValueError, match="same players"):
        rule.compare(champ, chall)


def test_exp_0002_metrics_reproduce_the_decision() -> None:
    rule = arena.load_rule(LAB / "experiments" / "EXP-0002")
    metrics = rule.metrics(rule.measure(committed("EXP-0002")))
    assert metrics["worst_coverage"]["value"] == pytest.approx(0.100278)
    assert metrics["worst_coverage"]["at"] == "W4.missed at 30 games"
    assert metrics["rho1_coverage"]["value"] == pytest.approx(0.869583)


def test_exp_0002_a_challenger_whose_worst_cell_improves_is_better() -> None:
    rule = arena.load_rule(LAB / "experiments" / "EXP-0002")
    champ = rule.measure(committed("EXP-0002"))
    chall = copy.deepcopy(champ)
    for c in chall["cells"]:
        c["coverage"], c["interval"] = 0.8, [0.78, 0.82]
    result = rule.compare(champ, chall)
    assert result["worst_coverage"]["verdict"] == "better"
    worse = copy.deepcopy(champ)
    worse["rho1"] = {"coverage": 0.99, "interval": [0.985, 0.995]}
    assert rule.compare(champ, worse)["rho1_coverage"]["verdict"] == "worse"


# ---- the arena around a fake experiment -------------------------------------------------------

FAKE_ANALYSIS = """\
import argparse, json
from pathlib import Path
from rematch_lab.experiments import write_results
p = argparse.ArgumentParser(); p.add_argument("--out", required=True)
p.add_argument("--algorithm"); p.add_argument("--seed-namespace", default="EXP-9999")
a = p.parse_args()
hits = json.loads((Path(__file__).parent / "hits.json").read_text())[a.algorithm]
write_results(a.out, {"algorithm_id": a.algorithm, "hits": hits,
                      "settings": {"seed_namespace": a.seed_namespace}})
"""

FAKE_RULE = """\
from rematch_lab import stats
METRICS = [{"name": "hits", "role": "primary", "label": "Hits", "better": "higher"},
           {"name": "misses", "role": "guard", "label": "Misses", "better": "lower"}]
def measure(results):
    return {"hits": results["hits"]}
def metrics(m):
    n = len(m["hits"]); k = sum(m["hits"])
    return {"hits": {"value": k / n, "interval": list(stats.wilson(k, n, 0.95)), "level": 0.95},
            "misses": {"value": 1 - k / n, "interval": None, "level": None}}
def compare(champion, challenger):
    pairs = list(zip(champion["hits"], challenger["hits"], strict=True))
    n11 = sum(1 for a, b in pairs if a and b); n10 = sum(1 for a, b in pairs if b and not a)
    n01 = sum(1 for a, b in pairs if a and not b); n00 = len(pairs) - n11 - n10 - n01
    d, lo, hi = stats.newcombe_paired(n11, n10, n01, n00, 0.975)
    return {"hits": {"effect": d, "interval": [lo, hi], "level": 0.975,
                     "verdict": stats.verdict(lo, hi, "higher")},
            "misses": {"effect": -d, "interval": [-hi, -lo], "level": 0.975,
                       "verdict": stats.verdict(-hi, -lo, "lower")}}
"""

SUMMARY = """\
---
id: EXP-9999
title: A fake
status: decided
algorithm: {champion}
issue: 1
updated: 2026-11-24
live: true
rerun: {rerun}
run_cpu_hours: {cpu}
run_minutes: 1
---

- A bullet.
"""


def fake(root: Path, hits: dict[str, list[int]], rerun: str = "per-pr", cpu: float = 0.1) -> Path:
    exp = root / "EXP-9999"
    exp.mkdir(parents=True)
    (exp / "analysis.py").write_text(FAKE_ANALYSIS, encoding="utf-8")
    (exp / "arena.py").write_text(FAKE_RULE, encoding="utf-8")
    (exp / "hits.json").write_text(json.dumps(hits), encoding="utf-8")
    (exp / "summary.md").write_text(
        SUMMARY.format(champion=CHAMPION, rerun=rerun, cpu=cpu), encoding="utf-8"
    )
    return exp


@pytest.fixture
def lab_dirs(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> Path:
    monkeypatch.setattr(experiments, "EXPERIMENTS", tmp_path / "experiments")
    (tmp_path / "experiments").mkdir()
    return tmp_path


SAME = {CHAMPION: [1] * 20 + [0] * 10, CHALLENGER: [1] * 20 + [0] * 10}
WORSE = {CHAMPION: [1] * 20 + [0] * 10, CHALLENGER: [0] * 30}


def run(lab_dirs: Path, *extra: str) -> tuple[int, dict]:
    board = lab_dirs / "leaderboard.json"
    out = lab_dirs / "arena.json"
    code = arena.main(
        [
            "--champion",
            CHAMPION,
            "--challenger",
            CHALLENGER,
            "--leaderboard",
            str(board),
            "--overrides",
            str(lab_dirs / "overrides.json"),
            "--json",
            str(out),
            "--date",
            "2026-11-24",
            *extra,
        ]
    )
    return code, json.loads(out.read_text(encoding="utf-8"))


def test_scopes_include_the_cheaper_reruns(lab_dirs: Path) -> None:
    fake(lab_dirs / "experiments", SAME, rerun="weekly")
    assert arena.live_experiments("per-pr") == []
    assert [x.id for x in arena.live_experiments("weekly")] == ["EXP-9999"]
    assert [x.id for x in arena.live_experiments("release")] == ["EXP-9999"]


def test_a_dry_run_plans_without_running(lab_dirs: Path) -> None:
    fake(lab_dirs / "experiments", SAME)
    code, report = run(lab_dirs, "--dry-run")
    assert code == 0
    assert [(p["experiment"], p["algorithm_id"]) for p in report["plan"]] == [
        ("EXP-9999", CHAMPION),
        ("EXP-9999", CHALLENGER),
    ]
    assert report["experiments"][0]["verdict"] == "not run"
    assert not (lab_dirs / "leaderboard.json").exists()


def test_a_run_writes_both_records_and_reuses_them(lab_dirs: Path) -> None:
    fake(lab_dirs / "experiments", SAME)
    code, report = run(lab_dirs, "--write")
    assert code == 0
    assert report["experiments"][0]["verdict"] == "no clear difference"
    board = json.loads((lab_dirs / "leaderboard.json").read_text(encoding="utf-8"))
    ids = sorted(r["algorithm_id"] for r in board["records"])
    assert ids == [CHAMPION, CHALLENGER]
    chall = next(r for r in board["records"] if r["algorithm_id"] == CHALLENGER)
    assert chall["against"] == CHAMPION and chall["verdict"] == "no clear difference"
    assert chall["metrics"]["hits"]["value"] == pytest.approx(20 / 30)
    assert chall["date"] == "2026-11-24"
    # The metrics' roles and labels, for the site's leaderboard.
    assert board["specs"]["EXP-9999"][0] == {
        "name": "hits",
        "role": "primary",
        "label": "Hits",
        "better": "higher",
    }
    # The second run reuses both records: nothing to plan.
    code, again = run(lab_dirs, "--dry-run")
    assert code == 0 and again["plan"] == []
    assert again["experiments"][0]["verdict"] == "no clear difference"


def test_a_clear_regression_fails(lab_dirs: Path) -> None:
    fake(lab_dirs / "experiments", WORSE)
    code, report = run(lab_dirs)
    assert code == 1
    assert report["experiments"][0]["verdict"] == "worse"
    assert report["experiments"][0]["comparison"]["hits"]["verdict"] == "worse"


def test_an_override_recorded_in_an_adr_lets_it_pass(lab_dirs: Path) -> None:
    fake(lab_dirs / "experiments", WORSE)
    adr = lab_dirs / "0999-override.md"
    adr.write_text("# 0999. An emergency\n", encoding="utf-8")
    overrides = [
        {
            "algorithm_id": CHALLENGER,
            "experiment": "EXP-9999",
            "metrics": ["hits", "misses"],
            "adr": str(adr),
            "reason": "a security fix that cannot wait",
        }
    ]
    (lab_dirs / "overrides.json").write_text(json.dumps(overrides), encoding="utf-8")
    code, report = run(lab_dirs)
    assert code == 0
    assert report["experiments"][0]["verdict"] == "overridden"
    # An override whose ADR does not exist is refused.
    overrides[0]["adr"] = str(lab_dirs / "missing.md")
    (lab_dirs / "overrides.json").write_text(json.dumps(overrides), encoding="utf-8")
    assert run(lab_dirs)[0] == 1


def test_the_plan_is_refused_past_the_budget(lab_dirs: Path) -> None:
    fake(lab_dirs / "experiments", SAME, cpu=0.8)
    code, report = run(lab_dirs, "--max-cpu-hours", "1")
    assert code == 2
    assert "budget" in report["refused"]
    assert not (lab_dirs / "leaderboard.json").exists()


def test_verify_reruns_the_challenger_and_compares_the_committed_record(lab_dirs: Path) -> None:
    exp = fake(lab_dirs / "experiments", SAME)
    assert run(lab_dirs, "--write")[0] == 0
    assert run(lab_dirs, "--verify", "--require-records")[0] == 0
    # The algorithm now gives a different result than the committed record says.
    hits = dict(SAME)
    hits[CHALLENGER] = [1] * 21 + [0] * 9
    (exp / "hits.json").write_text(json.dumps(hits), encoding="utf-8")
    code, report = run(lab_dirs, "--verify")
    assert code == 3
    assert any("differs from its committed record" in p for p in report["problems"])


def test_require_records_asks_for_the_challenger_on_the_leaderboard(lab_dirs: Path) -> None:
    fake(lab_dirs / "experiments", SAME)
    code, report = run(lab_dirs, "--require-records")
    assert code == 3
    assert any("not on the leaderboard" in p for p in report["problems"])


def test_records_on_other_seeds_or_code_are_not_reused(lab_dirs: Path) -> None:
    exp = fake(lab_dirs / "experiments", SAME)
    assert run(lab_dirs, "--write")[0] == 0
    (exp / "analysis.py").write_text(FAKE_ANALYSIS + "# changed\n", encoding="utf-8")
    code, report = run(lab_dirs, "--dry-run")
    assert len(report["plan"]) == 2


def test_the_committed_results_are_the_champions_cache() -> None:
    # EXP-0001 and EXP-0002 ran on the champion; their results.json is reused, never re-run.
    for live in arena.live_experiments("release"):
        record = arena.committed_record(live, CHAMPION)
        assert record is not None and record["seed_namespace"] == live.id
        digest = hashlib.sha256((live.exp / "results.json").read_bytes()).hexdigest()
        assert record["results_sha256"] == digest


def test_the_seeded_leaderboard_is_what_the_committed_results_give() -> None:
    board = arena.load_board(LAB / "leaderboard.json")
    for live in arena.live_experiments("release"):
        stored = arena.find_record(board, live, CHAMPION)
        assert stored is not None, live.id
        assert arena.same_record(stored, arena.committed_record(live, CHAMPION))

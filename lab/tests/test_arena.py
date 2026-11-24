"""The champion/challenger arena (rematch_lab.arena, ADR 0109): the interval rules, each live
experiment's own comparison, the plan and its budget, the leaderboard and the overrides."""

from __future__ import annotations

import copy
import hashlib
import json
import subprocess
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


@pytest.mark.parametrize("exp_id", ["EXP-0001", "EXP-0002", "EXP-0007"])
def test_a_version_against_itself_shows_no_clear_difference(exp_id: str) -> None:
    rule = arena.load_rule(LAB / "experiments" / exp_id)
    m = rule.measure(committed(exp_id))
    result = rule.compare(m, m)
    assert {r["verdict"] for r in result.values()} == {"no clear difference"}
    assert set(result) == {x["name"] for x in rule.METRICS}
    assert any(x["role"] == "primary" for x in rule.METRICS)


def test_exp_0007_metrics_reproduce_the_decision() -> None:
    rule = arena.load_rule(LAB / "experiments" / "EXP-0007")
    metrics = rule.metrics(rule.measure(committed("EXP-0007")))
    assert metrics["trusted_tags"]["tags"] == [
        "fork",
        "discoveredAttack",
        "promotion",
        "smotheredMate",
        "mate",
        "mateIn1",
        "mateIn2",
        "mateIn3",
        "mateIn4Plus",
    ]
    assert metrics["precision_fork"]["value"] == pytest.approx(0.9763)
    assert metrics["precision_fork"]["interval"] == pytest.approx([0.9691, 0.9819])
    assert metrics["recall_pin"]["value"] == pytest.approx(0.201, abs=1e-3)


def test_exp_0007_a_trusted_tag_may_not_fall_below_the_precision_floor() -> None:
    rule = arena.load_rule(LAB / "experiments" / "EXP-0007")
    champ = rule.measure(committed("EXP-0007"))
    chall = copy.deepcopy(champ)
    # fork's estimate holds, but the lower end of its interval drops to 0.79: no longer trusted.
    chall["fork"]["precision"]["interval"] = [0.79, 0.99]
    chall["fork"]["decision"] = "hidden pending a human audit"
    result = rule.compare(champ, chall)
    assert result["precision_fork"]["verdict"] == "worse"
    assert result["trusted_tags"]["verdict"] == "worse"
    assert result["trusted_tags"]["lost"] == ["fork"]
    roles = {m["name"]: m["role"] for m in rule.METRICS}
    assert roles["trusted_tags"] == "primary" and roles["precision_fork"] == "guard"
    assert roles["recall_fork"] == "report"


def test_exp_0007_a_tag_clears_only_by_the_registered_rule() -> None:
    rule = arena.load_rule(LAB / "experiments" / "EXP-0007")
    champ = rule.measure(committed("EXP-0007"))
    # skewer's precision interval clears 0.80, but the frozen rule did not trust it: no gain.
    raised = copy.deepcopy(champ)
    raised["skewer"]["precision"]["interval"] = [0.81, 0.85]
    result = rule.compare(champ, raised)
    assert result["trusted_tags"]["verdict"] == "no clear difference"
    assert result["precision_skewer"]["verdict"] == "better"
    # Once the rule trusts it (precision and recall both clear), the primary gains it.
    raised["skewer"]["decision"] = "trusted"
    result = rule.compare(champ, raised)
    assert result["trusted_tags"]["verdict"] == "better"
    assert result["trusted_tags"]["gained"] == ["skewer"]


def test_exp_0007_runs_without_the_algorithm_flag_on_its_own_namespace() -> None:
    rule = arena.load_rule(LAB / "experiments" / "EXP-0007")
    assert rule.ALGORITHM_FLAG is False
    live = next(x for x in arena.live_experiments("release") if x.id == "EXP-0007")
    assert (live.rerun, live.cpu_hours, live.minutes) == ("release", 0.02, 1.0)
    record = arena.committed_record(live, CHAMPION)
    assert record is not None and record["seed_namespace"] == "EXP-0007"


def test_exp_0001_metrics_reproduce_the_decision() -> None:
    rule = arena.load_rule(LAB / "experiments" / "EXP-0001")
    metrics = rule.metrics(rule.measure(committed("EXP-0001")))
    assert metrics["recall_at_2"]["value"] == pytest.approx(19 / 30)
    assert metrics["recall_at_2"]["interval"] == pytest.approx([0.455136, 0.781261], abs=1e-6)
    assert metrics["coverage"]["value"] == pytest.approx(0.575)
    assert metrics["clean_false_alarms"]["value"] == pytest.approx(1.0)  # both clean controls


def test_exp_0001_a_challenger_that_loses_hits_is_worse() -> None:
    rule = arena.load_rule(LAB / "experiments" / "EXP-0001")
    champ = rule.measure(committed("EXP-0001"))
    chall = copy.deepcopy(champ)
    for p in chall["flawed"].values():
        p["hit2"] = False
    result = rule.compare(champ, chall)
    assert result["recall_at_2"]["verdict"] == "worse"
    assert result["recall_at_2"]["effect"] == pytest.approx(-19 / 30)


def flip(m: dict, n: int, to: bool) -> dict:
    """A copy of EXP-0001's measures with n players' hit at 80 games set to `to`."""
    out = copy.deepcopy(m)
    for k in [k for k, p in out["flawed"].items() if p["hit2"] != to][:n]:
        out["flawed"][k]["hit2"] = to
    return out


def test_exp_0001_worse_uses_the_registered_guard_and_better_the_ship_rule() -> None:
    rule = arena.load_rule(LAB / "experiments" / "EXP-0001")
    champ = rule.measure(committed("EXP-0001"))
    # Four lost detections: the 80% guard interval (ADR 0104) lies below 0, the 97.5% ship
    # interval does not.
    lost = rule.compare(champ, flip(champ, 4, False))["recall_at_2"]
    assert lost["verdict"] == "worse"
    assert lost["guard_interval"][1] < 0 < lost["interval"][1]
    # Four gained: not enough for the registered ship rule.
    gained = rule.compare(champ, flip(champ, 4, True))["recall_at_2"]
    assert gained["verdict"] == "no clear difference"


def test_exp_0001_clean_false_alarms_are_reported_not_gated() -> None:
    rule = arena.load_rule(LAB / "experiments" / "EXP-0001")
    roles = {m["name"]: m["role"] for m in rule.METRICS}
    assert roles == {"recall_at_2": "primary", "coverage": "guard", "clean_false_alarms": "report"}


def test_exp_0001_coverage_guard_has_the_registered_margin() -> None:
    rule = arena.load_rule(LAB / "experiments" / "EXP-0001")
    champ = rule.measure(committed("EXP-0001"))
    # Three of 120 intervals fewer hold the truth: 0.025 further from 0.8, inside the 0.05 margin.
    near = copy.deepcopy(champ)
    left = 3
    for p in near["flawed"].values():
        for i, c in enumerate(p["covered"]):
            if c and left:
                p["covered"][i], left = False, left - 1
    assert rule.compare(champ, near)["coverage"]["verdict"] != "worse"
    none = copy.deepcopy(champ)
    for p in none["flawed"].values():
        p["covered"] = [False] * len(p["covered"])
    assert rule.compare(champ, none)["coverage"]["verdict"] == "worse"


def test_exp_0002_worst_coverage_interval_is_the_interval_of_the_maximum() -> None:
    rule = arena.load_rule(LAB / "experiments" / "EXP-0002")
    m = rule.measure(committed("EXP-0002"))
    metric = rule.metrics(m)["worst_coverage"]
    dists = [stats.distance_interval(c["coverage"], c["interval"], 0.8)[1] for c in m["cells"]]
    want = [max(d[0] for d in dists), max(d[1] for d in dists)]
    assert metric["distance_interval"] == pytest.approx(want, abs=1e-6)


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
    (lab_dirs / "decisions").mkdir()
    adr = lab_dirs / "decisions" / "0999-override.md"
    adr.write_text(
        f"# 0999. An emergency\n\nShip {CHALLENGER} despite EXP-9999's hits and misses.\n",
        encoding="utf-8",
    )
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
    # Refused: an ADR that does not exist, a file that is not an ADR, an ADR that does not name
    # what it overrides.
    readme = lab_dirs / "README.md"
    readme.write_text(adr.read_text(encoding="utf-8"), encoding="utf-8")
    for bad in (lab_dirs / "decisions" / "0998-missing.md", readme):
        overrides[0]["adr"] = str(bad)
        (lab_dirs / "overrides.json").write_text(json.dumps(overrides), encoding="utf-8")
        assert run(lab_dirs)[0] == 1, bad.name
    adr.write_text("# 0999. An emergency\n", encoding="utf-8")
    overrides[0]["adr"] = str(adr)
    (lab_dirs / "overrides.json").write_text(json.dumps(overrides), encoding="utf-8")
    assert run(lab_dirs)[0] == 1


def test_a_report_only_metric_never_fails_the_run(lab_dirs: Path) -> None:
    exp = fake(lab_dirs / "experiments", WORSE)
    rule = (exp / "arena.py").read_text(encoding="utf-8")
    rule = rule.replace('"role": "primary"', '"role": "report"')
    (exp / "arena.py").write_text(rule.replace('"role": "guard"', '"role": "report"'))
    code, report = run(lab_dirs)
    assert code == 0
    assert report["experiments"][0]["comparison"]["hits"]["verdict"] == "worse"
    assert report["experiments"][0]["verdict"] == "no clear difference"


def test_champion_records_come_from_the_champions_own_board(lab_dirs: Path) -> None:
    fake(lab_dirs / "experiments", SAME)
    assert run(lab_dirs, "--write")[0] == 0
    # The champion's board (main's, in a pull request) has no record: the working board's
    # record of the champion is not trusted.
    main_board = lab_dirs / "main-board.json"
    main_board.write_text(json.dumps({"schema": "leaderboard-v1", "specs": {}, "records": []}))
    code, report = run(lab_dirs, "--dry-run", "--champion-board", str(main_board))
    assert [p["algorithm_id"] for p in report["plan"]] == [CHAMPION]


def test_an_edited_rule_is_not_fed_stale_records(lab_dirs: Path) -> None:
    exp = fake(lab_dirs / "experiments", SAME)
    assert run(lab_dirs, "--write")[0] == 0
    (exp / "arena.py").write_text(FAKE_RULE + "# changed\n", encoding="utf-8")
    code, report = run(lab_dirs, "--dry-run")
    assert len(report["plan"]) == 2


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


# ---- external inputs and reviewed source changes (ADR 0109 Amendment 1) ------------------------


def with_external(exp: Path) -> None:
    (exp / "inputs.sha256").write_text(f"{'0' * 64}  external:puzzles/snapshot.csv\n")


def test_a_missing_external_input_is_reported_not_run_never_passed(
    lab_dirs: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setenv("REMATCH_DATA_DIR", str(lab_dirs / "data"))
    with_external(fake(lab_dirs / "experiments", WORSE))
    code, report = run(lab_dirs)
    assert code == 0 and report["plan"] == []
    verdict = report["experiments"][0]["verdict"]
    assert verdict == "not run: external input missing (puzzles/snapshot.csv)"
    assert report["not_run"] == {"EXP-9999": verdict}
    assert "Not run for want of data: EXP-9999" in arena.render(report)
    code, report = run(lab_dirs, "--require-external")
    assert code == 3
    assert any("external input missing" in p for p in report["problems"])


def test_verify_without_the_external_input_compares_the_committed_record(
    lab_dirs: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setenv("REMATCH_DATA_DIR", str(lab_dirs / "data"))
    exp = fake(lab_dirs / "experiments", SAME)
    # Both records are written where the snapshot is; then it is gone, as on a hosted runner.
    snapshot = lab_dirs / "data" / "puzzles" / "snapshot.csv"
    snapshot.parent.mkdir(parents=True)
    snapshot.write_text("PuzzleId\n")
    digest = hashlib.sha256(b"PuzzleId\n").hexdigest()
    (exp / "inputs.sha256").write_text(f"{digest}  external:puzzles/snapshot.csv\n")
    assert run(lab_dirs, "--write")[0] == 0
    snapshot.unlink()
    code, report = run(lab_dirs, "--verify")
    assert code == 0 and report["plan"] == []
    assert report["experiments"][0]["verdict"] == "no clear difference"
    assert any("not re-run" in n for n in report["notes"])


def git(repo: Path, *args: str) -> str:
    return subprocess.run(
        ["git", "-C", str(repo), "-c", "user.name=t", "-c", "user.email=t@t", *args],
        capture_output=True,
        text=True,
        check=True,
    ).stdout.strip()


def test_a_source_change_is_refused_unless_its_exact_content_is_reviewed(
    lab_dirs: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    repo = lab_dirs / "repo"
    (repo / "src").mkdir(parents=True)
    (repo / "algorithm").mkdir()
    (repo / "algorithm" / "manifest.json").write_text('{"id": "x"}\n')
    (repo / "src" / "players.rs").write_text("players\n")
    (repo / "src" / "harness.rs").write_text("harness\n")
    git(repo, "init", "-q")
    git(repo, "add", ".")
    git(repo, "commit", "-qm", "champion")
    champion_ref = git(repo, "rev-parse", "HEAD")
    (repo / "src" / "harness.rs").write_text("harness\n// additive\n")
    monkeypatch.setattr(arena, "REPO_ROOT", repo)
    exp = fake(lab_dirs / "experiments", SAME)
    (exp / "arena.py").write_text(FAKE_RULE + 'SOURCE_PATHS = ["src"]\n', encoding="utf-8")
    reviews = lab_dirs / "reviews.json"
    args = ("--dry-run", "--champion-ref", champion_ref, "--source-reviews", str(reviews))

    code, report = run(lab_dirs, *args)
    assert code == 3
    assert any("src/harness.rs changed" in p for p in report["problems"])

    (repo / "docs" / "decisions").mkdir(parents=True)
    adr = repo / "docs" / "decisions" / "0999-review.md"
    adr.write_text("# 0999. EXP-9999 never calls the addition to src/harness.rs\n")
    blob = git(repo, "hash-object", "--", "src/harness.rs")
    entry = {
        "experiment": "EXP-9999",
        "path": "src/harness.rs",
        "blob": blob,
        "adr": "docs/decisions/0999-review.md",
        "reason": "additive",
    }
    reviews.write_text(json.dumps([entry]))
    code, report = run(lab_dirs, *args)
    assert code == 0 and report["problems"] == []
    assert any("accepted by its review" in n for n in report["notes"])

    # Another edit of the file, a review of another experiment, or an ADR that does not name the
    # file: refused again.
    for bad in ({**entry, "blob": "0" * 40}, {**entry, "experiment": "EXP-0001"}):
        reviews.write_text(json.dumps([bad]))
        assert run(lab_dirs, *args)[0] == 3
    reviews.write_text(json.dumps([entry]))
    adr.write_text("# 0999. A review\n")
    assert run(lab_dirs, *args)[0] == 3

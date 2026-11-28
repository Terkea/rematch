"""The experiment rules (rematch_lab.experiments): registered first, frozen, checked inputs,
deterministic, worth running, and live or retired once decided (ADR 0109)."""

from __future__ import annotations

import hashlib
import json
import subprocess
from pathlib import Path

import pytest
from conftest import clean_git_env

from rematch_lab import experiments

ANALYSIS = """\
import argparse, random
from rematch_lab.experiments import Budget, seed, write_results
p = argparse.ArgumentParser(); p.add_argument("--out", required=True); a = p.parse_args()
rng = random.Random(seed("EXP-9999", "draws"))
budget = Budget({"draws": 100})
draws = []
for _ in range(100):
    budget.spend("draws")
    draws.append(rng.random())
write_results(a.out, {"algorithm_id": "unversioned", "mean": sum(draws) / 100,
                      "compute": budget.record()})
"""

# An experiment that drives the real algorithm through the bridge, as every experiment must.
BRIDGE_ANALYSIS = """\
import argparse, json
from pathlib import Path
import rematch_lab
from rematch_lab.experiments import Budget, write_results
p = argparse.ArgumentParser(); p.add_argument("--out", required=True); a = p.parse_args()
fixture = Path(rematch_lab.__file__).parent.parent / "fixtures" / "profile-request.json"
facts = rematch_lab.models.ProfileRequest.model_validate_json(fixture.read_text())
budget = Budget({"bridge_calls": 2})
budget.spend("bridge_calls", 2)
ranking = rematch_lab.rank(rematch_lab.profile(facts))
write_results(a.out, {"algorithm_id": ranking.algorithm_id,
                      "focuses": [p.focuses for p in ranking.players],
                      "compute": budget.record()})
"""


VALUE_CASE = (
    "## 0. Value case\n\n"
    "- Decision it informs: whether the rate rises.\n"
    "- If the result is a rise, we will act; if not, we will not.\n"
    "- Expected cost: a second of CPU.\n"
    "- Value score: impact 1 × uncertainty 3 ÷ cost 1 = 3\n"
    "- Cheaper alternative? No.\n"
    "- Compute budget: 100 draws; cap 150%\n\n"
)


def prereg(analysis: str, h1: str = "H1: the rate rises.") -> str:
    digest = hashlib.sha256(analysis.encode()).hexdigest()
    return (
        "# EXP-9999: a test\n\n- Status: pre-registered\n\n"
        f"{VALUE_CASE}"
        f"## 1. Question\n\nq\n\n## 2. Hypotheses\n\n{h1}\n\n"
        f"## 10. Analysis code\n\n- Analysis code sha256: `{digest}`\n\n"
        "## 11. Deviations\n\nNone.\n"
    )


def make(root: Path, analysis: str = ANALYSIS) -> Path:
    exp = root / "EXP-9999"
    exp.mkdir(parents=True)
    (exp / "analysis.py").write_text(analysis, encoding="utf-8")
    (exp / "preregistration.md").write_text(prereg(analysis), encoding="utf-8")
    return exp


def git(cwd: Path, *args: str) -> None:
    subprocess.run(
        ["git", "-c", "user.email=lab@example.invalid", "-c", "user.name=lab", *args],
        cwd=cwd,
        env=clean_git_env(),
        check=True,
        capture_output=True,
    )


@pytest.fixture(autouse=True)
def lab_root(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(experiments, "EXPERIMENTS", tmp_path)
    # The fixtures here test the other rules and carry no math.md; the maths rule's own tests
    # set or clear REMATCH_MATH_RULE themselves (ADR 0118).
    monkeypatch.setenv("REMATCH_MATH_RULE", "warn")


def test_a_deterministic_experiment_passes(tmp_path: Path) -> None:
    exp = make(tmp_path)
    assert experiments.check(exp) == []
    experiments.run_once(exp, exp)  # its results, as they would be committed
    assert experiments.check(exp, algorithm_id="unversioned") == []


def test_an_experiment_that_calls_the_bridge_runs_with_the_environment(tmp_path: Path) -> None:
    exp = make(tmp_path, BRIDGE_ANALYSIS)
    assert experiments.check(exp) == []
    experiments.run_once(exp, exp)
    results = json.loads((exp / "results.json").read_text())
    assert results["focuses"] == [["W1"], ["general"]]
    assert experiments.check(exp, algorithm_id=experiments.current_algorithm_id()) == []


def test_an_unseeded_experiment_fails(tmp_path: Path) -> None:
    exp = make(tmp_path, ANALYSIS.replace('random.Random(seed("EXP-9999", "draws"))', "random"))
    experiments.run_once(exp, exp)  # only committed results are rerun
    assert any("two runs" in p for p in experiments.check(exp))


def test_analysis_changed_after_results_fails(tmp_path: Path) -> None:
    exp = make(tmp_path)
    experiments.run_once(exp, exp)
    (exp / "analysis.py").write_text(ANALYSIS + "# tweak\n", encoding="utf-8")
    assert any("changed after its results" in p for p in experiments.check(exp))


def test_only_section_10_states_the_hash(tmp_path: Path) -> None:
    exp = make(tmp_path)
    experiments.run_once(exp, exp)
    text = (exp / "preregistration.md").read_text()
    moved = text.replace("## 10. Analysis code\n\n- Analysis code sha256:", "## 10. Analysis code\n\n- Was:")
    (exp / "preregistration.md").write_text("- Analysis code sha256: `" + "0" * 64 + "`\n" + moved)
    assert any("section 10 states no" in p for p in experiments.check(exp))


def test_results_that_differ_from_the_committed_ones_fail(tmp_path: Path) -> None:
    exp = make(tmp_path)
    (exp / "results.json").write_text('{"algorithm_id": "unversioned", "mean": 0.5}\n')
    assert any("differs from the committed" in p for p in experiments.check(exp, algorithm_id="unversioned"))


def test_results_must_name_their_algorithm(tmp_path: Path) -> None:
    exp = make(tmp_path)
    (exp / "results.json").write_text('{"mean": 0.5}\n')
    assert any("which algorithm_id" in p for p in experiments.check(exp))


LIVE = """---
id: EXP-9999
title: t
status: decided
algorithm: algo-1.0.0+000000000000
issue: 1
updated: 2026-11-24
live: true
rerun: weekly
run_cpu_hours: 0.1
run_minutes: 1
---

- b
"""


def test_results_from_an_older_algorithm_are_left_to_the_arena(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]
) -> None:
    exp = make(tmp_path)
    (exp / "results.json").write_text(
        '{"algorithm_id": "algo-1.0.0+000000000000", "mean": 0.5, '
        '"compute": {"budget": {"draws": 100}, "actual": {"draws": 100}}}\n'
    )
    (exp / "summary.md").write_text(LIVE, encoding="utf-8")
    (exp / "arena.py").write_text("METRICS = []\n", encoding="utf-8")
    # The arena re-runs it against the champion, so the check does not rerun it twice more.
    assert experiments.check(exp, algorithm_id="algo-2.0.0+111111111111") == []
    assert "live, re-run against the champion by the arena (weekly)" in capsys.readouterr().out


def test_a_decided_experiment_is_live_with_a_schedule_or_retired(tmp_path: Path) -> None:
    exp = make(tmp_path)
    (exp / "summary.md").write_text(LIVE, encoding="utf-8")
    assert any("arena.py" in p for p in experiments.metadata_problems(exp))
    (exp / "arena.py").write_text("METRICS = []\n", encoding="utf-8")
    assert experiments.metadata_problems(exp) == []
    (exp / "summary.md").write_text(LIVE.replace("rerun: weekly", "rerun: monthly"))
    assert any("`rerun`" in p for p in experiments.metadata_problems(exp))
    (exp / "summary.md").write_text(LIVE.replace("run_cpu_hours: 0.1", "run_cpu_hours: 0"))
    assert any("run_cpu_hours" in p for p in experiments.metadata_problems(exp))
    (exp / "summary.md").write_text(LIVE.replace("live: true", "live: false"))
    assert any("retired" in p for p in experiments.metadata_problems(exp))
    retired = LIVE.replace("live: true", "live: false\nretired: the detector was removed")
    (exp / "summary.md").write_text(retired)
    assert experiments.metadata_problems(exp) == []
    (exp / "summary.md").write_text(LIVE.replace("live: true\n", ""))
    assert any("live: true" in p for p in experiments.metadata_problems(exp))
    (exp / "summary.md").write_text(LIVE.replace("status: decided", "status: running"))
    (exp / "arena.py").unlink()
    assert experiments.metadata_problems(exp) == []


def test_the_committed_decided_experiments_are_live() -> None:
    root = Path(__file__).resolve().parent.parent / "experiments"
    for exp in sorted(root.glob("EXP-*")):
        assert experiments.metadata_problems(exp) == [], exp.name


def test_inputs_are_checked_against_their_sha256(tmp_path: Path) -> None:
    exp = make(tmp_path)
    (exp / "data.json").write_text("[1, 2]\n")
    digest = hashlib.sha256(b"[1, 2]\n").hexdigest()
    (exp / "inputs.sha256").write_text(f"{digest}  data.json\n")
    assert experiments.check(exp) == []
    (exp / "data.json").write_text("[1, 3]\n")
    assert any("does not match" in p for p in experiments.check(exp))


def test_an_external_input_is_checked_when_present_and_skips_the_rerun_when_absent(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]
) -> None:
    data = tmp_path / "data"
    monkeypatch.setenv("REMATCH_DATA_DIR", str(data))
    exp = make(tmp_path)
    digest = hashlib.sha256(b"PuzzleId\n").hexdigest()
    (exp / "inputs.sha256").write_text(f"{digest}  external:puzzles/db.csv\n")
    # Absent (CI): no problem, and no rerun.
    (exp / "results.json").write_text(
        '{"algorithm_id": "unversioned", "mean": 0.5, '
        '"compute": {"budget": {"draws": 100}, "actual": {"draws": 100}}}\n'
    )
    assert experiments.check(exp, algorithm_id="unversioned") == []
    assert "not rerun" in capsys.readouterr().out
    # Present: it must match, and the experiment is rerun.
    (data / "puzzles").mkdir(parents=True)
    (data / "puzzles" / "db.csv").write_bytes(b"PuzzleId\n")
    assert experiments.external_input("puzzles/db.csv") == data / "puzzles" / "db.csv"
    problems = experiments.check(exp, algorithm_id="unversioned")
    assert any("differs from the committed" in p for p in problems)
    (data / "puzzles" / "db.csv").write_bytes(b"PuzzleId,FEN\n")
    assert any("does not match" in p for p in experiments.check(exp, algorithm_id="unversioned"))


def test_history_registered_first_then_frozen(tmp_path: Path) -> None:
    git(tmp_path, "init", "-q")
    exp = make(tmp_path)
    git(tmp_path, "add", ".")
    git(tmp_path, "commit", "-qm", "register")
    experiments.run_once(exp, exp)
    git(tmp_path, "add", ".")
    git(tmp_path, "commit", "-qm", "results")
    assert experiments.history_problems(exp) == []

    # Editing H1 after the results is outcome switching.
    text = (exp / "preregistration.md").read_text()
    (exp / "preregistration.md").write_text(text.replace("the rate rises", "the rate falls"))
    assert any("sections 0 to 10" in p for p in experiments.history_problems(exp))
    # A deviation in section 11 is allowed.
    (exp / "preregistration.md").write_text(text.replace("None.", "2026-12-01: one seed failed."))
    assert experiments.history_problems(exp) == []


def test_history_results_in_the_same_commit_as_the_registration_fail(tmp_path: Path) -> None:
    git(tmp_path, "init", "-q")
    exp = make(tmp_path)
    experiments.run_once(exp, exp)
    git(tmp_path, "add", ".")
    git(tmp_path, "commit", "-qm", "everything at once")
    problems = experiments.history_problems(exp)
    assert any("preregistration.md was not committed before" in p for p in problems)


def test_results_are_canonical() -> None:
    assert experiments.canonical({"b": 1, "a": [0.1]}) == '{\n  "a": [\n    0.1\n  ],\n  "b": 1\n}\n'
    with pytest.raises(ValueError):
        experiments.canonical({"x": float("nan")})
    assert experiments.seed("EXP-0001", "a") == experiments.seed("EXP-0001", "a")
    assert experiments.seed("EXP-0001", "a") != experiments.seed("EXP-0001", "b")


def test_a_pre_registration_needs_its_value_case(tmp_path: Path) -> None:
    exp = make(tmp_path)
    text = (exp / "preregistration.md").read_text()
    missing = experiments.value_case_problems(exp, text.replace(VALUE_CASE, ""))
    assert any("no section 0" in p for p in missing)
    gap = text.replace("- Cheaper alternative? No.\n", "")
    assert any("Cheaper alternative" in p for p in experiments.value_case_problems(exp, gap))
    wrong = text.replace("cost 1 = 3", "cost 1 = 9")
    assert any("value score is 3" in p for p in experiments.value_case_problems(exp, wrong))
    assert experiments.value_case_problems(exp, text) == []
    sentence = text.replace("cost 1 = 3", "cost 2 = 1.5.")  # a full stop ends the score
    assert experiments.value_case_problems(exp, sentence) == []
    (exp / "preregistration.md").write_text(text.replace(VALUE_CASE, ""))
    assert any("no section 0" in p for p in experiments.check(exp))


def test_experiments_before_the_gate_are_exempt(tmp_path: Path) -> None:
    exp = tmp_path / "EXP-0001"
    exp.mkdir()
    assert experiments.value_case_problems(exp, "## 1. Question\n") == []
    assert experiments.compute_problems(exp, "", {}) == []


def test_the_budget_stops_the_run_at_its_cap() -> None:
    budget = experiments.Budget({"games": 10})
    budget.spend("games", 15)  # 150% is still inside the cap
    with pytest.raises(experiments.BudgetExceeded):
        budget.spend("games", 1)
    section0 = "## 0. Value case\n\n- Compute budget: 10 games; cap 200%\n"
    assert experiments.registered_cap(section0) == 2.0
    assert experiments.registered_cap(section0.replace("; cap 200%", "")) == 1.5


def test_results_past_the_cap_or_without_a_budget_fail(tmp_path: Path) -> None:
    exp = make(tmp_path)
    text = (exp / "preregistration.md").read_text()
    over = {"compute": {"budget": {"draws": 100}, "actual": {"draws": 151}}}
    assert any("past the cap" in p for p in experiments.compute_problems(exp, text, over))
    assert any("no compute budget" in p for p in experiments.compute_problems(exp, text, {}))
    fine = {"compute": {"budget": {"draws": 100}, "actual": {"draws": 150}}}
    assert experiments.compute_problems(exp, text, fine) == []


def test_an_experiment_closed_unrun_is_never_run(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """The kill rule: a decision.md with no results.json closes an experiment before it runs."""
    exp = make(tmp_path, ANALYSIS.replace('random.Random(seed("EXP-9999", "draws"))', "random"))
    (exp / "decision.md").write_text("# closed unrun\n", encoding="utf-8")
    assert experiments.check(exp) == []
    (exp / "preregistration.md").write_text("# no value case\n", encoding="utf-8")
    assert any("value case" in p for p in experiments.check(exp))


def test_the_git_helper_never_touches_a_repository_named_by_git_dir(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    # A container run sets GIT_DIR and GIT_WORK_TREE so git works in a mounted worktree; the
    # helper must still init and commit in its own temporary repository, never in that one.
    sentinel = tmp_path / "sentinel"
    sentinel.mkdir()
    git(sentinel, "init", "-q")
    (sentinel / "a").write_text("a\n")
    git(sentinel, "add", "a")
    git(sentinel, "commit", "-qm", "sentinel")
    before = subprocess.run(
        ["git", "rev-list", "--all"], cwd=sentinel, env=clean_git_env(),
        capture_output=True, text=True, check=True,
    ).stdout
    monkeypatch.setenv("GIT_DIR", str(sentinel / ".git"))
    monkeypatch.setenv("GIT_WORK_TREE", str(sentinel))
    inner = tmp_path / "inner"
    inner.mkdir()
    git(inner, "init", "-q")
    (inner / "b").write_text("b\n")
    git(inner, "add", "b")
    git(inner, "commit", "-qm", "inner")
    after = subprocess.run(
        ["git", "rev-list", "--all"], cwd=sentinel, env=clean_git_env(),
        capture_output=True, text=True, check=True,
    ).stdout
    assert after == before
    assert (inner / ".git").is_dir()


def test_an_experiment_without_committed_results_is_not_rerun(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]
) -> None:
    """Regression (2026-11-28): with its external inputs present and no results.json, the check
    ran analysis.py twice, which started an experiment's confirmatory run before its sizing
    commit. It reruns only an experiment whose results are committed."""
    data = tmp_path / "data"
    monkeypatch.setenv("REMATCH_DATA_DIR", str(data))
    (data / "puzzles").mkdir(parents=True)
    (data / "puzzles" / "db.csv").write_bytes(b"PuzzleId\n")
    marker = tmp_path / "ran"
    analysis = ANALYSIS + f"open({str(marker)!r}, 'a').write('x')\n"
    exp = make(tmp_path, analysis)
    digest = hashlib.sha256(b"PuzzleId\n").hexdigest()
    (exp / "inputs.sha256").write_text(f"{digest}  external:puzzles/db.csv\n")
    assert experiments.check(exp, algorithm_id="unversioned") == []
    assert not marker.exists()
    assert "not rerun: no results committed" in capsys.readouterr().out


def test_results_in_a_git_checkout_count_only_once_committed(tmp_path: Path) -> None:
    git(tmp_path, "init", "-q")
    exp = make(tmp_path)
    git(tmp_path, "add", ".")
    git(tmp_path, "commit", "-qm", "register")
    experiments.run_once(exp, exp)  # written by a run, not yet committed
    assert not experiments.results_committed(exp)
    git(tmp_path, "add", ".")
    git(tmp_path, "commit", "-qm", "results")
    assert experiments.results_committed(exp)


# ---- the maths (ADR 0118) ---------------------------------------------------------------------

MATH_BODIES = {
    1: "$n$ games, from `analysis.py` `load`.",
    2: "$y_i \\sim \\mathrm{Bernoulli}(p)$.",
    3: "$\\hat p = \\bar y$ (`analysis.py` `estimate`).",
    4: "**Result 1.** $E[\\hat p] = p$. *Proof.* Linearity. $\\blacksquare$",
    5: "`mean` = 0.5 in results.json.",
    6: "Independence of games; sensitivity S1.",
}


def math_md(filled: tuple[int, ...] = (1, 2, 3, 4, 5, 6)) -> str:
    parts = ["# EXP-9999: the maths\n"]
    for n, heading in enumerate(experiments.MATH_SECTIONS, start=1):
        body = MATH_BODIES[n] if n in filled else "<!-- to do -->"
        parts.append(f"{heading}\n\n{body}\n")
    return "\n".join(parts)


MATH_CHECK_OK = "import sympy\nassert sympy.simplify(sympy.Symbol('p') - sympy.Symbol('p')) == 0\n"
MATH_CHECK_BAD = (
    "import sys\nprint('Result 1 does not simplify to 0', file=sys.stderr)\nsys.exit(1)\n"
)


def decide(exp: Path) -> None:
    (exp / "decision.md").write_text("# ship\n", encoding="utf-8")


def test_the_math_template_has_the_checked_headings() -> None:
    template = experiments.LAB_ROOT.parent / "docs" / "experiments" / "MATH-TEMPLATE.md"
    if not template.is_file():
        pytest.skip("the docs are not in this image")
    lines = template.read_text(encoding="utf-8").splitlines()
    assert tuple(line for line in lines if line.startswith("## ")) == experiments.MATH_SECTIONS


def test_the_template_math_check_runs_and_passes(tmp_path: Path) -> None:
    exp = make(tmp_path)
    decide(exp)
    (exp / "math.md").write_text(math_md(), encoding="utf-8")
    template = experiments.LAB_ROOT / "templates" / "math_check.py"
    (exp / "math_check.py").write_text(template.read_text(encoding="utf-8"), encoding="utf-8")
    assert experiments.math_problems(exp) == []
    # Its numbers check reads the experiment's results.json: a wrong rate fails it.
    (exp / "results.json").write_text(
        '{"n": 400, "wins": 212, "rate": 0.54, "interval": [0.4811, 0.5789]}\n'
    )
    assert any("the rate" in p for p in experiments.math_problems(exp))


def test_a_pre_registered_experiment_needs_no_maths_yet(tmp_path: Path) -> None:
    exp = make(tmp_path)
    assert experiments.math_problems(exp) == []


def test_a_decided_experiment_with_its_maths_passes(tmp_path: Path) -> None:
    exp = make(tmp_path)
    decide(exp)
    (exp / "math.md").write_text(math_md(), encoding="utf-8")
    (exp / "math_check.py").write_text(MATH_CHECK_OK, encoding="utf-8")
    assert experiments.math_problems(exp) == []


def test_a_decided_experiment_without_maths_fails(tmp_path: Path) -> None:
    exp = make(tmp_path)
    decide(exp)
    problems = experiments.math_problems(exp)
    assert any("[math]" in p and "no math.md" in p for p in problems)
    assert any("no math_check.py" in p for p in problems)


def test_a_missing_or_empty_math_section_fails(tmp_path: Path) -> None:
    exp = make(tmp_path)
    decide(exp)
    (exp / "math_check.py").write_text(MATH_CHECK_OK, encoding="utf-8")
    (exp / "math.md").write_text(math_md().replace("## 3. Estimators", "## 3. Estimates"))
    assert any('"## 3. Estimators"' in p for p in experiments.math_problems(exp))
    (exp / "math.md").write_text(math_md(filled=(1, 2, 3, 4, 6)))
    problems = experiments.math_problems(exp)
    assert [p for p in problems if "is empty" in p] == [
        'EXP-9999: [math] "## 5. Numbers" of math.md is empty'
    ]


def test_a_failing_math_check_fails(tmp_path: Path) -> None:
    exp = make(tmp_path)
    decide(exp)
    (exp / "math.md").write_text(math_md(), encoding="utf-8")
    (exp / "math_check.py").write_text(MATH_CHECK_BAD, encoding="utf-8")
    problems = experiments.math_problems(exp)
    assert any("math_check.py exited 1" in p and "does not simplify" in p for p in problems)


def test_math_check_runs_in_the_experiment_folder(tmp_path: Path) -> None:
    exp = make(tmp_path)
    decide(exp)
    (exp / "math.md").write_text(math_md(), encoding="utf-8")
    (exp / "results.json").write_text('{"mean": 0.5}\n')
    (exp / "math_check.py").write_text(
        "import json\nassert json.load(open('results.json'))['mean'] == 0.5\n", encoding="utf-8"
    )
    assert experiments.math_problems(exp) == []


def test_at_the_freeze_sections_1_to_4_are_due_and_5_6_wait_for_the_decision(
    tmp_path: Path,
) -> None:
    git(tmp_path, "init", "-q")
    exp = make(tmp_path)
    git(tmp_path, "add", ".")
    git(tmp_path, "commit", "-qm", "register")
    assert experiments.math_problems(exp) == []
    git(tmp_path, "tag", "exp-9999-freeze")
    assert any("no math.md" in p for p in experiments.math_problems(exp))
    (exp / "math_check.py").write_text(MATH_CHECK_OK, encoding="utf-8")
    (exp / "math.md").write_text(math_md(filled=(1, 2, 3)), encoding="utf-8")
    assert any('"## 4. Derivations and proofs"' in p for p in experiments.math_problems(exp))
    (exp / "math.md").write_text(math_md(filled=(1, 2, 3, 4)), encoding="utf-8")
    assert experiments.math_problems(exp) == []
    decide(exp)
    problems = experiments.math_problems(exp)
    assert len(problems) == 2 and all("is empty" in p for p in problems)


def test_the_math_rule_fails_by_default_and_warns_only_when_asked(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]
) -> None:
    exp = make(tmp_path)
    (exp / "decision.md").write_text("# closed unrun\n", encoding="utf-8")
    monkeypatch.delenv("REMATCH_MATH_RULE", raising=False)
    assert any("[math] no math.md" in p for p in experiments.check(exp))
    monkeypatch.setenv("REMATCH_MATH_RULE", "warn")
    assert experiments.check(exp) == []
    assert "warning: EXP-9999: [math] no math.md" in capsys.readouterr().out
    monkeypatch.setenv("REMATCH_MATH_RULE", "maybe")
    with pytest.raises(SystemExit):
        experiments.check(exp)

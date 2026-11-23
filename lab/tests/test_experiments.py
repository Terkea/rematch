"""The experiment rules (rematch_lab.experiments): registered first, frozen, checked inputs,
deterministic, and historical results left alone."""

from __future__ import annotations

import hashlib
import json
import subprocess
from pathlib import Path

import pytest

from rematch_lab import experiments

ANALYSIS = """\
import argparse, random
from rematch_lab.experiments import seed, write_results
p = argparse.ArgumentParser(); p.add_argument("--out", required=True); a = p.parse_args()
rng = random.Random(seed("EXP-9999", "draws"))
write_results(a.out, {"algorithm_id": "unversioned",
                      "mean": sum(rng.random() for _ in range(100)) / 100})
"""

# An experiment that drives the real algorithm through the bridge, as every experiment must.
BRIDGE_ANALYSIS = """\
import argparse, json
from pathlib import Path
import rematch_lab
from rematch_lab.experiments import write_results
p = argparse.ArgumentParser(); p.add_argument("--out", required=True); a = p.parse_args()
fixture = Path(rematch_lab.__file__).parent.parent / "fixtures" / "profile-request.json"
facts = rematch_lab.models.ProfileRequest.model_validate_json(fixture.read_text())
ranking = rematch_lab.rank(rematch_lab.profile(facts))
write_results(a.out, {"algorithm_id": ranking.algorithm_id,
                      "focuses": [p.focuses for p in ranking.players]})
"""


def prereg(analysis: str, h1: str = "H1: the rate rises.") -> str:
    digest = hashlib.sha256(analysis.encode()).hexdigest()
    return (
        "# EXP-9999: a test\n\n- Status: pre-registered\n\n"
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
        check=True,
        capture_output=True,
    )


@pytest.fixture(autouse=True)
def lab_root(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(experiments, "EXPERIMENTS", tmp_path)


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


def test_results_from_an_older_algorithm_are_historical(tmp_path: Path) -> None:
    exp = make(tmp_path)
    (exp / "results.json").write_text('{"algorithm_id": "algo-1.0.0+000000000000", "mean": 0.5}\n')
    # Not rerun on an algorithm it never measured, so the differing number is no problem.
    assert experiments.check(exp, algorithm_id="algo-2.0.0+111111111111") == []


def test_inputs_are_checked_against_their_sha256(tmp_path: Path) -> None:
    exp = make(tmp_path)
    (exp / "data.json").write_text("[1, 2]\n")
    digest = hashlib.sha256(b"[1, 2]\n").hexdigest()
    (exp / "inputs.sha256").write_text(f"{digest}  data.json\n")
    assert experiments.check(exp) == []
    (exp / "data.json").write_text("[1, 3]\n")
    assert any("does not match" in p for p in experiments.check(exp))


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
    assert any("sections 1 to 10" in p for p in experiments.history_problems(exp))
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

"""The experiment rules (rematch_lab.experiments): determinism and the frozen analysis."""

from __future__ import annotations

import hashlib
from pathlib import Path

import pytest

from rematch_lab import experiments

ANALYSIS = """\
import argparse, random
from rematch_lab.experiments import seed, write_results
p = argparse.ArgumentParser(); p.add_argument("--out", required=True); a = p.parse_args()
rng = random.Random(seed("EXP-9999", "draws"))
write_results(a.out, {"mean": sum(rng.random() for _ in range(100)) / 100})
"""


def make(tmp_path: Path, analysis: str = ANALYSIS) -> Path:
    exp = tmp_path / "EXP-9999"
    exp.mkdir()
    (exp / "analysis.py").write_text(analysis, encoding="utf-8")
    digest = hashlib.sha256(analysis.encode()).hexdigest()
    (exp / "preregistration.md").write_text(f"Analysis code sha256: `{digest}`\n")
    return exp


@pytest.fixture(autouse=True)
def lab_root(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(experiments, "EXPERIMENTS", tmp_path)


def test_a_deterministic_experiment_passes(tmp_path: Path) -> None:
    exp = make(tmp_path)
    assert experiments.check(exp) == []
    experiments.run_once(exp, exp)  # commit its results
    assert experiments.check(exp) == []


def test_an_unseeded_experiment_fails(tmp_path: Path) -> None:
    exp = make(tmp_path, ANALYSIS.replace('random.Random(seed("EXP-9999", "draws"))', "random"))
    assert any("two runs" in p for p in experiments.check(exp))


def test_analysis_changed_after_results_fails(tmp_path: Path) -> None:
    exp = make(tmp_path)
    experiments.run_once(exp, exp)
    (exp / "analysis.py").write_text(ANALYSIS + "# tweak\n", encoding="utf-8")
    problems = experiments.check(exp)
    assert any("changed after its results" in p for p in problems)


def test_results_that_differ_from_the_committed_ones_fail(tmp_path: Path) -> None:
    exp = make(tmp_path)
    (exp / "results.json").write_text('{"mean": 0.5}\n')
    assert any("differs from the committed" in p for p in experiments.check(exp))


def test_results_are_canonical() -> None:
    assert experiments.canonical({"b": 1, "a": [0.1]}) == '{\n  "a": [\n    0.1\n  ],\n  "b": 1\n}\n'
    with pytest.raises(ValueError):
        experiments.canonical({"x": float("nan")})
    assert experiments.seed("EXP-0001", "a") == experiments.seed("EXP-0001", "a")
    assert experiments.seed("EXP-0001", "a") != experiments.seed("EXP-0001", "b")

"""EXP-0029's registered analysis on a synthetic cohort with a known truth: a null reads
outcome 2 or 3 and never 1, an injected effect is recovered as outcome 1 on its own key, the
control holds when accuracy is linked and fails when it is not, two runs are byte-identical, the
budget cap stops the run, EXP-0009's hash range is excluded, and the sizing step reads the
players files only."""

from __future__ import annotations

import importlib.util
import json
import sys
from pathlib import Path

import pytest

import rematch_lab
from rematch_lab.experiments import BudgetExceeded

sys.path.insert(0, str(Path(__file__).resolve().parent))
import exp0029_synthetic as S  # noqa: E402

EXP_DIR = Path(__file__).resolve().parent.parent / "experiments" / "EXP-0029"
spec = importlib.util.spec_from_file_location("exp0029_analysis", EXP_DIR / "analysis.py")
A = importlib.util.module_from_spec(spec)
spec.loader.exec_module(A)
CAP = (144, 120)


def cohort(tmp: Path, monkeypatch: pytest.MonkeyPatch, **kw) -> None:
    truth = S.build(tmp, **kw)
    monkeypatch.setenv("REMATCH_DATA_DIR", str(tmp))
    monkeypatch.setattr(rematch_lab, "observe", S.fake_observe(truth))
    monkeypatch.setattr(rematch_lab, "evaluate", S.fake_evaluate)
    monkeypatch.setattr(rematch_lab, "algorithm", lambda *_: {"id": "algo-synthetic"})


def run(tmp: Path, name: str = "out") -> dict:
    A.run(tmp / name, 0.6, CAP)
    return json.loads((tmp / name / "results.json").read_text(encoding="utf-8"))


def test_an_injected_effect_is_recovered_on_its_own_key(tmp_path, monkeypatch):
    cohort(tmp_path, monkeypatch, beta_key=15.0, beta_control=15.0)
    r = run(tmp_path)
    d = r["decision"]
    assert d["outcome"] == 1
    assert d["climbing"] == ["W7.traps"]
    assert r["primary"]["W7.traps"]["reads"] == "precedes climbing"
    assert d["positive_control_holds"] is True
    # The window-mean level model is the primary; the F1-level sensitivity is fitted too.
    assert r["f1level"]["W7.traps"]["estimate"] is not None
    rel = r["primary"]["W7.traps"]["reliability"]
    assert 0.01 <= rel["lower"] <= rel["point"] <= 1


def test_a_null_never_reads_candidates_and_the_control_holds(tmp_path, monkeypatch):
    cohort(tmp_path, monkeypatch, beta_key=0.0, beta_control=15.0)
    r = run(tmp_path)
    d = r["decision"]
    assert d["outcome"] in (2, 3)
    assert d["climbing"] == []
    assert d["positive_control_holds"] is True
    assert r["primary"]["accuracy"]["holds"] is True


def test_without_a_control_link_the_result_is_need_more_data(tmp_path, monkeypatch):
    cohort(tmp_path, monkeypatch, beta_key=0.0, beta_control=0.0)
    r = run(tmp_path)
    assert r["decision"]["positive_control_holds"] is False
    assert r["decision"]["outcome"] == 3


def test_two_runs_are_byte_identical_and_exclude_exp0009s_range(tmp_path, monkeypatch):
    cohort(tmp_path, monkeypatch, beta_key=15.0)
    first = run(tmp_path, "a")
    run(tmp_path, "b")
    assert (tmp_path / "a" / "results.json").read_bytes() == \
        (tmp_path / "b" / "results.json").read_bytes()
    assert first["design"]["slice"] == [0.3, 0.9]
    assert first["players"] > 0
    # A tenth of the synthetic players sit in [0, 0.3): none is followed.
    assert first["flow"]["history"] < 1000 * 0.95
    assert first["compute"]["actual"]["observe_games"] == first["games"]["games_read"] - \
        first["games"]["games_not_standard"]


def test_the_budget_cap_stops_the_run(tmp_path, monkeypatch):
    cohort(tmp_path, monkeypatch)
    monkeypatch.setattr(A, "BUDGET", {"observe_games": 1_000, "engine_nodes": 1_000_000})
    with pytest.raises(BudgetExceeded):
        A.run(tmp_path / "out", 0.6, CAP)
    assert not (tmp_path / "out" / "results.json").exists()


def test_the_sizing_step_reads_the_players_files_only(tmp_path, monkeypatch):
    cohort(tmp_path, monkeypatch)

    def no_games(*_, **__):
        raise AssertionError("the sizing step must not observe a game")

    monkeypatch.setattr(rematch_lab, "observe", no_games)
    for m in (*S.F1, *S.F2):
        (tmp_path / "exp29-cohort" / m / "games.jsonl").unlink()
    m = A.sizing_measures(7.0)
    assert set(m["caps"]) == {"144/120", "96/80", "72/60"}
    assert m["counts"]["with_outcome"] > 0
    assert m["april_share"]["0.98"] <= 0
    results0009 = json.loads(A.POWER.EXP0009.read_text(encoding="utf-8"))
    d = A.POWER.decide_size(results0009, {k: v for k, v in m.items() if k != "counts"})
    assert "run" in d

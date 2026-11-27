"""EXP-0029's registered sizing decision (power.py, section 6): mechanical, deterministic, and
closing the run when the gap month is too quiet or the caps cannot be met."""

from __future__ import annotations

import importlib.util
import json
from pathlib import Path

EXP_DIR = Path(__file__).resolve().parent.parent / "experiments" / "EXP-0029"
spec = importlib.util.spec_from_file_location("exp0029_power", EXP_DIR / "power.py")
P = importlib.util.module_from_spec(spec)
spec.loader.exec_module(P)
RESULTS = json.loads(P.EXP0009.read_text(encoding="utf-8"))
FIXTURE = json.loads((EXP_DIR / "sizing-fixture.json").read_text(encoding="utf-8"))


def test_the_fixture_runs_and_the_decision_is_deterministic():
    a = P.decide_size(RESULTS, FIXTURE)
    b = P.decide_size(RESULTS, FIXTURE)
    assert a == b
    assert a["run"] is True
    assert a["w"] in P.W_GRID
    assert len(a["outcome2_keys"]) >= 4
    chosen = a["tried"][-1]
    assert chosen["observe_games"] <= P.OBSERVE_GAMES_BUDGET * P.BUDGET_CAP
    assert chosen["cpu_hours"] <= P.CPU_HOURS_CAP


def test_a_quiet_gap_month_closes_the_run_unrun():
    quiet = {**FIXTURE, "april_games": 15}
    assert P.decide_size(RESULTS, quiet)["run"] is False


def test_too_many_games_fall_back_to_a_smaller_cap_before_the_kill():
    busy = {**FIXTURE, "density": 2.5, "k": 5.6}
    d = P.decide_size(RESULTS, busy)
    caps = {tuple(t["cap"]) for t in d["tried"]}
    assert (144, 120) in caps
    if d["run"]:
        assert d["tried"][-1]["observe_games"] <= P.OBSERVE_GAMES_BUDGET * P.BUDGET_CAP


def test_a_streaming_overrun_closes_the_run():
    slow = {**FIXTURE, "streaming_cpu_hours": 24.0}
    assert P.decide_size(RESULTS, slow)["run"] is False

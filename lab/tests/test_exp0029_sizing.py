"""EXP-0029's registered sizing decision (power.py, section 6): mechanical, deterministic, at
the largest size within the caps, on the five registered keys, and closing the run when the gap
month carries too much of the exposure's luck or the caps cannot be met."""

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


def test_the_fixture_runs_at_the_largest_size_and_is_deterministic():
    a = P.decide_size(RESULTS, FIXTURE)
    assert a == P.decide_size(RESULTS, FIXTURE)
    assert a["run"] is True
    assert a["w"] == max(P.W_GRID)
    assert a["cap"] == list(P.CAPS_PER_WINDOW[0])
    chosen = a["tried"][-1]
    assert set(chosen["below"]) == set(P.OUTCOME2_KEYS)
    assert chosen["observe_games"] <= P.OBSERVE_GAMES_BUDGET * P.BUDGET_CAP
    assert chosen["cpu_hours"] <= P.CPU_HOURS_CAP


def test_a_quiet_gap_month_closes_the_run_unrun():
    # Players who barely play in April: the outcome keeps EXP-0009's whole share of the luck.
    quiet = {**FIXTURE, "april_share": {"0.97": -0.87, "0.98": -0.81}}
    d = P.decide_size(RESULTS, quiet)
    assert d["run"] is False


def test_too_many_games_shrink_the_size_then_the_cap_before_the_kill():
    busy = {**FIXTURE, "density": 2.5}
    d = P.decide_size(RESULTS, busy)
    assert d["tried"][0]["fits"] is False
    if d["run"]:
        assert d["tried"][-1]["observe_games"] <= P.OBSERVE_GAMES_BUDGET * P.BUDGET_CAP


def test_a_streaming_overrun_closes_the_run():
    slow = {**FIXTURE, "streaming_cpu_hours": 24.0}
    d = P.decide_size(RESULTS, slow)
    assert d["run"] is False and d["reason"] == "no size fits the caps"

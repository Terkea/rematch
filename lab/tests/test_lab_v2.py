"""The lab-v2 steps (#125, ADR 0115): the trajectory and the timeline through `cli lab`, the weekly
focus and the trainer's sources through the web's Node step.

1. Each committed request gives the committed golden file: byte for byte from the step, value for
   value through the typed wrapper. The Rust test (`backend/crates/cli/tests/lab.rs`) and the web
   test (`apps/web/test/lab-bridge.test.ts`) check the same golden files from their side.
2. Every wrapper returns what its step answered, never a result of its own: with the steps
   replaced by a stand-in that answers something no rule would, the wrappers return exactly that.
"""

from __future__ import annotations

import json
import stat
import sys
from pathlib import Path

import pytest

import rematch_lab
from rematch_lab import LabError, models
from rematch_lab.bridge import call_raw, web_bridge

LAB = Path(__file__).resolve().parent.parent
FIXTURES = LAB / "fixtures"
GOLDEN = FIXTURES / "golden"
PLACEHOLDER = "ALGORITHM_ID"

RUST = {
    "trajectory": ("TrajectoryRequest", "TrajectoryResponse", rematch_lab.trajectory),
    "timeline": ("TimelineRequest", "TimelineResponse", rematch_lab.timeline),
}
WEB = {
    "focus-target": ("FocusTargetRequest", "FocusTargetResponse", rematch_lab.focus_target),
    "focus-progress": ("FocusProgressRequest", "FocusProgressResponse", rematch_lab.focus_progress),
    "trainer-sources": (
        "TrainerSourcesRequest",
        "TrainerSourcesResponse",
        rematch_lab.trainer_sources,
    ),
}


def request_text(step: str) -> str:
    return (FIXTURES / f"{step}-request.json").read_text(encoding="utf-8")


def golden_text(step: str) -> str:
    return (GOLDEN / f"{step}-response.json").read_text(encoding="utf-8")


def with_placeholder(text: str, algorithm_id: str) -> str:
    return text.replace(f'"algorithm_id": "{algorithm_id}"', f'"algorithm_id": "{PLACEHOLDER}"')


def run_web(step: str, stdin: str) -> str:
    import subprocess

    proc = subprocess.run(
        [*web_bridge(), step], input=stdin, capture_output=True, text=True, encoding="utf-8"
    )
    assert proc.returncode == 0, proc.stderr
    return proc.stdout


def raw(step: str, stdin: str) -> str:
    return call_raw(["lab", step], stdin) if step in RUST else run_web(step, stdin)


@pytest.mark.parametrize("step", [*RUST, *WEB])
def test_the_step_gives_the_golden_file_byte_for_byte(step: str) -> None:
    request_message, response_message, _ = {**RUST, **WEB}[step]
    request = request_text(step)
    rematch_lab.check(request_message, json.loads(request))
    out = raw(step, request)
    doc = json.loads(out)
    rematch_lab.check(response_message, doc)
    assert doc["algorithm_id"] == rematch_lab.algorithm()["id"]
    assert with_placeholder(out, doc["algorithm_id"]) == golden_text(step)
    assert raw(step, request) == out, "the same request gave other bytes"


@pytest.mark.parametrize("step", [*RUST, *WEB])
def test_the_typed_wrapper_gives_the_golden_values(step: str) -> None:
    request_message, _, wrapper = {**RUST, **WEB}[step]
    req = models.MESSAGES[request_message].model_validate_json(request_text(step))
    res = wrapper(req)
    assert res.schema_version == rematch_lab.SCHEMA_VERSION
    golden = json.loads(golden_text(step))
    golden["algorithm_id"] = res.algorithm_id
    assert json.loads(res.model_dump_json(by_alias=True)) == golden


def test_the_trajectory_verdicts_of_the_fixture() -> None:
    req = models.TrajectoryRequest.model_validate_json(request_text("trajectory"))
    by = {p.id: p.trajectory for p in rematch_lab.trajectory(req.players).players}
    assert by["lab-peaked"].kind == "falling"
    assert by["lab-peaked"].then_vs_now is not None
    assert by["lab-peaked"].then_vs_now.rating.verdict == "worse"
    assert by["lab-steady"].kind != "falling"
    assert by["lab-new"].sufficient is False


def test_the_timeline_confirms_the_injected_change() -> None:
    req = models.TimelineRequest.model_validate_json(request_text("timeline"))
    res = rematch_lab.timeline(req.players)
    w1 = res.players[0].series["W1"]
    assert any(r.kind == "onset" for r in w1.rows)
    assert res.detector_version == "tl-v1"
    assert res.players[1].series["W1"].state == "not_an_issue"


def test_a_bad_request_is_refused_before_the_step() -> None:
    with pytest.raises(LabError, match="TrajectoryRequest"):
        rematch_lab.check("TrajectoryRequest", {"schema_version": "lab-v1", "players": [{"id": "x"}]})
    with pytest.raises(LabError, match="TrainerSourcesRequest"):
        rematch_lab.check(
            "TrainerSourcesRequest",
            {"schema_version": "lab-v1", "candidates": [{"game_id": "g", "sources": ["W1"]}]},
        )


def test_the_steps_refuse_an_unknown_schema_version() -> None:
    with pytest.raises(LabError, match="lab-v0"):
        call_raw(["lab", "timeline"], '{"schema_version": "lab-v0", "players": []}')
    import subprocess

    proc = subprocess.run(
        [*web_bridge(), "focus-target"],
        input='{"schema_version": "lab-v0", "items": []}',
        capture_output=True,
        text=True,
    )
    assert proc.returncode == 1
    assert "lab-v0" in proc.stderr


# ---- the wrappers compute nothing themselves -----------------------------------------------------

ALGO = "algo-0.0.1+000000000000"

#: Answers no production rule gives for the requests below, so a wrapper that computed the rule
#: itself (or quietly fell back to Python) could not return them.
CANNED = {
    "trajectory": {
        "schema_version": "lab-v1",
        "algorithm_id": ALGO,
        "players": [
            {
                "id": "stand-in",
                "trajectory": {
                    "ratedGames": 4242,
                    "sufficient": False,
                    "needMore": 7,
                    "kind": None,
                    "peak": None,
                    "current": None,
                    "change": None,
                    "months": [],
                    "thenVsNow": None,
                },
            }
        ],
    },
    "timeline": {
        "schema_version": "lab-v1",
        "algorithm_id": ALGO,
        "detector_version": "tl-stand-in",
        "players": [{"id": "stand-in", "series": {}}],
    },
    "focus-target": {
        "schema_version": "lab-v1",
        "algorithm_id": ALGO,
        "focus_version": "focus-stand-in",
        "items": [{"id": "stand-in", "target": {"rate": 0.999, "basis": "peer"}}],
    },
    "focus-progress": {
        "schema_version": "lab-v1",
        "algorithm_id": ALGO,
        "focus_version": "focus-stand-in",
        "items": [
            {
                "id": "stand-in",
                "progress": {
                    "events": 99,
                    "chances": 1,
                    "rate": None,
                    "interval": None,
                    "games": {"played": 0, "of": 1},
                    "verdict": {"kind": "on_track"},
                    "done": True,
                    "outcome": "met",
                },
            }
        ],
    },
    "trainer-sources": {
        "schema_version": "lab-v1",
        "algorithm_id": ALGO,
        "trainer_version": "trainer-stand-in",
        "sources": ["blunder"],
        "candidates": [
            {
                "game_id": "stand-in",
                "ply": 0,
                "kept": True,
                "primary_source": "W7.traps",
                "held_out": True,
            }
        ],
    },
}


@pytest.fixture
def stand_in(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> Path:
    """`cli` and the Node step replaced by one script that logs its step and answers CANNED."""
    log = tmp_path / "steps.log"
    script = tmp_path / "stand-in"
    script.write_text(
        f"#!{sys.executable}\n"
        "import json, sys\n"
        f"canned = json.loads({json.dumps(json.dumps(CANNED))})\n"
        "step = sys.argv[-1]\n"
        "sys.stdin.read()\n"
        f"open({str(log)!r}, 'a').write(step + '\\n')\n"
        "print(json.dumps(canned[step]))\n",
        encoding="utf-8",
    )
    script.chmod(script.stat().st_mode | stat.S_IEXEC)
    monkeypatch.setenv("REMATCH_CLI", str(script))
    monkeypatch.setenv("REMATCH_WEB_BRIDGE", str(script))
    return log


def test_every_wrapper_returns_the_steps_answer_not_its_own(stand_in: Path) -> None:
    for step, (request_message, _, wrapper) in {**RUST, **WEB}.items():
        req = models.MESSAGES[request_message].model_validate_json(request_text(step))
        got = json.loads(wrapper(req).model_dump_json(by_alias=True))
        assert got == CANNED[step], f"{step}: the wrapper did not return the step's answer"
    assert stand_in.read_text(encoding="utf-8").split() == [*RUST, *WEB]

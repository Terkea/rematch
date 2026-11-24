"""`rematch_lab.bench` and `rematch_lab.analyse`.

The bench wrapper is tested against a stand-in cli that records its arguments, so it needs no
database. The real `analyse` needs Postgres and Stockfish; the Rust test
`backend/crates/cli/tests/lab.rs` runs it in the gate.
"""

from __future__ import annotations

import json
import stat
import sys
from pathlib import Path

import pytest

import rematch_lab

CANNED = {"scorecard": {"metrics": {}}, "suite": {"name": "smoke"}, "players": []}


@pytest.fixture
def fake_cli(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> Path:
    log = tmp_path / "argv.json"
    script = tmp_path / "cli"
    script.write_text(
        f"#!{sys.executable}\n"
        "import json, sys\n"
        f"open({str(log)!r}, 'w').write(json.dumps(sys.argv[1:]))\n"
        "if sys.argv[1:3] == ['bench', 'run'] and '--baseline' in sys.argv:\n"
        "    print('regression: recall_at_2', file=sys.stderr); sys.exit(2)\n"
        f"print(json.dumps({CANNED!r}))\n",
        encoding="utf-8",
    )
    script.chmod(script.stat().st_mode | stat.S_IEXEC)
    monkeypatch.setenv("REMATCH_CLI", str(script))
    return log


def test_bench_passes_its_options_and_returns_the_results(fake_cli: Path, tmp_path: Path) -> None:
    out = rematch_lab.bench("full", out_dir=tmp_path / "o", nodes=5000, engines=2)
    assert out == CANNED
    assert json.loads(fake_cli.read_text()) == [
        "bench",
        "run",
        "--suite",
        "full",
        "--json",
        "--out",
        str(tmp_path / "o"),
        "--nodes",
        "5000",
        "--engines",
        "2",
    ]


def test_a_benchmark_regression_is_an_error(fake_cli: Path, tmp_path: Path) -> None:
    with pytest.raises(rematch_lab.LabError, match="regression: recall_at_2"):
        rematch_lab.bench(out_dir=tmp_path, baseline="bench/baseline.json")


def test_bench_refuses_an_unknown_suite(tmp_path: Path) -> None:
    with pytest.raises(ValueError):
        rematch_lab.bench("huge", out_dir=tmp_path)



def test_bench_runs_a_suite_file(fake_cli: Path, tmp_path: Path) -> None:
    out = rematch_lab.bench(out_dir=tmp_path / "o", suite_file=tmp_path / "suite.json", engines=8)
    assert out == CANNED
    assert json.loads(fake_cli.read_text()) == [
        "bench",
        "run",
        "--suite-file",
        str(tmp_path / "suite.json"),
        "--json",
        "--out",
        str(tmp_path / "o"),
        "--engines",
        "8",
    ]

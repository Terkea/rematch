"""EXP-0029's stream.sh with a stand-in for docker: the dry run prints the nine commands and
streams nothing, the guard refuses a df it cannot parse, a stream sums the containers' own CPU,
and a failed month deletes its partial files and stops the stream."""

from __future__ import annotations

import os
import subprocess
from pathlib import Path

import pytest

SCRIPT = Path(__file__).resolve().parent.parent / "experiments" / "EXP-0029" / "stream.sh"
FAKE_DOCKER = r"""#!/bin/sh
# A stand-in for docker: records each call, answers the image checks, and "streams" a month
# by creating its folder and printing a cpu.stat (or failing, for $FAKE_FAIL_MONTH).
echo "$*" >> "$FAKE_CALLS"
case "$1" in
  image) echo "sha256:fakedigest"; exit 0 ;;
  stop) exit 0 ;;
esac
case "$*" in
  *"--entrypoint cli"*) echo '{"id": "algo-6.0.0+f40a2b27677a"}'; exit 0 ;;
  *"cohort --help"*) exit 0 ;;
esac
m=$(printf '%s' "$*" | sed -n 's/.*--month \([0-9-]*\).*/\1/p')
mkdir -p "$FAKE_OUT/$m"
if [ "$m" = "${FAKE_FAIL_MONTH:-}" ]; then
  touch "$FAKE_OUT/$m/games.jsonl.part"
  echo "sha256 mismatch"
  exit 3
fi
echo "exp29-cpu-stat"
echo "usage_usec 1800000000"
exit 0
"""


@pytest.fixture
def env(tmp_path: Path) -> dict[str, str]:
    bin_dir = tmp_path / "bin"
    bin_dir.mkdir()
    docker = bin_dir / "docker"
    docker.write_text(FAKE_DOCKER, encoding="utf-8")
    docker.chmod(0o755)
    out = tmp_path / "exp29-cohort"
    return {
        **os.environ,
        "EXP29_DOCKER": str(docker),
        "EXP29_OUT": str(out),
        "EXP29_OUT_HOST": str(out),
        "EXP29_DF_PATH": str(tmp_path),
        "EXP29_MIN_FREE_GB": "0",
        "EXP29_WATCH_SECONDS": "1",
        "FAKE_OUT": str(out),
        "FAKE_CALLS": str(tmp_path / "calls.txt"),
    }


def sh(env: dict[str, str], *args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(["sh", str(SCRIPT), *args], env=env, capture_output=True, text=True,
                          check=False)


def test_the_dry_run_prints_the_nine_commands_and_streams_nothing(env):
    r = sh(env, "--dry-run")
    assert r.returncode == 0, r.stdout + r.stderr
    cmds = [line for line in r.stdout.splitlines() if "dry run: " in line and "cohort --month" in line]
    assert len(cmds) == 9
    assert sum("--games" in c for c in cmds) == 6
    for m in ("2025-09", "2026-04", "2026-06"):
        assert all("--games" not in c for c in cmds if f"--month {m}" in c)
    assert all("--player-rate 0.9" in c and "/sys/fs/cgroup/cpu.stat" in c for c in cmds)
    assert "nothing streamed" in r.stdout
    assert not Path(env["EXP29_OUT"]).exists()
    calls = Path(env["FAKE_CALLS"]).read_text(encoding="utf-8")
    assert "--entrypoint sh" not in calls  # no month was run


def test_a_df_that_does_not_parse_stops_before_anything(env, tmp_path):
    bad = tmp_path / "bin" / "df"
    bad.write_text("#!/bin/sh\necho 'Avail'\necho 'n/a'\n", encoding="utf-8")
    bad.chmod(0o755)
    r = sh({**env, "EXP29_DF": str(bad)}, "--dry-run")
    assert r.returncode == 2
    assert "did not parse a number" in r.stdout + r.stderr


def test_a_stream_sums_the_containers_cpu(env):
    r = sh(env)
    assert r.returncode == 0, r.stdout + r.stderr
    out = Path(env["EXP29_OUT"])
    tsv = (out / "stream-cpu.tsv").read_text(encoding="utf-8").splitlines()
    assert len(tsv) == 1 + 9
    assert "4.500 CPU-hours" in (out / "stream.log").read_text(encoding="utf-8")
    assert "sha256:fakedigest" in (out / "stream.log").read_text(encoding="utf-8")
    assert "algo-6.0.0+f40a2b27677a" in (out / "stream.log").read_text(encoding="utf-8")


def test_a_failed_month_removes_its_partial_files_and_stops(env):
    r = sh({**env, "FAKE_FAIL_MONTH": "2025-11"})
    assert r.returncode == 1
    out = Path(env["EXP29_OUT"])
    assert not list((out / "2025-11").glob("*.part"))
    months = [line.split("\t")[0] for line in
              (out / "stream-cpu.tsv").read_text(encoding="utf-8").splitlines()[1:]]
    assert months == ["2025-09", "2026-04", "2026-06", "2025-10"]
    assert "2025-11: exit 3" in (out / "stream.log").read_text(encoding="utf-8")


def test_an_existing_month_is_never_resumed(env):
    (Path(env["EXP29_OUT"]) / "2025-10").mkdir(parents=True)
    r = sh(env)
    assert r.returncode == 1
    assert "never resumed" in r.stdout

"""EXP-0029's stream.sh with a stand-in for docker: the dry run prints the nine commands and
streams nothing; the guard refuses a df it cannot parse; the preflight refuses an image with no
cgroup cpu.stat or another cli id; a stream sums the containers' own CPU; a failed month is
removed whole and the stream stops; a complete month is skipped and a partial one refused; a
signal stops the container and removes the month; the 70 GB cap stops the stream."""

from __future__ import annotations

import json
import os
import signal
import subprocess
import time
from pathlib import Path

import pytest

SCRIPT = Path(__file__).resolve().parent.parent / "experiments" / "EXP-0029" / "stream.sh"
SALT = "80b7ed61df63b5e94e96089d100887cef8465b13028260352639573bee9ae5d4"
ALGO = "algo-6.0.0+f40a2b27677a"
FAKE_DOCKER = r"""#!/bin/sh
# A stand-in for docker: records each call, answers the image checks, and "streams" a month by
# writing its files and a cpu.stat (or failing for $FAKE_FAIL_MONTH, or hanging for
# $FAKE_HANG_MONTH until it is stopped).
echo "$*" >> "$FAKE_CALLS"
case "$1" in
  image) echo "sha256:fakedigest"; exit 0 ;;
  stop) touch "$FAKE_OUT/.stopped"; exit 0 ;;
esac
case "$*" in
  *"--entrypoint cli"*) printf '{"components": {"id": "inner"},\n "id": "%s"}\n' "${FAKE_ALGO:-@ALGO@}"; exit 0 ;;
  *"cohort --help"*) exit 0 ;;
  *"cat /sys/fs/cgroup/cpu.stat'"*|*"-c cat /sys/fs/cgroup/cpu.stat"*)
    [ -n "${FAKE_NO_CGROUP:-}" ] || echo "usage_usec 19344"; exit 0 ;;
esac
m=$(printf '%s' "$*" | sed -n 's/.*--month \([0-9-]*\).*/\1/p')
mkdir -p "$FAKE_OUT/$m"
if [ "$m" = "${FAKE_FAIL_MONTH:-}" ]; then
  touch "$FAKE_OUT/$m/games.jsonl.part" "$FAKE_OUT/$m/players.jsonl"
  echo "sha256 mismatch"
  exit 3
fi
if [ "$m" = "${FAKE_HANG_MONTH:-}" ]; then
  touch "$FAKE_OUT/$m/games.jsonl.part"
  while [ ! -e "$FAKE_OUT/.stopped" ]; do sleep 0.2; done
  exit 137
fi
echo '{"salt": "@SALT@", "player_rate": 0.9}' > "$FAKE_OUT/$m/meta.json"
touch "$FAKE_OUT/$m/players.jsonl"
case "$*" in *--games*) touch "$FAKE_OUT/$m/games.jsonl" ;; esac
echo "exp29-cpu-stat"
echo "usage_usec 1800000000"
exit 0
""".replace("@SALT@", SALT).replace("@ALGO@", ALGO)


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
        "EXP29_PYTHON": "python",
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
                          check=False, timeout=120)


def tsv_months(out: Path) -> list[str]:
    lines = (out / "stream-cpu.tsv").read_text(encoding="utf-8").splitlines()
    assert lines[0] == "month\tkind\tusage_usec"
    return [line.split("\t")[0] for line in lines[1:]]


def complete_month(out: Path, m: str, games: bool, rate: float = 0.9) -> None:
    (out / m).mkdir(parents=True)
    (out / m / "meta.json").write_text(json.dumps({"salt": SALT, "player_rate": rate}))
    (out / m / "players.jsonl").touch()
    if games:
        (out / m / "games.jsonl").touch()


def test_the_dry_run_runs_the_preflight_and_prints_the_nine_commands(env):
    r = sh(env, "--dry-run")
    assert r.returncode == 0, r.stdout + r.stderr
    cmds = [line for line in r.stdout.splitlines() if "dry run: " in line and "cohort --month" in line]
    assert len(cmds) == 9
    assert sum("--games" in c for c in cmds) == 6
    for m in ("2025-09", "2026-04", "2026-06"):
        assert all("--games" not in c for c in cmds if f"--month {m}" in c)
    assert all("--player-rate 0.9" in c and "/sys/fs/cgroup/cpu.stat" in c for c in cmds)
    assert "usage_usec 19344" in r.stdout
    assert f"the image's cli is {ALGO}" in r.stdout  # the top-level id, not the nested one
    assert "would stop" not in r.stdout
    assert "nothing streamed" in r.stdout
    assert not Path(env["EXP29_OUT"]).exists()


def test_a_df_that_does_not_parse_stops_before_anything(env, tmp_path):
    bad = tmp_path / "bin" / "df"
    bad.write_text("#!/bin/sh\necho 'Avail'\necho 'n/a'\n", encoding="utf-8")
    bad.chmod(0o755)
    r = sh({**env, "EXP29_DF": str(bad)}, "--dry-run")
    assert r.returncode == 2
    assert "did not parse a number" in r.stdout + r.stderr


def test_the_preflight_refuses_an_image_without_cpu_stat_or_with_another_cli(env):
    r = sh({**env, "FAKE_NO_CGROUP": "1"})
    assert r.returncode == 1 and "no usage_usec" in r.stdout
    r = sh({**env, "FAKE_ALGO": "algo-5.0.0+3cabedf4c935"})
    assert r.returncode == 1 and "not the registered" in r.stdout
    assert not Path(env["EXP29_OUT"]).exists()


def test_a_stream_sums_the_containers_cpu(env):
    r = sh(env)
    assert r.returncode == 0, r.stdout + r.stderr
    out = Path(env["EXP29_OUT"])
    assert len(tsv_months(out)) == 9
    log = (out / "stream.log").read_text(encoding="utf-8")
    assert "4.500 CPU-hours" in log and "sha256:fakedigest" in log and ALGO in log


def test_a_failed_month_is_removed_whole_and_the_stream_stops(env):
    r = sh({**env, "FAKE_FAIL_MONTH": "2025-11"})
    assert r.returncode == 1
    out = Path(env["EXP29_OUT"])
    assert not (out / "2025-11").exists()
    assert tsv_months(out) == ["2025-09", "2026-04", "2026-06", "2025-10"]
    assert "2025-11: exit 3" in (out / "stream.log").read_text(encoding="utf-8")


def test_complete_months_are_skipped_and_the_tsv_is_appended(env):
    out = Path(env["EXP29_OUT"])
    r = sh({**env, "FAKE_FAIL_MONTH": "2025-11"})
    assert r.returncode == 1
    calls = Path(env["FAKE_CALLS"])
    calls.write_text("")
    r = sh(env)
    assert r.returncode == 0, r.stdout + r.stderr
    assert "2025-09: complete, skipped" in r.stdout
    assert "--month 2025-09" not in calls.read_text(encoding="utf-8")
    assert tsv_months(out) == ["2025-09", "2026-04", "2026-06", "2025-10", "2025-11", "2025-12",
                               "2026-01", "2026-02", "2026-03"]


def test_a_partial_month_is_refused(env):
    out = Path(env["EXP29_OUT"])
    complete_month(out, "2025-10", games=False)  # a games month without games.jsonl
    r = sh(env)
    assert r.returncode == 1 and "partial month" in r.stdout
    complete_month(out / "x", "2025-09", games=False, rate=1.0)  # another rate is not complete
    (out / "2025-10").rename(out / "y")
    (out / "x" / "2025-09").rename(out / "2025-09")
    r = sh(env)
    assert r.returncode == 1 and "2025-09 is a partial month" in r.stdout


def test_a_signal_stops_the_container_and_removes_the_month(env):
    out = Path(env["EXP29_OUT"])
    p = subprocess.Popen(["sh", str(SCRIPT)], env={**env, "FAKE_HANG_MONTH": "2025-09"},
                         stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    for _ in range(100):
        if (out / "2025-09" / "games.jsonl.part").exists():
            break
        time.sleep(0.1)
    p.send_signal(signal.SIGTERM)
    stdout, _ = p.communicate(timeout=30)
    assert p.returncode != 0
    assert "signal received during 2025-09" in stdout
    assert (out / ".stopped").exists()  # docker stop was called
    assert not (out / "2025-09").exists()


def test_the_cap_on_the_cohort_folder_stops_the_stream(env):
    out = Path(env["EXP29_OUT"])
    out.mkdir(parents=True)
    (out / "filler").write_bytes(b"\0" * (2 * 1024 * 1024))
    r = sh({**env, "EXP29_CAP_GB": "1"})  # 1 GB used (rounded up) + 1 GB reserve > 1 GB
    assert r.returncode == 1 and "passes the 1 GB cap" in r.stdout

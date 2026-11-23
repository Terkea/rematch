"""Experiments under lab/experiments/EXP-NNNN/ (ADR 0106, docs/experiments/TEMPLATE.md).

Each experiment's `analysis.py` is one entry point: `python analysis.py --out DIR` writes
`DIR/results.json` (and any figures under `DIR/figures/`) and nothing else. `check` enforces the
two rules CI needs:

1. Determinism: two runs give byte-identical `results.json`, equal to the committed one when the
   experiment has results.
2. The analysis is frozen: once `results.json` is committed, the sha256 of `analysis.py` must be
   the one the pre-registration states (`Analysis code sha256: <hex>`).

    python -m rematch_lab.experiments check            # every experiment
    python -m rematch_lab.experiments check EXP-0001   # one
    python -m rematch_lab.experiments hash EXP-0001    # print analysis.py's sha256
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any

LAB_ROOT = Path(__file__).resolve().parent.parent
EXPERIMENTS = LAB_ROOT / "experiments"
EXP_ID = re.compile(r"^EXP-\d{4}$")
HASH_LINE = re.compile(r"Analysis code sha256:\s*`?([0-9a-f]{64})`?", re.IGNORECASE)


def canonical(data: Any) -> str:
    """The results format: sorted keys, two-space indent, a final newline, NaN refused."""
    return json.dumps(data, sort_keys=True, indent=2, ensure_ascii=False, allow_nan=False) + "\n"


def write_results(out_dir: str | Path, data: Any) -> Path:
    """Writes `results.json` in the canonical format; experiments call this."""
    out = Path(out_dir)
    out.mkdir(parents=True, exist_ok=True)
    path = out / "results.json"
    path.write_text(canonical(data), encoding="utf-8")
    return path


def seed(exp_id: str, name: str) -> int:
    """A stable 32-bit seed for one named random step of an experiment (no wall clock)."""
    digest = hashlib.sha256(f"{exp_id}/{name}".encode()).digest()
    return int.from_bytes(digest[:4], "big")


def experiments(only: list[str] | None = None) -> list[Path]:
    dirs = sorted(p for p in EXPERIMENTS.iterdir() if p.is_dir() and EXP_ID.match(p.name))
    if only:
        missing = set(only) - {d.name for d in dirs}
        if missing:
            raise SystemExit(f"no such experiment: {', '.join(sorted(missing))}")
        dirs = [d for d in dirs if d.name in only]
    return dirs


def analysis_hash(exp: Path) -> str:
    return hashlib.sha256((exp / "analysis.py").read_bytes()).hexdigest()


def run_once(exp: Path, out: Path) -> bytes:
    subprocess.run(
        [sys.executable, "-P", str(exp / "analysis.py"), "--out", str(out)],
        cwd=exp,
        check=True,
        env={"PYTHONPATH": str(LAB_ROOT), "PYTHONHASHSEED": "0", "MPLBACKEND": "Agg"},
    )
    return (out / "results.json").read_bytes()


def check(exp: Path) -> list[str]:
    """The problems with one experiment; empty means it passes."""
    problems = []
    for required in ("preregistration.md", "analysis.py"):
        if not (exp / required).is_file():
            problems.append(f"{exp.name}: no {required}")
    if problems:
        return problems
    committed = exp / "results.json"
    if committed.exists():
        stated = HASH_LINE.search((exp / "preregistration.md").read_text(encoding="utf-8"))
        actual = analysis_hash(exp)
        if not stated:
            problems.append(f"{exp.name}: the pre-registration states no analysis code sha256")
        elif stated.group(1) != actual:
            problems.append(
                f"{exp.name}: analysis.py changed after its results were committed "
                f"(pre-registered {stated.group(1)[:12]}, now {actual[:12]}); a change is a "
                "deviation or a new experiment"
            )
    with tempfile.TemporaryDirectory() as a, tempfile.TemporaryDirectory() as b:
        first = run_once(exp, Path(a))
        second = run_once(exp, Path(b))
    if first != second:
        problems.append(f"{exp.name}: two runs gave different results.json")
    if committed.exists() and committed.read_bytes() != first:
        problems.append(f"{exp.name}: the rerun differs from the committed results.json")
    return problems


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(prog="python -m rematch_lab.experiments")
    sub = parser.add_subparsers(dest="cmd", required=True)
    c = sub.add_parser("check", help="rerun experiments twice and compare results.json")
    c.add_argument("ids", nargs="*")
    h = sub.add_parser("hash", help="print the sha256 of an experiment's analysis.py")
    h.add_argument("id")
    args = parser.parse_args(argv)
    if args.cmd == "hash":
        print(analysis_hash(experiments([args.id])[0]))
        return 0
    found = experiments(args.ids or None)
    problems = [p for exp in found for p in check(exp)]
    for p in problems:
        print(p, file=sys.stderr)
    print(f"checked {len(found)} experiment(s), {len(problems)} problem(s)")
    return 1 if problems else 0


if __name__ == "__main__":
    raise SystemExit(main())

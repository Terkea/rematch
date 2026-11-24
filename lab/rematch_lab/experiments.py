"""Experiments under lab/experiments/EXP-NNNN/ (ADR 0106, docs/experiments/TEMPLATE.md).

Each experiment's `analysis.py` is one entry point: `python analysis.py --out DIR` writes
`DIR/results.json` (with the `algorithm_id` it ran on) and any figures under `DIR/figures/`, and
nothing else. `check` enforces what CI needs:

1. Registered first: in git, `preregistration.md` and `analysis.py` were added in a commit
   strictly before the one that added `results.json` (with `--history`, which CI passes).
2. Frozen: once results exist, sections 0 to 10 of the pre-registration are the text they had in
   the commit that added the results, and `analysis.py` has the sha256 that section 10 states.
3. Checked inputs: every line of `inputs.sha256` (`<hex>  <path from the experiment folder>`)
   matches its file.
4. Deterministic: two runs give byte-identical `results.json`. For an experiment whose results
   ran on the current algorithm (`algorithm_id`), the rerun also equals the committed file. An
   older experiment is historical: its rules 1 to 3 are checked, but it is not rerun on an
   algorithm it never measured.
5. Worth running (ADR 0107): section 0 of the pre-registration is a value case with every field
   of the template, and once results exist, `results.json` records its compute budget against
   the actual (`compute`), with no unit past the registered cap (150% unless section 0 says
   more). EXP-0001 and EXP-0002 were registered before the gate and are exempt.

    python -m rematch_lab.experiments check [--history] [EXP-0001 ...]
    python -m rematch_lab.experiments hash EXP-0001    # print analysis.py's sha256
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any

LAB_ROOT = Path(__file__).resolve().parent.parent
EXPERIMENTS = LAB_ROOT / "experiments"
EXP_ID = re.compile(r"^EXP-\d{4}$")
SECTION = re.compile(r"^## (\d+)\.", re.MULTILINE)
HASH_LINE = re.compile(r"^- Analysis code sha256:\s*`([0-9a-f]{64})`\s*$", re.MULTILINE)
# The value case of section 0 (ADR 0107), field by field, in the template's order.
VALUE_FIELDS = (
    "- Decision it informs:",
    "- If the result is ",
    "- Expected cost:",
    "- Value score:",
    "- Cheaper alternative?",
    "- Compute budget:",
)
VALUE_SCORE = re.compile(r"impact ([1-3]) × uncertainty ([1-3]) ÷ cost ([1-3]) = ([0-9.]+)")
BUDGET_CAP = re.compile(r"^- Compute budget:.*\bcap (\d+)%", re.MULTILINE)
DEFAULT_CAP = 1.5
# Registered before the value gate existed.
VALUE_GATE_EXEMPT = frozenset({"EXP-0001", "EXP-0002"})


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


class BudgetExceeded(RuntimeError):
    """A run spent more than its registered cap of a unit; it is a failed result."""


class Budget:
    """The compute budget of section 0, in units the run counts deterministically.

    budget = Budget({"games": 3840, "profile_calls": 50000})  # cap 150% unless registered
    budget.spend("games", 120)  # raises BudgetExceeded past the cap
    write_results(out, {..., "compute": budget.record()})
    """

    def __init__(self, budget: dict[str, float], cap: float = DEFAULT_CAP) -> None:
        if not budget or any(v <= 0 for v in budget.values()) or cap < 1:
            raise ValueError("a budget needs positive units and a cap of at least 1")
        self.budget = dict(budget)
        self.cap = cap
        self.actual = {unit: 0 for unit in budget}

    def spend(self, unit: str, amount: float = 1) -> None:
        if unit not in self.budget:
            raise KeyError(f"{unit} is not a budgeted unit")
        self.actual[unit] += amount
        if self.actual[unit] > self.cap * self.budget[unit]:
            raise BudgetExceeded(
                f"{unit}: spent {self.actual[unit]:g} of a budget of {self.budget[unit]:g} "
                f"(cap {self.cap:.0%}); the run stops here"
            )

    def record(self) -> dict[str, Any]:
        return {"budget": self.budget, "actual": self.actual, "cap": self.cap}


def seed(exp_id: str, name: str) -> int:
    """A stable 32-bit seed for one named random step (every name is listed in section 7)."""
    digest = hashlib.sha256(f"{exp_id}/{name}".encode()).digest()
    return int.from_bytes(digest[:4], "big")


def experiments(only: list[str] | None = None) -> list[Path]:
    if not EXPERIMENTS.is_dir():
        return []
    dirs = sorted(p for p in EXPERIMENTS.iterdir() if p.is_dir() and EXP_ID.match(p.name))
    if only:
        missing = set(only) - {d.name for d in dirs}
        if missing:
            raise SystemExit(f"no such experiment: {', '.join(sorted(missing))}")
        dirs = [d for d in dirs if d.name in only]
    return dirs


def analysis_hash(exp: Path) -> str:
    return hashlib.sha256((exp / "analysis.py").read_bytes()).hexdigest()


def sections(text: str) -> dict[int, str]:
    """The numbered `## N.` sections of a pre-registration, by number."""
    marks = list(SECTION.finditer(text))
    return {
        int(m.group(1)): text[m.start() : marks[i + 1].start() if i + 1 < len(marks) else None]
        for i, m in enumerate(marks)
    }


def frozen_part(text: str) -> str:
    """Sections 0 to 10: what may not change once results exist."""
    s = sections(text)
    return "".join(s.get(n, "") for n in range(0, 11))


def stated_hash(text: str) -> str | None:
    """The analysis sha256 of section 10 (a hash anywhere else does not count)."""
    m = HASH_LINE.search(sections(text).get(10, ""))
    return m.group(1) if m else None


def registered_cap(text: str) -> float:
    """The budget cap of section 0 as a factor: 1.5 unless a higher one is registered."""
    m = BUDGET_CAP.search(sections(text).get(0, ""))
    return max(DEFAULT_CAP, int(m.group(1)) / 100) if m else DEFAULT_CAP


def value_case_problems(exp: Path, text: str) -> list[str]:
    """Section 0 has every field, in order, and its value score's arithmetic holds."""
    if exp.name in VALUE_GATE_EXEMPT:
        return []
    value = sections(text).get(0)
    if value is None:
        return [f"{exp.name}: no section 0 (the value case) in the pre-registration"]
    problems = []
    at = 0
    for field in VALUE_FIELDS:
        found = value.find(f"\n{field}", at)
        if found < 0:
            name = field[2:].strip()
            problems.append(f'{exp.name}: the value case has no "{name}" (or it is out of order)')
        else:
            at = found + 1
    score = VALUE_SCORE.search(value)
    if score is None:
        problems.append(f"{exp.name}: the value score is not impact × uncertainty ÷ cost = S")
    else:
        impact, uncertainty, cost = (int(score.group(i)) for i in (1, 2, 3))
        if abs(float(score.group(4)) - impact * uncertainty / cost) > 0.05:
            problems.append(f"{exp.name}: the value score is {impact * uncertainty / cost:g}")
    return problems


def compute_problems(exp: Path, text: str, results: dict[str, Any]) -> list[str]:
    """results.json records the budget against the actual, with no unit past the cap."""
    if exp.name in VALUE_GATE_EXEMPT:
        return []
    compute = results.get("compute")
    if not isinstance(compute, dict) or not {"budget", "actual"} <= compute.keys():
        return [f"{exp.name}: results.json has no compute budget and actual (`compute`)"]
    cap = registered_cap(text)
    problems = []
    for unit, spent in compute["actual"].items():
        if unit not in compute["budget"]:
            problems.append(f"{exp.name}: compute unit {unit} has no budget")
        elif spent > cap * compute["budget"][unit]:
            problems.append(f"{exp.name}: {unit} spent {spent:g}, past the cap of {cap:.0%}")
    return problems


def current_algorithm_id() -> str:
    from .bridge import call_raw

    out = call_raw(["lab", "profile"], '{"schema_version": "lab-v1", "players": []}')
    return json.loads(out)["algorithm_id"]


def run_once(exp: Path, out: Path) -> bytes:
    env = {
        **os.environ,
        "PYTHONPATH": str(LAB_ROOT),
        "PYTHONHASHSEED": "0",
        "MPLBACKEND": "Agg",
    }
    subprocess.run(
        [sys.executable, "-P", str(exp / "analysis.py"), "--out", str(out)],
        cwd=exp,
        check=True,
        env=env,
    )
    return (out / "results.json").read_bytes()


def git(exp: Path, *args: str) -> str:
    return subprocess.run(
        ["git", "-C", str(exp), *args], capture_output=True, text=True, check=True
    ).stdout.strip()


def first_added(exp: Path, name: str) -> str | None:
    """The commit that first added `exp/name`, or None if git never saw it."""
    log = git(exp, "log", "--diff-filter=A", "--format=%H", "--", name)
    return log.splitlines()[-1] if log else None


def history_problems(exp: Path) -> list[str]:
    """Rule 1 and the frozen sections, from git."""
    added = first_added(exp, "results.json")
    if added is None:
        return [f"{exp.name}: results.json is not committed"]
    problems = []
    for name in ("preregistration.md", "analysis.py"):
        reg = first_added(exp, name)
        if reg is None or reg == added:
            problems.append(f"{exp.name}: {name} was not committed before results.json")
            continue
        ancestor = subprocess.run(
            ["git", "-C", str(exp), "merge-base", "--is-ancestor", reg, added], check=False
        )
        if ancestor.returncode != 0:
            problems.append(f"{exp.name}: {name} was not committed before results.json")
    rel = git(exp, "rev-parse", "--show-prefix")
    then = git(exp, "show", f"{added}:{rel}preregistration.md")
    now = (exp / "preregistration.md").read_text(encoding="utf-8")
    if frozen_part(then) != frozen_part(now):
        problems.append(
            f"{exp.name}: sections 0 to 10 of the pre-registration changed after the results "
            "were committed; write a deviation in section 11 or register a new experiment"
        )
    return problems


def input_problems(exp: Path) -> list[str]:
    manifest = exp / "inputs.sha256"
    if not manifest.exists():
        return []
    problems = []
    for line in manifest.read_text(encoding="utf-8").splitlines():
        if not line.strip() or line.startswith("#"):
            continue
        want, _, rel = line.partition("  ")
        path = (exp / rel.strip()).resolve()
        if not path.is_file():
            problems.append(f"{exp.name}: input {rel.strip()} is missing")
        elif hashlib.sha256(path.read_bytes()).hexdigest() != want.strip():
            problems.append(f"{exp.name}: input {rel.strip()} does not match its sha256")
    return problems


def check(exp: Path, *, history: bool = False, algorithm_id: str | None = None) -> list[str]:
    """The problems with one experiment; empty means it passes."""
    problems = []
    for required in ("preregistration.md", "analysis.py"):
        if not (exp / required).is_file():
            problems.append(f"{exp.name}: no {required}")
    if problems:
        return problems
    text = (exp / "preregistration.md").read_text(encoding="utf-8")
    problems += value_case_problems(exp, text)
    problems += input_problems(exp)
    committed = exp / "results.json"
    historical = False
    if committed.exists():
        stated = stated_hash(text)
        actual = analysis_hash(exp)
        if not stated:
            problems.append(f"{exp.name}: section 10 states no analysis code sha256")
        elif stated != actual:
            problems.append(
                f"{exp.name}: analysis.py changed after its results were committed "
                f"(pre-registered {stated[:12]}, now {actual[:12]}); a change is a new experiment"
            )
        if history:
            problems += history_problems(exp)
        results = json.loads(committed.read_text(encoding="utf-8"))
        problems += compute_problems(exp, text, results)
        ran_on = results.get("algorithm_id")
        if not ran_on:
            problems.append(f"{exp.name}: results.json does not say which algorithm_id it ran on")
        elif algorithm_id is not None and ran_on != algorithm_id:
            historical = True
            print(f"{exp.name}: historical (ran on {ran_on}, the cli is {algorithm_id})")
    if historical:
        return problems
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
    c = sub.add_parser("check", help="the registration, freeze, input and determinism rules")
    c.add_argument("--history", action="store_true", help="check the order of commits in git")
    c.add_argument("ids", nargs="*")
    h = sub.add_parser("hash", help="print the sha256 of an experiment's analysis.py")
    h.add_argument("id")
    args = parser.parse_args(argv)
    if args.cmd == "hash":
        print(analysis_hash(experiments([args.id])[0]))
        return 0
    found = experiments(args.ids or None)
    algorithm_id = current_algorithm_id() if found else None
    problems = [p for exp in found for p in check(exp, history=args.history, algorithm_id=algorithm_id)]
    for p in problems:
        print(p, file=sys.stderr)
    print(f"checked {len(found)} experiment(s), {len(problems)} problem(s)")
    return 1 if problems else 0


if __name__ == "__main__":
    raise SystemExit(main())

"""Experiments under lab/experiments/EXP-NNNN/ (ADR 0106, docs/experiments/TEMPLATE.md).

Each experiment's `analysis.py` is one entry point: `python analysis.py --out DIR` writes
`DIR/results.json` (with the `algorithm_id` it ran on) and any figures under `DIR/figures/`, and
nothing else. `check` enforces what CI needs:

1. Registered first: in git, `preregistration.md` and `analysis.py` were added in a commit
   strictly before the one that added `results.json` (with `--history`, which CI passes).
2. Frozen: once results exist, sections 0 to 10 of the pre-registration are the text they had in
   the commit that added the results, and `analysis.py` has the sha256 that section 10 states.
3. Checked inputs: every line of `inputs.sha256` (`<hex>  <path from the experiment folder>`)
   matches its file. An external input (`<hex>  external:<path in the data directory>`) is a
   large public file kept out of git in the data directory (`$REMATCH_DATA_DIR`, by default the
   checkout's git-ignored `data/`), such as a pinned snapshot of the Lichess puzzle database. When
   it is present it must match; when it is absent, as in CI, the check says so and does not rerun
   the experiment, since its answer cannot be reproduced without its data.
4. Deterministic: two runs give byte-identical `results.json`. Only an experiment whose
   `results.json` is committed is rerun; without one the check says "not rerun: no results
   committed", since running `analysis.py` would start the experiment itself. For one whose results
   ran on the current algorithm (`algorithm_id`), the rerun also equals the committed file. Once
   the algorithm moves on, a decided experiment stays live (ADR 0109): `rematch_lab.arena`
   re-runs it on its schedule against the champion, so this check does not rerun it twice more;
   its rules 1 to 3 and 5 are still checked here.
6. Live or retired (ADR 0109): a decided experiment's `summary.md` says `live: true` with a
   `rerun` of per-pr, weekly or release, its measured `run_cpu_hours` and `run_minutes`, and an
   `arena.py` with its comparison rule; or `live: false` with the reason in `retired`.
5. Worth running (ADR 0107): section 0 of the pre-registration is a value case with every field
   of the template, and once results exist, `results.json` records its compute budget against
   the actual (`compute`), with no unit past the registered cap (150% unless section 0 says
   more). EXP-0001 and EXP-0002 were registered before the gate and are exempt.
6. Closed unrun (the kill rule, ADR 0107): an experiment with a `decision.md` and no
   `results.json` was closed before it ran. Its value case and inputs are still checked; it is
   never run.
7. The maths (ADR 0118, the `[math]` problems): `math.md` has the six headings of
   docs/experiments/MATH-TEMPLATE.md (`MATH_SECTIONS`), with sections 1 to 4 filled once the
   experiment is frozen (its `exp-nnnn-freeze` tag), running or has results, and all six once it
   has a `decision.md`; and `math_check.py`, run in the experiment folder, exits 0.
   `REMATCH_MATH_RULE=warn` (the default until every experiment is backfilled) prints them as
   warnings; `REMATCH_MATH_RULE=fail` makes them problems.

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
VALUE_SCORE = re.compile(r"impact ([1-3]) × uncertainty ([1-3]) ÷ cost ([1-3]) = ([0-9]+(?:\.[0-9]+)?)")
BUDGET_CAP = re.compile(r"^- Compute budget:.*\bcap (\d+)%", re.MULTILINE)
DEFAULT_CAP = 1.5
# How often a live experiment is re-run against the champion (ADR 0109), cheapest first.
RERUNS = ("per-pr", "weekly", "release")
FRONT_MATTER = re.compile(r"^---\n(.*?)\n---\n", re.DOTALL)
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


EXTERNAL = "external:"


def data_dir() -> Path:
    """Where external inputs live: `$REMATCH_DATA_DIR`, or the checkout's git-ignored `data/`."""
    return Path(os.environ.get("REMATCH_DATA_DIR") or LAB_ROOT.parent / "data")


def external_input(rel: str) -> Path:
    """The path of an external input, `rel` inside the data directory; experiments call this."""
    return data_dir() / rel


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


def front_matter(text: str) -> dict[str, str]:
    """The flat `key: value` front matter of a summary.md (quotes stripped)."""
    m = FRONT_MATTER.match(text.replace("\r\n", "\n"))
    if not m:
        return {}
    meta = {}
    for line in m.group(1).splitlines():
        key, sep, value = line.partition(":")
        if sep and re.fullmatch(r"[a-z_]+", key.strip()):
            v = value.strip()
            if len(v) >= 2 and v[0] == v[-1] and v[0] in "\"'":
                v = v[1:-1]
            meta[key.strip()] = v
    return meta


def summary_meta(exp: Path) -> dict[str, str]:
    path = exp / "summary.md"
    return front_matter(path.read_text(encoding="utf-8")) if path.is_file() else {}


def _positive(value: str | None) -> bool:
    try:
        return value is not None and float(value) > 0
    except ValueError:
        return False


def metadata_problems(exp: Path) -> list[str]:
    """Rule 6: a decided experiment is live with a schedule and a rule, or retired with a reason."""
    meta = summary_meta(exp)
    if meta.get("status") != "decided":
        return []
    live = meta.get("live")
    if live == "false":
        if not meta.get("retired"):
            return [f"{exp.name}: live: false needs the reason in `retired`"]
        return []
    if live != "true":
        return [f"{exp.name}: a decided experiment says `live: true` or `live: false` in summary.md"]
    problems = []
    if meta.get("rerun") not in RERUNS:
        problems.append(f"{exp.name}: `rerun` is one of {', '.join(RERUNS)}")
    for key in ("run_cpu_hours", "run_minutes"):
        if not _positive(meta.get(key)):
            problems.append(f"{exp.name}: `{key}` is the measured cost of one run, above 0")
    if not (exp / "arena.py").is_file():
        problems.append(f"{exp.name}: a live experiment has an arena.py with its comparison rule")
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


def input_lines(exp: Path) -> list[tuple[str, str]]:
    """`(sha256, path)` for each line of `inputs.sha256`; an external path keeps its prefix."""
    manifest = exp / "inputs.sha256"
    if not manifest.exists():
        return []
    out = []
    for line in manifest.read_text(encoding="utf-8").splitlines():
        if not line.strip() or line.startswith("#"):
            continue
        want, _, rel = line.partition("  ")
        out.append((want.strip(), rel.strip()))
    return out


def sha256_of(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def missing_external(exp: Path) -> list[str]:
    """The external inputs of `exp` that are not on this machine."""
    return [
        rel
        for _, rel in input_lines(exp)
        if rel.startswith(EXTERNAL) and not external_input(rel[len(EXTERNAL) :]).is_file()
    ]


def input_problems(exp: Path) -> list[str]:
    problems = []
    for want, rel in input_lines(exp):
        if rel.startswith(EXTERNAL):
            path = external_input(rel[len(EXTERNAL) :])
            if not path.is_file():
                continue  # reported by check(), which then skips the rerun
        else:
            path = (exp / rel).resolve()
        if not path.is_file():
            problems.append(f"{exp.name}: input {rel} is missing")
        elif sha256_of(path) != want:
            problems.append(f"{exp.name}: input {rel} does not match its sha256")
    return problems


def in_git(exp: Path) -> bool:
    inside = subprocess.run(
        ["git", "-C", str(exp), "rev-parse", "--is-inside-work-tree"],
        capture_output=True,
        text=True,
        check=False,
    )
    return inside.returncode == 0 and inside.stdout.strip() == "true"


def results_committed(exp: Path) -> bool:
    """`results.json` is here and, in a git checkout, tracked: a run's fresh output is not."""
    if not (exp / "results.json").is_file():
        return False
    if not in_git(exp):
        return True
    tracked = subprocess.run(
        ["git", "-C", str(exp), "ls-files", "--error-unmatch", "results.json"],
        capture_output=True,
        check=False,
    )
    return tracked.returncode == 0


# ---- the maths (ADR 0118, docs/experiments/MATH-TEMPLATE.md) --------------------------------

#: The headings of math.md, exactly as docs/experiments/MATH-TEMPLATE.md has them.
MATH_SECTIONS = (
    "## 1. Notation",
    "## 2. Model",
    "## 3. Estimators",
    "## 4. Derivations and proofs",
    "## 5. Numbers",
    "## 6. Assumptions that can fail",
)
#: Due once the experiment is frozen (its `exp-nnnn-freeze` tag) or has results.
MATH_BEFORE_RUN = (1, 2, 3, 4)
#: Due with the decision.
MATH_ALL = (1, 2, 3, 4, 5, 6)
MATH_RULES = ("warn", "fail")
MATH_CHECK_TIMEOUT = 1800
COMMENT = re.compile(r"<!--.*?-->", re.DOTALL)


def math_rule() -> str:
    """`REMATCH_MATH_RULE`: warn (the default until the backfill is done) or fail."""
    rule = os.environ.get("REMATCH_MATH_RULE", "warn").strip() or "warn"
    if rule not in MATH_RULES:
        raise SystemExit(f"REMATCH_MATH_RULE is {rule!r}; it is one of {', '.join(MATH_RULES)}")
    return rule


def freeze_tag(exp: Path) -> str:
    return f"{exp.name.lower()}-freeze"


def frozen(exp: Path) -> bool:
    """The experiment's freeze tag exists in its git repository."""
    if not in_git(exp):
        return False
    tags = subprocess.run(
        ["git", "-C", str(exp), "tag", "-l", freeze_tag(exp)],
        capture_output=True,
        text=True,
        check=False,
    )
    return tags.stdout.strip() == freeze_tag(exp)


def math_due(exp: Path) -> tuple[int, ...]:
    """The math.md sections due now: all six with a decision, 1 to 4 once frozen or run."""
    if (exp / "decision.md").is_file():
        return MATH_ALL
    running = summary_meta(exp).get("status") in ("running", "decided")
    if running or (exp / "results.json").is_file() or frozen(exp):
        return MATH_BEFORE_RUN
    return ()


def math_sections(text: str) -> dict[int, str]:
    """The body of each heading of MATH_SECTIONS found in `text`, comments removed."""
    lines = text.replace("\r\n", "\n").split("\n")
    marks = [
        (i, MATH_SECTIONS.index(line.rstrip()) + 1)
        for i, line in enumerate(lines)
        if line.rstrip() in MATH_SECTIONS
    ]
    out = {}
    for k, (start, n) in enumerate(marks):
        end = marks[k + 1][0] if k + 1 < len(marks) else len(lines)
        out[n] = COMMENT.sub("", "\n".join(lines[start + 1 : end])).strip()
    return out


def run_math_check(exp: Path) -> str | None:
    """Runs math_check.py in the experiment folder; the failure, or None when it exits 0."""
    env = {**os.environ, "PYTHONPATH": str(LAB_ROOT), "PYTHONHASHSEED": "0", "MPLBACKEND": "Agg"}
    try:
        done = subprocess.run(
            [sys.executable, "-P", "math_check.py"],
            cwd=exp,
            env=env,
            capture_output=True,
            text=True,
            timeout=MATH_CHECK_TIMEOUT,
            check=False,
        )
    except subprocess.TimeoutExpired:
        return f"math_check.py ran past {MATH_CHECK_TIMEOUT} s"
    if done.returncode == 0:
        return None
    tail = " | ".join((done.stderr or done.stdout).strip().splitlines()[-5:])
    return f"math_check.py exited {done.returncode}: {tail}"


def math_problems(exp: Path) -> list[str]:
    """The `math` problems: math.md has the six headings with the due ones filled, and
    math_check.py exits 0. A math_check.py is run whenever there is one."""
    due = math_due(exp)
    problems = []
    md = exp / "math.md"
    if due and not md.is_file():
        problems.append(f"{exp.name}: [math] no math.md (docs/experiments/MATH-TEMPLATE.md)")
    elif md.is_file() and due:
        found = math_sections(md.read_text(encoding="utf-8"))
        for n, heading in enumerate(MATH_SECTIONS, start=1):
            if n not in found:
                problems.append(f'{exp.name}: [math] math.md has no "{heading}"')
            elif n in due and not found[n]:
                problems.append(f'{exp.name}: [math] "{heading}" of math.md is empty')
    check_file = exp / "math_check.py"
    if not check_file.is_file():
        if due:
            problems.append(f"{exp.name}: [math] no math_check.py")
        return problems
    failure = run_math_check(exp)
    if failure:
        problems.append(f"{exp.name}: [math] {failure}")
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
    problems += metadata_problems(exp)
    maths = math_problems(exp)
    if math_rule() == "fail":
        problems += maths
    else:
        for p in maths:
            print(f"warning: {p} (REMATCH_MATH_RULE=warn; ADR 0118)")
    problems += input_problems(exp)
    committed = exp / "results.json"
    if not committed.exists() and (exp / "decision.md").is_file():
        print(f"{exp.name}: closed unrun (decision.md, no results.json)")
        return problems
    moved_on = False
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
            moved_on = True
            meta = summary_meta(exp)
            if meta.get("live") == "true":
                print(
                    f"{exp.name}: ran on {ran_on}, the cli is {algorithm_id}; live, re-run against "
                    f"the champion by the arena ({meta.get('rerun')})"
                )
            else:
                print(f"{exp.name}: ran on {ran_on}, the cli is {algorithm_id}; not live")
    if moved_on:
        return problems
    if not results_committed(exp):
        # Running it here would start the experiment itself, before its results are due.
        print(f"{exp.name}: not rerun: no results committed")
        return problems
    absent = missing_external(exp)
    if absent:
        print(f"{exp.name}: not rerun, its external input is not here ({', '.join(absent)})")
        return problems
    with tempfile.TemporaryDirectory() as a, tempfile.TemporaryDirectory() as b:
        first = run_once(exp, Path(a))
        second = run_once(exp, Path(b))
    if first != second:
        problems.append(f"{exp.name}: two runs gave different results.json")
    if committed.read_bytes() != first:
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

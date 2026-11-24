"""Champion and challenger (ADR 0108): every live experiment, re-run on the same registered seeds
and inputs for two algorithm versions, compared by the experiment's own rule.

The champion is the production algorithm (by default the manifest on main); the challenger is
the working tree's manifest or a built `cli`. For each live experiment in the scope:

1. Each side's record comes from the leaderboard (`lab/leaderboard.json`), or from the committed
   `results.json` when it ran on that version with the registered seeds, or from one run of the
   frozen `analysis.py` with `--algorithm <id>` and that version's `cli` as `$REMATCH_CLI`. A
   record is reused only with the same seed namespace, the same `analysis.py` and the same
   `inputs.sha256`, so both sides always ran the same experiment on the same players.
2. The plan (the runs the caches cannot answer) is costed from each experiment's measured
   `run_cpu_hours` and refused before anything runs when it is past `--max-cpu-hours`; a run is
   stopped at 150% of its measured `run_minutes` (times `$ARENA_SLOWDOWN` for a slower machine).
   Each experiment's own `Budget` still caps what it counts (ADR 0107).
3. `arena.py` in the experiment folder measures each side and compares them: per primary and
   guarded metric, the effect, its interval and a verdict (better, worse, no clear difference).
   A worse primary or guard fails the run unless `lab/overrides.json` overrides it with an ADR.

    python -m rematch_lab.arena --scope per-pr --dry-run           # the plan and what is cached
    python -m rematch_lab.arena --scope release --write            # run, then record
    python -m rematch_lab.arena --verify --require-records          # what the pull request job does

Exit status: 0 no regression, 1 a clear regression, 2 the plan was refused, 3 a record is
missing or a re-run differs from its committed record.
"""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import importlib.util
import json
import os
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path
from types import ModuleType
from typing import Any

from . import experiments as ex
from .stats import BETTER, NO_CLEAR, WORSE

LAB_ROOT = ex.LAB_ROOT
REPO_ROOT = LAB_ROOT.parent
LEADERBOARD = LAB_ROOT / "leaderboard.json"
OVERRIDES = LAB_ROOT / "overrides.json"
SCHEMA = "leaderboard-v1"
# A scope runs its own experiments and every cheaper one.
SCOPES = {
    "per-pr": ("per-pr",),
    "weekly": ("per-pr", "weekly"),
    "release": ("per-pr", "weekly", "release"),
}
# The CPU-hours one arena run may plan, unless --max-cpu-hours says otherwise.
MAX_CPU_HOURS = {"per-pr": 1.0, "weekly": 4.0, "release": 8.0}
RUN_CAP = ex.DEFAULT_CAP
OVERRIDDEN = "overridden"
NOT_RUN = "not run"
# The fields that identify a record's content; a re-run must reproduce them exactly.
CONTENT = (
    "experiment",
    "algorithm_id",
    "seed_namespace",
    "analysis_sha256",
    "inputs_sha256",
    "results_sha256",
    "measures",
    "metrics",
)


@dataclass(frozen=True)
class Live:
    exp: Path
    id: str
    rerun: str
    cpu_hours: float
    minutes: float


def live_experiments(scope: str, only: list[str] | None = None) -> list[Live]:
    """The decided, live experiments that the scope re-runs."""
    out = []
    for exp in ex.experiments(only):
        meta = ex.summary_meta(exp)
        if meta.get("status") != "decided" or meta.get("live") != "true":
            continue
        if meta.get("rerun") not in SCOPES[scope]:
            continue
        out.append(
            Live(
                exp=exp,
                id=exp.name,
                rerun=meta["rerun"],
                cpu_hours=float(meta["run_cpu_hours"]),
                minutes=float(meta["run_minutes"]),
            )
        )
    return out


def load_rule(exp: Path) -> ModuleType:
    """The experiment's comparison rule, `exp/arena.py`."""
    path = exp / "arena.py"
    spec = importlib.util.spec_from_file_location(f"arena_rule_{exp.name.replace('-', '_')}", path)
    if spec is None or spec.loader is None:
        raise SystemExit(f"{exp.name}: no arena.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def inputs_sha256(exp: Path) -> str | None:
    path = exp / "inputs.sha256"
    return sha256(path.read_bytes()) if path.is_file() else None


def seed_namespace(results: dict[str, Any]) -> str | None:
    for value in results.values():
        if isinstance(value, dict) and isinstance(value.get("seed_namespace"), str):
            return value["seed_namespace"]
    return None


def make_record(live: Live, raw: bytes) -> dict[str, Any]:
    results = json.loads(raw)
    rule = load_rule(live.exp)
    measures = rule.measure(results)
    return {
        "experiment": live.id,
        "algorithm_id": results["algorithm_id"],
        "seed_namespace": seed_namespace(results),
        "analysis_sha256": ex.analysis_hash(live.exp),
        "inputs_sha256": inputs_sha256(live.exp),
        "rule_sha256": sha256((live.exp / "arena.py").read_bytes()),
        "results_sha256": sha256(raw),
        "measures": measures,
        "metrics": rule.metrics(measures),
    }


def same_record(a: dict[str, Any] | None, b: dict[str, Any] | None) -> bool:
    if a is None or b is None:
        return False
    return all(ex.canonical(a.get(k)) == ex.canonical(b.get(k)) for k in CONTENT)


def committed_record(live: Live, algorithm_id: str) -> dict[str, Any] | None:
    """The committed results.json as a record, when it ran on this version with the registered
    seeds."""
    path = live.exp / "results.json"
    if not path.is_file():
        return None
    raw = path.read_bytes()
    results = json.loads(raw)
    if results.get("algorithm_id") != algorithm_id or seed_namespace(results) != live.id:
        return None
    return make_record(live, raw)


def load_board(path: Path) -> dict[str, Any]:
    if not path.is_file():
        return {"schema": SCHEMA, "specs": {}, "records": []}
    board = json.loads(path.read_text(encoding="utf-8"))
    if board.get("schema") != SCHEMA:
        raise SystemExit(f"{path}: schema is not {SCHEMA}")
    return board


def find_record(board: dict[str, Any], live: Live, algorithm_id: str) -> dict[str, Any] | None:
    """The newest record of this version on this experiment, on the registered seeds and the
    current analysis and inputs."""
    analysis, inputs = ex.analysis_hash(live.exp), inputs_sha256(live.exp)
    found = [
        r
        for r in board["records"]
        if r["experiment"] == live.id
        and r["algorithm_id"] == algorithm_id
        and r["seed_namespace"] == live.id
        and r["analysis_sha256"] == analysis
        and r["inputs_sha256"] == inputs
    ]
    return found[-1] if found else None


def upsert(board: dict[str, Any], record: dict[str, Any]) -> None:
    """One record per experiment, version and champion it was compared with; a re-run replaces."""
    key = (record["experiment"], record["algorithm_id"], record["seed_namespace"], record["against"])
    board["records"] = [
        r
        for r in board["records"]
        if (r["experiment"], r["algorithm_id"], r["seed_namespace"], r["against"]) != key
    ] + [record]
    board["records"].sort(key=lambda r: (r["experiment"], r["date"], r["algorithm_id"]))


def load_overrides(path: Path) -> list[dict[str, Any]]:
    if not path.is_file():
        return []
    return json.loads(path.read_text(encoding="utf-8"))


def override_for(
    overrides: list[dict[str, Any]], algorithm_id: str, exp_id: str, metric: str
) -> dict[str, Any] | None:
    """An override counts only when the ADR that records it exists."""
    for o in overrides:
        if o.get("algorithm_id") != algorithm_id or o.get("experiment") != exp_id:
            continue
        if metric not in o.get("metrics", []):
            continue
        adr = Path(o.get("adr", ""))
        if not adr.is_absolute():
            adr = REPO_ROOT / adr
        if o.get("reason") and adr.is_file():
            return o
    return None


def run_side(live: Live, algorithm_id: str, cli: str | None) -> dict[str, Any]:
    """One run of the frozen analysis on the registered seeds, with that version's cli."""
    env = {
        **os.environ,
        "PYTHONPATH": str(LAB_ROOT),
        "PYTHONHASHSEED": "0",
        "MPLBACKEND": "Agg",
    }
    if cli:
        env["REMATCH_CLI"] = cli
    slowdown = float(os.environ.get("ARENA_SLOWDOWN", "1"))
    timeout = live.minutes * 60 * RUN_CAP * slowdown
    with tempfile.TemporaryDirectory() as out:
        try:
            subprocess.run(
                [
                    sys.executable,
                    "-P",
                    str(live.exp / "analysis.py"),
                    "--out",
                    out,
                    "--algorithm",
                    algorithm_id,
                ],
                cwd=live.exp,
                check=True,
                env=env,
                timeout=timeout,
            )
        except subprocess.TimeoutExpired as e:
            raise ex.BudgetExceeded(
                f"{live.id} on {algorithm_id}: stopped at {timeout / 60:.0f} minutes, 150% of its "
                f"measured {live.minutes:g} (ARENA_SLOWDOWN {slowdown:g})"
            ) from e
        raw = (Path(out) / "results.json").read_bytes()
    record = make_record(live, raw)
    if record["seed_namespace"] != live.id:
        raise SystemExit(f"{live.id}: the run did not use the registered seeds")
    return record


def experiment_verdict(
    live: Live, rule: ModuleType, comparison: dict[str, Any], challenger: str, overrides: list
) -> str:
    roles = {m["name"]: m["role"] for m in rule.METRICS}
    worse, overridden = False, False
    for name, c in comparison.items():
        if c["verdict"] != WORSE:
            continue
        o = override_for(overrides, challenger, live.id, name)
        if o:
            c["override"] = {"adr": o["adr"], "reason": o["reason"]}
            overridden = True
        else:
            worse = True
    if worse:
        return WORSE
    if overridden:
        return OVERRIDDEN
    if any(c["verdict"] == BETTER and roles.get(n) == "primary" for n, c in comparison.items()):
        return BETTER
    return NO_CLEAR


def manifest_id(text: str) -> str:
    return json.loads(text)["id"]


def resolve_champion(args: argparse.Namespace) -> str:
    if args.champion:
        return args.champion
    for ref in [args.champion_ref] if args.champion_ref else ["origin/main", "main"]:
        shown = subprocess.run(
            ["git", "-C", str(REPO_ROOT), "show", f"{ref}:algorithm/manifest.json"],
            capture_output=True,
            text=True,
            check=False,
        )
        if shown.returncode == 0:
            return manifest_id(shown.stdout)
    raise SystemExit("no champion: pass --champion ID or a --champion-ref with a manifest")


def resolve_challenger(args: argparse.Namespace) -> str:
    if args.challenger:
        return args.challenger
    if args.challenger_cli:
        out = subprocess.run(
            [args.challenger_cli, "algo", "show", "--json"],
            capture_output=True,
            text=True,
            check=True,
        ).stdout
        return manifest_id(out)
    return manifest_id((REPO_ROOT / "algorithm" / "manifest.json").read_text(encoding="utf-8"))


def fmt_interval(iv: list[float] | None) -> str:
    return f"[{iv[0]:.3f}, {iv[1]:.3f}]" if iv else ""


def render(report: dict[str, Any]) -> str:
    lines = [
        f"## Arena: {report['challenger']} against the champion {report['champion']}",
        "",
        f"Scope `{report['scope']}`: {len(report['experiments'])} live experiment(s); "
        f"{len(report['plan'])} run(s) planned, {report['cpu_hours']:g} CPU-hours of "
        f"{report['max_cpu_hours']:g}.",
        "",
    ]
    if report["refused"]:
        lines += [f"Refused: {report['refused']}", ""]
    for p in report["problems"]:
        lines.append(f"- Problem: {p}")
    for e in report["experiments"]:
        lines += ["", f"### {e['experiment']}: {e['verdict']}", ""]
        if not e["comparison"]:
            continue
        lines += [
            "| Metric | Role | Champion | Challenger | Effect | Interval | Verdict |",
            "|---|---|---|---|---|---|---|",
        ]
        for m in e["metric_specs"]:
            n = m["name"]
            a, b, c = e["champion_metrics"][n], e["challenger_metrics"][n], e["comparison"][n]
            verdict = c["verdict"]
            if "override" in c:
                verdict += f" (overridden, {c['override']['adr']})"
            lines.append(
                f"| {m['label']} | {m['role']} | {a['value']:.3f} {fmt_interval(a['interval'])} "
                f"| {b['value']:.3f} {fmt_interval(b['interval'])} | {c['effect']:+.3f} "
                f"| {fmt_interval(c['interval'])} ({c['level']}) | {verdict} |"
            )
    return "\n".join(lines) + "\n"


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(prog="python -m rematch_lab.arena")
    ap.add_argument("--scope", choices=list(SCOPES), default="release")
    ap.add_argument("--only", nargs="*", help="these experiments only")
    ap.add_argument("--champion", help="the champion's algorithm_id")
    ap.add_argument("--champion-ref", help="a git ref whose algorithm/manifest.json is the champion")
    ap.add_argument("--champion-cli", help="a cli of the champion, for a champion run")
    ap.add_argument("--challenger", help="the challenger's algorithm_id")
    ap.add_argument("--challenger-cli", help="a cli of the challenger (default $REMATCH_CLI)")
    ap.add_argument("--dry-run", action="store_true", help="plan and compare caches, run nothing")
    ap.add_argument("--verify", action="store_true", help="re-run the challenger, match its record")
    ap.add_argument("--require-records", action="store_true", help="the challenger is on the board")
    ap.add_argument("--write", action="store_true", help="record the results in the leaderboard")
    ap.add_argument("--leaderboard", type=Path, default=LEADERBOARD)
    ap.add_argument("--write-to", type=Path, help="write the leaderboard here instead")
    ap.add_argument("--overrides", type=Path, default=OVERRIDES)
    ap.add_argument("--max-cpu-hours", type=float)
    ap.add_argument("--date", default=dt.datetime.now(dt.UTC).date().isoformat())
    ap.add_argument("--json", type=Path, help="write the report as JSON")
    ap.add_argument("--summary", type=Path, help="append the Markdown report (a job summary)")
    args = ap.parse_args(argv)

    champion = resolve_champion(args)
    challenger = resolve_challenger(args)
    max_cpu = args.max_cpu_hours if args.max_cpu_hours is not None else MAX_CPU_HOURS[args.scope]
    board = load_board(args.leaderboard)
    overrides = load_overrides(args.overrides)
    lives = live_experiments(args.scope, args.only)

    def cached(live: Live, algorithm_id: str) -> dict[str, Any] | None:
        return find_record(board, live, algorithm_id) or committed_record(live, algorithm_id)

    records: dict[tuple[str, str], dict[str, Any] | None] = {}
    plan: list[dict[str, Any]] = []
    problems: list[str] = []
    for live in lives:
        sides = [champion] if champion == challenger else [champion, challenger]
        for algorithm_id in sides:
            hit = cached(live, algorithm_id)
            if algorithm_id == challenger and args.verify:
                hit = None
            records[(live.id, algorithm_id)] = hit
            if hit is None:
                plan.append(
                    {"experiment": live.id, "algorithm_id": algorithm_id, "cpu_hours": live.cpu_hours}
                )
        if args.require_records and find_record(board, live, challenger) is None:
            problems.append(
                f"{live.id}: {challenger} is not on the leaderboard; run the arena with --write "
                "and commit lab/leaderboard.json"
            )
    cpu = round(sum(p["cpu_hours"] for p in plan), 6)
    report: dict[str, Any] = {
        "champion": champion,
        "challenger": challenger,
        "scope": args.scope,
        "plan": plan,
        "cpu_hours": cpu,
        "max_cpu_hours": max_cpu,
        # A champion run needs the champion's cli (--champion-cli); analysis.py refuses any other.
        "needs_champion_cli": any(
            p["algorithm_id"] == champion and champion != challenger for p in plan
        ),
        "refused": None,
        "problems": problems,
        "experiments": [],
    }
    clis = {champion: args.champion_cli, challenger: args.challenger_cli}
    if not args.dry_run:
        if cpu > max_cpu:
            report["refused"] = (
                f"the plan needs {cpu:g} CPU-hours, past the budget of {max_cpu:g}; nothing ran"
            )
    if not args.dry_run and not report["refused"]:
        by_id = {live.id: live for live in lives}
        for p in plan:
            live = by_id[p["experiment"]]
            cli = clis.get(p["algorithm_id"]) or os.environ.get("REMATCH_CLI")
            record = run_side(live, p["algorithm_id"], cli)
            records[(live.id, p["algorithm_id"])] = record
            if p["algorithm_id"] == challenger and args.verify:
                stored = find_record(board, live, challenger)
                if stored is not None and not same_record(stored, record):
                    problems.append(
                        f"{live.id}: the re-run of {challenger} differs from its committed record"
                    )

    for live in lives:
        rule = load_rule(live.exp)
        a, b = records.get((live.id, champion)), records.get((live.id, challenger))
        entry: dict[str, Any] = {
            "experiment": live.id,
            "rerun": live.rerun,
            "metric_specs": rule.METRICS,
            "verdict": NOT_RUN,
            "comparison": None,
        }
        if a is not None and b is not None:
            comparison = rule.compare(a["measures"], b["measures"])
            entry["comparison"] = comparison
            entry["champion_metrics"] = a["metrics"]
            entry["challenger_metrics"] = b["metrics"]
            entry["verdict"] = experiment_verdict(live, rule, comparison, challenger, overrides)
            if args.write and not args.dry_run:
                board.setdefault("specs", {})[live.id] = rule.METRICS
                if champion != challenger and "date" not in a:
                    blank = {"date": args.date, "against": None, "comparison": None, "verdict": None}
                    upsert(board, {**a, **blank})
                upsert(
                    board,
                    {
                        **b,
                        "date": args.date,
                        "against": champion,
                        "comparison": comparison,
                        "verdict": entry["verdict"],
                    },
                )
        report["experiments"].append(entry)

    if args.write and not args.dry_run and not report["refused"]:
        target = args.write_to or args.leaderboard
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(ex.canonical(board), encoding="utf-8")
    text = render(report)
    print(text)
    if args.summary:
        with args.summary.open("a", encoding="utf-8") as f:
            f.write(text)
    if args.json:
        args.json.write_text(ex.canonical(report), encoding="utf-8")
    if report["refused"]:
        return 2
    if problems:
        return 3
    return 1 if any(e["verdict"] == WORSE for e in report["experiments"]) else 0


if __name__ == "__main__":
    raise SystemExit(main())

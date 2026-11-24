"""The bridge to the Rust algorithm (ADR 0106): one `cli` process per call, JSON on stdin and stdout.

The binary is `$REMATCH_CLI` (in the lab image, `/usr/local/bin/cli` from a Rematch build). Every
request and every response is checked against `lab/schemas/lab-v1.schema.json` before it is
parsed into its pydantic model, so a drift between the Rust and Python sides fails loudly.
"""

from __future__ import annotations

import json
import os
import subprocess
from functools import cache
from pathlib import Path
from typing import Any

import jsonschema
from pydantic import BaseModel

from .models import (
    SCHEMA_VERSION,
    AnalysePlayer,
    AnalyseRequest,
    AnalyseResponse,
    PlayerFacts,
    PlayerProfile,
    ProfileRequest,
    ProfileResponse,
    RankRequest,
    RankResponse,
)

SCHEMA_PATH = Path(__file__).resolve().parent.parent / "schemas" / "lab-v1.schema.json"


class LabError(RuntimeError):
    """The cli refused a request or failed; `stderr` holds its message."""

    def __init__(self, message: str, stderr: str = "") -> None:
        super().__init__(f"{message}\n{stderr}".rstrip())
        self.stderr = stderr


def cli_path() -> str:
    return os.environ.get("REMATCH_CLI", "cli")


@cache
def schema() -> dict[str, Any]:
    return json.loads(SCHEMA_PATH.read_text(encoding="utf-8"))


@cache
def validator(message: str) -> jsonschema.Draft202012Validator:
    s = dict(schema())
    s["$ref"] = f"#/$defs/{message}"
    return jsonschema.Draft202012Validator(s, format_checker=jsonschema.FormatChecker())


def check(message: str, doc: Any) -> None:
    """Raises LabError unless `doc` is a valid `message` of the schema."""
    errors = sorted(validator(message).iter_errors(doc), key=lambda e: list(e.absolute_path))
    if errors:
        lines = [f"{'/'.join(map(str, e.absolute_path)) or '<root>'}: {e.message}" for e in errors]
        raise LabError(f"{message} does not match lab-v1:\n  " + "\n  ".join(lines[:20]))


def to_json(model: BaseModel) -> str:
    return model.model_dump_json(by_alias=True)


def call_raw(command: list[str], stdin: str, env: dict[str, str] | None = None) -> str:
    """Runs `cli <command...>` with `stdin`; returns stdout. Raises LabError on a non-zero exit."""
    proc = subprocess.run(
        [cli_path(), *command],
        input=stdin,
        capture_output=True,
        text=True,
        encoding="utf-8",
        env={**os.environ, **(env or {})},
        check=False,
    )
    if proc.returncode != 0:
        raise LabError(f"cli {' '.join(command)} exited with {proc.returncode}", proc.stderr)
    return proc.stdout


def lab_call(step: str, request: BaseModel, request_message: str, response_message: str) -> Any:
    payload = json.loads(to_json(request))
    check(request_message, payload)
    out = json.loads(call_raw(["lab", step], json.dumps(payload)))
    check(response_message, out)
    return out


def analyse(
    players: list[AnalysePlayer] | AnalyseRequest,
    *,
    nodes: int | None = None,
    engines: int | None = None,
) -> AnalyseResponse:
    """Games to facts: the production analysis and build-profile (needs a database and Stockfish)."""
    req = (
        players
        if isinstance(players, AnalyseRequest)
        else AnalyseRequest(players=players, nodes=nodes, engines=engines)
    )
    return AnalyseResponse.model_validate(
        lab_call("analyse", req, "AnalyseRequest", "AnalyseResponse")
    )


def profile(facts: AnalyseResponse | ProfileRequest | list[PlayerFacts]) -> ProfileResponse:
    """Facts to per-player rates with 80% intervals (`compute_estimates`)."""
    if isinstance(facts, ProfileRequest):
        req = facts
    elif isinstance(facts, AnalyseResponse):
        req = ProfileRequest(algorithm_id=facts.algorithm_id, players=facts.players)
    else:
        req = ProfileRequest(players=facts)
    return ProfileResponse.model_validate(
        lab_call("profile", req, "ProfileRequest", "ProfileResponse")
    )


def rank(
    profiles: ProfileResponse | RankRequest | list[PlayerProfile], *, first_report: bool = False
) -> RankResponse:
    """Rates to the ranked weaknesses, the coach's focuses and their drills."""
    if isinstance(profiles, RankRequest):
        req = profiles
    elif isinstance(profiles, ProfileResponse):
        req = RankRequest(
            algorithm_id=profiles.algorithm_id, first_report=first_report, players=profiles.players
        )
    else:
        req = RankRequest(first_report=first_report, players=profiles)
    return RankResponse.model_validate(lab_call("rank", req, "RankRequest", "RankResponse"))


def algorithm(which: str | None = None) -> dict[str, Any]:
    """`cli algo show --json` (ADR 0105): this build's manifest, or the one `which` names (an
    id stored in `algorithm_versions`, which needs DATABASE_URL, or a manifest file)."""
    out = json.loads(call_raw(["algo", "show", *([which] if which else []), "--json"], ""))
    if not str(out.get("id", "")).startswith("algo-"):
        raise LabError("cli algo show --json gave no algorithm id")
    return out


def algorithm_diff(a: str, b: str) -> dict[str, Any]:
    """`cli algo diff A B --json`: both ids and every changed leaf of the manifest."""
    return json.loads(call_raw(["algo", "diff", a, b, "--json"], ""))


def bench(
    suite: str = "smoke",
    *,
    out_dir: str | Path,
    nodes: int | None = None,
    engines: int | None = None,
    baseline: str | Path | None = None,
    suite_file: str | Path | None = None,
) -> dict[str, Any]:
    """`cli bench run --json` (ADR 0100): the Tier 1 benchmark's full results as a dict.

    `suite_file` runs a suite read from JSON (the Rust `bench::suite::Suite` shape: name,
    players with name, flaw `[id, rate]` or null, games and seed, checkpoints, nodes,
    opponents, perturb) instead of the named `suite`; an experiment that needs more players
    than the named suites writes one (`cli bench run --suite-file`).

    Needs DATABASE_URL and STOCKFISH_PATH. Exit code 2 (a regression against `baseline`) is
    raised as LabError with the cli's message.
    """
    if suite_file is not None:
        args = ["bench", "run", "--suite-file", str(suite_file), "--json", "--out", str(out_dir)]
    elif suite not in ("smoke", "full"):
        raise ValueError("suite must be smoke or full")
    else:
        args = ["bench", "run", "--suite", suite, "--json", "--out", str(out_dir)]
    if nodes is not None:
        args += ["--nodes", str(int(nodes))]
    if engines is not None:
        args += ["--engines", str(int(engines))]
    if baseline is not None:
        args += ["--baseline", str(baseline)]
    out = json.loads(call_raw(args, ""))
    for field in ("scorecard", "suite", "players"):
        if field not in out:
            raise LabError(f"cli bench run --json gave no {field!r}")
    return out


__all__ = [
    "SCHEMA_VERSION",
    "LabError",
    "algorithm",
    "algorithm_diff",
    "analyse",
    "bench",
    "call_raw",
    "check",
    "profile",
    "rank",
]

"""The bridge to the Rust algorithm (ADR 0106): one `cli` process per call, JSON on stdin and stdout.
The web's rules (the weekly focus, the trainer's sources) go through the same kind of step in
Node, `$REMATCH_WEB_BRIDGE` (ADR 0115).

The binary is `$REMATCH_CLI` (in the lab image, `/usr/local/bin/cli` from a Rematch build). Every
request and every response is checked against `lab/schemas/lab-v1.schema.json` before it is
parsed into its pydantic model, so a drift between the Rust and Python sides fails loudly.
"""

from __future__ import annotations

import json
import os
import shlex
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
    DepthGame,
    FocusProgressItem,
    FocusProgressRequest,
    FocusProgressResponse,
    FocusTargetItem,
    FocusTargetRequest,
    FocusTargetResponse,
    EvaluateRequest,
    EvaluateResponse,
    JudgeRequest,
    JudgeResponse,
    ObserveRequest,
    ObserveResponse,
    MotifLine,
    PlayerFacts,
    PlayerProfile,
    PositionsRequest,
    PositionsResponse,
    ProfileRequest,
    ProfileResponse,
    RankRequest,
    RankResponse,
    TagMotifsRequest,
    TagMotifsResponse,
    TimelinePlayer,
    TimelineRequest,
    TimelineResponse,
    TrainerCandidate,
    TrainerSourcesRequest,
    TrainerSourcesResponse,
    TrajectoryPlayer,
    TrajectoryRequest,
    TrajectoryResponse,
)

SCHEMA_PATH = Path(__file__).resolve().parent.parent / "schemas" / "lab-v1.schema.json"
#: The bundle of the web's Node step in the lab image (lab/Dockerfile, ADR 0115).
DEFAULT_WEB_BRIDGE = "/usr/local/lib/rematch/lab-bridge.mjs"


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


def variant_env(variant: str | None) -> dict[str, str]:
    """`ALGO_VARIANT` for one call (EXP-0006): `champion`, `challenger` or `dev:<options>`
    (`profile_core::stats::variant`). `None` runs the build's production rule and clears any
    `ALGO_VARIANT` the lab's own environment has, so a call never inherits one by accident."""
    return {"ALGO_VARIANT": variant or ""}


def lab_call(
    step: str,
    request: BaseModel,
    request_message: str,
    response_message: str,
    variant: str | None = None,
) -> Any:
    payload = json.loads(to_json(request))
    check(request_message, payload)
    out = json.loads(call_raw(["lab", step], json.dumps(payload), env=variant_env(variant)))
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


def profile(
    facts: AnalyseResponse | ProfileRequest | list[PlayerFacts], *, variant: str | None = None
) -> ProfileResponse:
    """Facts to per-player rates with 80% intervals (`compute_estimates`), under the prior rule
    of `variant` (`ALGO_VARIANT`; `None` is production's)."""
    if isinstance(facts, ProfileRequest):
        req = facts
    elif isinstance(facts, AnalyseResponse):
        req = ProfileRequest(algorithm_id=facts.algorithm_id, players=facts.players)
    else:
        req = ProfileRequest(players=facts)
    return ProfileResponse.model_validate(
        lab_call("profile", req, "ProfileRequest", "ProfileResponse", variant)
    )


def rank(
    profiles: ProfileResponse | RankRequest | list[PlayerProfile],
    *,
    first_report: bool = False,
    variant: str | None = None,
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
    return RankResponse.model_validate(lab_call("rank", req, "RankRequest", "RankResponse", variant))


def tag_motifs(lines: list[MotifLine] | TagMotifsRequest) -> TagMotifsResponse:
    """Lines to motif tags: each `pv` is a move's best line through the production feature
    derivation (`derive_move_features`, which calls the tagger). Pure: no engine, no database."""
    req = lines if isinstance(lines, TagMotifsRequest) else TagMotifsRequest(lines=lines)
    return TagMotifsResponse.model_validate(
        lab_call("tag-motifs", req, "TagMotifsRequest", "TagMotifsResponse")
    )


def positions(games: list[DepthGame] | PositionsRequest) -> PositionsResponse:
    """The positions production's `evaluate_game` asks the engine for, per game (EXP-0008). Pure."""
    req = games if isinstance(games, PositionsRequest) else PositionsRequest(games=games)
    return PositionsResponse.model_validate(
        lab_call("positions", req, "PositionsRequest", "PositionsResponse")
    )


def evaluate(
    fens: list[str] | EvaluateRequest,
    *,
    nodes: int | None = None,
    hash_mb: int | None = None,
    syzygy: bool = False,
    engines: int | None = None,
) -> EvaluateResponse:
    """Each FEN searched by **Stockfish** as `evaluate_game` searches it (one thread, `ucinewgame`,
    `go nodes`, MultiPV 1), at `nodes`, with the production hash unless `hash_mb` says otherwise and
    the Syzygy tables only when `syzygy` (needs `STOCKFISH_PATH`, and `SYZYGY_PATH` for the tables)."""
    if isinstance(fens, EvaluateRequest):
        req = fens
    else:
        if nodes is None:
            raise ValueError("evaluate needs nodes")
        req = EvaluateRequest(nodes=nodes, hash_mb=hash_mb, syzygy=syzygy, engines=engines, fens=fens)
    return EvaluateResponse.model_validate(
        lab_call("evaluate", req, "EvaluateRequest", "EvaluateResponse")
    )


def judge(req: JudgeRequest) -> JudgeResponse:
    """Each game judged on the given searches through `evaluate_game`, `build_move_records` and
    `game_accuracy`, as the `analyse-game` job does (EXP-0008). Pure."""
    return JudgeResponse.model_validate(lab_call("judge", req, "JudgeRequest", "JudgeResponse"))


def observe(req: ObserveRequest) -> ObserveResponse:
    """Each Lichess game's weakness observations from Lichess's own evaluations, through the
    production feature path with no enrichment (`bench::observe`, EXP-0009): accuracy, judgement
    counts and the keys whose detectors need no enrichment input. Pure."""
    return ObserveResponse.model_validate(
        lab_call("observe", req, "ObserveRequest", "ObserveResponse")
    )


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
    variant: str | None = None,
) -> dict[str, Any]:
    """`cli bench run --json` (ADR 0100): the Tier 1 benchmark's full results as a dict.

    `suite_file` runs a suite read from JSON (the Rust `bench::suite::Suite` shape: name,
    players with name, flaw `[id, rate]` or null, games and seed, checkpoints, nodes,
    opponents, perturb) instead of the named `suite`; an experiment that needs more players
    than the named suites writes one (`cli bench run --suite-file`).

    `variant` runs that prior rule side by side (`ALGO_VARIANT`): every player's games also go
    into a second account built under it, and the output gains each player's `variant`
    checkpoints and the run's `variant` scorecard.

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
    out = json.loads(call_raw(args, "", env=variant_env(variant)))
    fields = ("scorecard", "suite", "players", *(("variant",) if variant else ()))
    for field in fields:
        if field not in out:
            raise LabError(f"cli bench run --json gave no {field!r}")
    return out


# ---- lab-v2 (#125): the trajectory, the timeline, the weekly focus and the trainer's sources ----


def trajectory(players: list[TrajectoryPlayer] | TrajectoryRequest) -> TrajectoryResponse:
    """Rated games to the trajectory (EXP-0015): `cli lab trajectory`, the loader's
    `trajectory_input`, `compute` and `contract` (ADR 0089). Each player's `trajectory` is the
    CoachingInput's as the rule computes it: `kind == "falling"` is "clearly worse than your
    peak". Since algo-5.0.0 the coach and the web never see `kind` or the rating's verdict
    (EXP-0015, ADR 0116); the lab still does. The bootstrap's seed is the player's `seed`. Pure.

    The caller passes what the builder reads from the database: the games in `(played_at, id)`
    order, and at most three weaknesses as `trajectory_weaknesses` picks them (the estimates with
    `priority > 0`, by priority then key, each with `prior_of` its estimate)."""
    req = players if isinstance(players, TrajectoryRequest) else TrajectoryRequest(players=players)
    return TrajectoryResponse.model_validate(
        lab_call("trajectory", req, "TrajectoryRequest", "TrajectoryResponse")
    )


def timeline(players: list[TimelinePlayer] | TimelineRequest) -> TimelineResponse:
    """Series points to change states (EXP-0014): `cli lab timeline`, the build's
    `evaluate_all` (`tl-v1`): each series' state, `P_imp` and the `weakness_state_history` rows.
    Game ids are UUIDs, since games of the same instant replay in id order. Pass the
    evaluation baselines (`model_reference`, from `analyse`) as the build adds them. Pure."""
    req = players if isinstance(players, TimelineRequest) else TimelineRequest(players=players)
    return TimelineResponse.model_validate(
        lab_call("timeline", req, "TimelineRequest", "TimelineResponse")
    )


def web_bridge() -> list[str]:
    """The web's Node step (ADR 0115): `$REMATCH_WEB_BRIDGE`, a command line (in the lab image
    `node /usr/local/lib/rematch/lab-bridge.mjs`, the bundle of `apps/web/scripts/lab-bridge.ts`)."""
    return shlex.split(os.environ.get("REMATCH_WEB_BRIDGE", f"node {DEFAULT_WEB_BRIDGE}"))


def web_call(step: str, request: BaseModel, request_message: str, response_message: str) -> Any:
    """Runs one step of the web's Node bridge, checking both messages against the schema."""
    payload = json.loads(to_json(request))
    check(request_message, payload)
    command = [*web_bridge(), step]
    proc = subprocess.run(
        command,
        input=json.dumps(payload),
        capture_output=True,
        text=True,
        encoding="utf-8",
        check=False,
    )
    if proc.returncode != 0:
        raise LabError(f"lab-bridge {step} exited with {proc.returncode}", proc.stderr)
    try:
        out = json.loads(proc.stdout)
    except json.JSONDecodeError as e:
        raise LabError(f"lab-bridge {step} wrote no JSON: {e}", proc.stderr) from e
    check(response_message, out)
    return out


def focus_target(items: list[FocusTargetItem] | FocusTargetRequest) -> FocusTargetResponse:
    """The weekly focus's target (EXP-0011): `chooseTarget` of `apps/web/src/server/focus/goal.ts`
    on each `baseline` and `peer`. The app offers the band median as `peer` only when the
    weakness is compared with peers and is not W5 (`focus/index.ts` `candidate`); otherwise null."""
    req = items if isinstance(items, FocusTargetRequest) else FocusTargetRequest(items=items)
    return FocusTargetResponse.model_validate(
        web_call("focus-target", req, "FocusTargetRequest", "FocusTargetResponse")
    )


def focus_progress(items: list[FocusProgressItem] | FocusProgressRequest) -> FocusProgressResponse:
    """The weekly focus's progress and outcome (EXP-0011): `focusProgress` of `goal.ts` on `k`
    events in `n` chances over `games` of `games_n` games, against `target`."""
    req = items if isinstance(items, FocusProgressRequest) else FocusProgressRequest(items=items)
    return FocusProgressResponse.model_validate(
        web_call("focus-progress", req, "FocusProgressRequest", "FocusProgressResponse")
    )


def trainer_sources(
    candidates: list[TrainerCandidate] | TrainerSourcesRequest,
) -> TrainerSourcesResponse:
    """The trainer's drillable positions (EXP-0012): `keepCandidate`, `primarySource` and
    `isHeldOut` of `apps/web/src/server/train/model.ts` on each candidate, and `SOURCES` in its
    order. A candidate's `sources` are the weakness series whose exemplar plies hold it, plus
    `blunder` when the move was judged one (`train/store.ts` `candidates`)."""
    req = (
        candidates
        if isinstance(candidates, TrainerSourcesRequest)
        else TrainerSourcesRequest(candidates=candidates)
    )
    return TrainerSourcesResponse.model_validate(
        web_call("trainer-sources", req, "TrainerSourcesRequest", "TrainerSourcesResponse")
    )


__all__ = [
    "SCHEMA_VERSION",
    "LabError",
    "algorithm",
    "algorithm_diff",
    "analyse",
    "bench",
    "call_raw",
    "check",
    "focus_progress",
    "focus_target",
    "profile",
    "rank",
    "tag_motifs",
    "timeline",
    "trainer_sources",
    "trajectory",
    "web_call",
]

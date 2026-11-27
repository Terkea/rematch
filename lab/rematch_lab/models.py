"""Typed messages of the lab bridge, `lab/schemas/lab-v1.schema.json` (ADR 0106).

The models mirror the schema's `$defs` field for field, and `tests/test_contract.py` checks that
they do: same names, same required fields, no extra field on either side. The envelope and the
lab's own objects are snake_case; the algorithm's own types (WeaknessEstimate, BandReferenceStat,
RankedWeakness) keep the camelCase the Rust code writes, through aliases.
"""

from __future__ import annotations

from datetime import datetime
from uuid import UUID
from typing import Literal

from pydantic import BaseModel, ConfigDict, Field, model_serializer, model_validator
from pydantic.alias_generators import to_camel

SCHEMA_VERSION = "lab-v1"

Band = Literal["B0", "B1", "B2", "B3", "B4", "B5"]
WeaknessState = Literal["not_an_issue", "emerging", "active", "improving", "resolved", "relapsed"]
Reason = Literal["ok", "no_benchmark", "insufficient_data", "no_events"]


class Snake(BaseModel):
    """A lab object: snake_case, no unknown field."""

    model_config = ConfigDict(extra="forbid", frozen=True)


class Camel(BaseModel):
    """An algorithm type: camelCase on the wire, snake_case in Python."""

    model_config = ConfigDict(
        extra="forbid", frozen=True, alias_generator=to_camel, populate_by_name=True
    )


# ---- facts ------------------------------------------------------------------------------------


class LabPoint(Snake):
    played_at: datetime
    events: int = Field(ge=0)
    opportunities: int = Field(ge=0)


class LabOutcome(Snake):
    state: WeaknessState
    p_imp: float | None = None


class BandReferenceStat(Camel):
    metric: str
    median: float
    p25: float
    p40: float
    p75: float
    pooled_rate: float | None
    epl_per_event: float | None
    opps_per_game: float | None
    opp_slope_per_100: float | None = Field(alias="oppSlopePer100")
    n_players: int
    n_games: int
    low_sample: bool


class PlayerFacts(Snake):
    id: str
    now: datetime
    band: Band
    games_in_window: int = Field(ge=0)
    series: dict[str, list[LabPoint]] = {}
    reference: dict[str, BandReferenceStat] | None = None
    model_reference: dict[str, BandReferenceStat] = {}
    outcomes: dict[str, LabOutcome] = {}


class PlayerCheck(Snake):
    id: str
    analysed_games: int
    profile_matches_build: bool
    mismatches: list[str]


class LabGame(Snake):
    id: str
    pgn: str
    played_at: datetime
    player_color: Literal["white", "black"]
    white_name: str
    black_name: str
    white_rating: int
    black_rating: int
    result: Literal["1-0", "0-1", "1/2-1/2"]
    termination: str
    base_seconds: int = Field(ge=0)
    increment_seconds: int = Field(default=0, ge=0)


class AnalysePlayer(Snake):
    id: str
    now: datetime | None = None
    games: list[LabGame]


class AnalyseRequest(Snake):
    schema_version: Literal["lab-v1"] = SCHEMA_VERSION
    nodes: int | None = None
    engines: int | None = None
    players: list[AnalysePlayer]


class AnalyseResponse(Snake):
    schema_version: Literal["lab-v1"]
    algorithm_id: str
    nodes: int
    players: list[PlayerFacts]
    checks: list[PlayerCheck]


class ProfileRequest(Snake):
    schema_version: Literal["lab-v1"] = SCHEMA_VERSION
    algorithm_id: str | None = None
    players: list[PlayerFacts]


# ---- profile ----------------------------------------------------------------------------------


class Interval(Snake):
    low: float
    high: float


class Posterior(Snake):
    alpha: float
    beta: float
    mean: float
    interval: Interval


class WeaknessEstimate(Camel):
    key: str
    k: float
    n: float
    n_unweighted: float
    k_unweighted: float
    posterior: Posterior
    band_median: float | None
    band_p40: float | None
    percentile: float | None
    confidence: float
    excess: float
    trainability: float
    priority: float
    precise: bool
    min_evidence_met: bool
    reason: Reason
    credible: bool
    model_baseline: bool
    invalid_points: int


class PlayerProfile(Snake):
    id: str
    now: datetime
    band: Band
    games_in_window: int
    estimates: list[WeaknessEstimate]
    outcomes: dict[str, LabOutcome] = {}


class ProfileResponse(Snake):
    schema_version: Literal["lab-v1"]
    algorithm_id: str
    players: list[PlayerProfile]


class RankRequest(Snake):
    schema_version: Literal["lab-v1"] = SCHEMA_VERSION
    algorithm_id: str | None = None
    first_report: bool = False
    players: list[PlayerProfile]


# ---- rank -------------------------------------------------------------------------------------


class RankedWeakness(Camel):
    """The CoachingInput's RankedWeakness. Its other fields (label, area, ...) are kept as is."""

    model_config = ConfigDict(
        extra="allow", frozen=True, alias_generator=to_camel, populate_by_name=True
    )

    id: str
    variant: str | None = None
    rank: int
    posterior_rate: float
    interval_low: float
    interval_high: float
    band_median: float
    confidence: float
    priority: float
    state: WeaknessState
    events: int
    opportunities: int
    exemplars: list[dict] = []


class PlayerRanking(Snake):
    id: str
    ranked: list[RankedWeakness]
    drills: dict[str, list[str]]
    focuses: list[str]
    focus_drills: dict[str, list[str]]


class RankResponse(Snake):
    schema_version: Literal["lab-v1"]
    algorithm_id: str
    players: list[PlayerRanking]


# ---- tag-motifs -------------------------------------------------------------------------------

MotifTag = Literal[
    "hangingPiece",
    "fork",
    "pin",
    "skewer",
    "discoveredAttack",
    "deflection",
    "sacrifice",
    "promotion",
    "mate",
    "mateIn1",
    "mateIn2",
    "mateIn3",
    "mateIn4Plus",
    "matingNet",
    "backRankMate",
    "smotheredMate",
]


class MotifLine(Snake):
    """A line to tag: `setup` is played from `fen`, then `pv` is the mover's best line."""

    id: str
    fen: str
    setup: list[str] = []
    pv: list[str]


class TagMotifsRequest(Snake):
    schema_version: Literal["lab-v1"] = SCHEMA_VERSION
    lines: list[MotifLine]


class LineAnalysis(Camel):
    """The algorithm's LineAnalysis: the settled swing and the motif tags."""

    gain: float
    depth: int | None
    mate_in: int | None
    tags: list[MotifTag]


class TaggedLine(Snake):
    id: str
    fen: str | None
    pov: Literal["white", "black"] | None
    best: LineAnalysis | None


class TagMotifsResponse(Snake):
    schema_version: Literal["lab-v1"]
    algorithm_id: str
    lines: list[TaggedLine]


# ---- engine depth (EXP-0008) -----------------------------------------------------------------


class EngineScore(Snake):
    """An engine score: exactly one of `cp` or `mate` (the other is left out on the wire)."""

    cp: int | None = None
    mate: int | None = None

    @model_validator(mode="after")
    def _one(self) -> EngineScore:
        if (self.cp is None) == (self.mate is None):
            raise ValueError("an engine score is exactly one of cp or mate")
        return self

    @model_serializer
    def _wire(self) -> dict[str, int]:
        return {"cp": self.cp} if self.cp is not None else {"mate": self.mate}  # type: ignore[dict-item]


class DepthGame(Snake):
    """A game for `positions` or `judge`; `lichess_evals` is Lichess's evaluation after each ply,
    White's point of view, as production stores a Lichess game's."""

    id: str
    pgn: str
    base_seconds: int | None = None
    increment_seconds: int | None = None
    lichess_evals: list[EngineScore | None] | None = None


class PositionsRequest(Snake):
    schema_version: Literal["lab-v1"] = SCHEMA_VERSION
    games: list[DepthGame]


class GamePositions(Snake):
    id: str
    plies: int
    terminal: bool
    fens: list[str]
    sequence: list[str | None]


class PositionsResponse(Snake):
    schema_version: Literal["lab-v1"]
    algorithm_id: str
    games: list[GamePositions]


class EvaluateRequest(Snake):
    schema_version: Literal["lab-v1"] = SCHEMA_VERSION
    nodes: int
    hash_mb: int | None = None
    syzygy: bool = False
    engines: int | None = None
    fens: list[str]


class Search(Snake):
    """One search: the side to move's score, as `evaluate_game` reads it."""

    fen: str
    score: EngineScore
    best_uci: str | None
    depth: int
    nodes: int
    cpu_ms: float | None


class EvaluateResponse(Snake):
    schema_version: Literal["lab-v1"]
    algorithm_id: str
    nodes: int
    hash_mb: int
    syzygy: bool
    threads: int
    searches: list[Search]


class GivenSearch(Snake):
    fen: str
    score: EngineScore
    best_uci: str | None = None


class JudgeRequest(Snake):
    schema_version: Literal["lab-v1"] = SCHEMA_VERSION
    games: list[DepthGame]
    searches: list[GivenSearch]


class JudgedMove(Snake):
    ply: int
    color: Literal["white", "black"]
    uci: str
    best_uci: str | None
    win_before: float
    win_after: float
    accuracy: float
    judgement: Literal["inaccuracy", "mistake", "blunder"] | None
    decided: bool


class JudgedGame(Snake):
    id: str
    moves: list[JudgedMove]
    accuracy_white: float
    accuracy_black: float
    lichess_used: int


class JudgeResponse(Snake):
    schema_version: Literal["lab-v1"]
    algorithm_id: str
    games: list[JudgedGame]


class ObserveGame(Snake):
    """A Lichess game for `observe` (EXP-0009): Lichess's evaluation after each ply, White's
    point of view, and the sides to observe."""

    id: str
    pgn: str
    base_seconds: int
    increment_seconds: int
    lichess_evals: list[EngineScore | None]
    sides: list[Literal["white", "black"]]
    white_rating: int | None = None
    black_rating: int | None = None
    termination: str | None = None


class ObserveRequest(Snake):
    schema_version: Literal["lab-v1"] = SCHEMA_VERSION
    games: list[ObserveGame]
    searches: list[GivenSearch]


class KeyCount(Snake):
    events: int
    opportunities: int


class ObservedSide(Snake):
    color: Literal["white", "black"]
    accuracy: float | None
    moves: int
    inaccuracies: int
    mistakes: int
    blunders: int
    keys: dict[str, KeyCount]


class ObservedGame(Snake):
    id: str
    error: str | None
    sides: list[ObservedSide]


class ObserveResponse(Snake):
    schema_version: Literal["lab-v1"]
    algorithm_id: str
    games: list[ObservedGame]


# ---- lab-v2: the trajectory and the timeline (cli lab, #125) -----------------------------------

GameResult = Literal["win", "draw", "loss", "unfinished"]
ChangeVerdict = Literal["better", "worse", "no_clear_change", "not_enough_games"]


class RatedGame(Snake):
    """One rated game as `load_trajectory` reads it: the player's own stored rating (on chess.com
    the rating after the game, on Lichess the one before it)."""

    id: str
    played_at: datetime
    rating: int | None = None
    result: GameResult
    analysed: bool = False
    accuracy: float | None = None
    blunders: int = Field(default=0, ge=0)


class GamePoint(Snake):
    game_id: str
    events: int = Field(ge=0)
    opportunities: int = Field(ge=0)


class LabPrior(Snake):
    mu: float
    n0: float


class TrajectoryWeakness(Snake):
    series_key: str
    prior: LabPrior
    points: list[GamePoint] = []


class TrajectoryPlayer(Snake):
    id: str
    seed: str
    chesscom: bool = False
    games: list[RatedGame]
    weaknesses: list[TrajectoryWeakness] = []


class TrajectoryRequest(Snake):
    schema_version: Literal["lab-v1"] = SCHEMA_VERSION
    # The rating rule (EXP-0030): "peak-bootstrap", "memory-halves" or "memory-before"; None runs
    # the production rule.
    rule: Literal["peak-bootstrap", "memory-halves", "memory-before"] | None = None
    players: list[TrajectoryPlayer]


class ColorMetric(Camel):
    value: float | None
    unit: Literal["rate", "per_game", "pct", "seconds", "elo"]
    n: int
    low: float | None
    high: float | None
    sufficient: bool
    need_more: int | None


class ThenNowMetric(Camel):
    then: ColorMetric
    now: ColorMetric
    verdict: ChangeVerdict
    diff: float | None
    diff_low: float | None
    diff_high: float | None


class ThenNowWeakness(Camel):
    series_key: str
    id: str
    variant: str | None
    label: str
    area: str
    then: ColorMetric
    now: ColorMetric
    verdict: ChangeVerdict
    then_events: int
    then_opportunities: int
    now_events: int
    now_opportunities: int


class TrajectoryWindow(Camel):
    basis: Literal["peak", "start", "last"]
    games: int
    from_: str = Field(alias="from")
    to: str


class RatingAt(Camel):
    rating: int
    date: str


class MonthRating(Camel):
    month: str
    games: int
    last: int
    high: int


class ThenVsNow(Camel):
    then: TrajectoryWindow
    now: TrajectoryWindow
    rating: ThenNowMetric
    accuracy: ThenNowMetric
    blunders_per_game: ThenNowMetric
    score: ThenNowMetric
    weaknesses: list[ThenNowWeakness]


class Trajectory(Camel):
    """The CoachingInput's `trajectory` (ADR 0089) as the rule computes it. `kind == "falling"`
    is `then_vs_now.rating.verdict == "worse"`. algo-5.0.0 withheld both (EXP-0015, ADR 0116);
    since algo-6.0.0 production's rule is EXP-0030's memory-halves and they are stated again
    (ADR 0117)."""

    rated_games: int
    sufficient: bool
    need_more: int | None
    kind: Literal["falling", "rising", "flat"] | None
    peak: RatingAt | None
    current: RatingAt | None
    change: int | None
    # The rule that computed `kind` (absent below the minimum); since algo-6.0.0 production's is
    # EXP-0030's "memory-halves" (ADR 0117).
    rating_rule: Literal["peak-bootstrap", "memory-halves", "memory-before"] | None = None
    months: list[MonthRating]
    then_vs_now: ThenVsNow | None


class PlayerTrajectory(Snake):
    id: str
    trajectory: Trajectory


class TrajectoryResponse(Snake):
    schema_version: Literal["lab-v1"]
    algorithm_id: str
    players: list[PlayerTrajectory]


class TimelinePoint(Snake):
    game_id: UUID
    played_at: datetime
    events: int = Field(ge=0)
    opportunities: int = Field(ge=0)


class TimelinePlayer(Snake):
    id: str
    reference: dict[str, BandReferenceStat] | None = None
    model_reference: dict[str, BandReferenceStat] = {}
    series: dict[str, list[TimelinePoint]]


class TimelineRequest(Snake):
    schema_version: Literal["lab-v1"] = SCHEMA_VERSION
    players: list[TimelinePlayer]


class Evidence(Snake):
    k: int
    n: int
    confidence: float


class LabOnset(Snake):
    game_id: UUID
    played_at: datetime
    low: datetime
    high: datetime


class LabTimelineRow(Snake):
    """One `weakness_state_history` row the build would write."""

    kind: Literal["transition", "onset"]
    from_: WeaknessState | None = Field(alias="from")
    to: WeaknessState
    at_game_id: UUID
    at_played_at: datetime
    onset: LabOnset | None
    p_change: float | None
    rate_before: float | None
    rate_after: float | None
    evidence: Evidence


class LabSeriesOutcome(Snake):
    state: WeaknessState
    p_imp: float | None
    invalid_points: int
    rows: list[LabTimelineRow]


class PlayerTimeline(Snake):
    id: str
    series: dict[str, LabSeriesOutcome]


class TimelineResponse(Snake):
    schema_version: Literal["lab-v1"]
    algorithm_id: str
    detector_version: str
    players: list[PlayerTimeline]


# ---- lab-v2: the web's rules, through Node (`lab-bridge`, ADR 0115) ----------------------------

SourceKey = Literal["W4.missed", "W3.shallow", "W3.deep", "W7.traps", "blunder"]


class FocusTargetItem(Snake):
    id: str
    baseline: float
    peer: float | None


class FocusTargetRequest(Snake):
    schema_version: Literal["lab-v1"] = SCHEMA_VERSION
    items: list[FocusTargetItem]


class Target(Snake):
    """goal.ts `Target`."""

    rate: float
    basis: Literal["peer", "relative"]


class FocusTargetResult(Snake):
    id: str
    target: Target | None


class FocusTargetResponse(Snake):
    schema_version: Literal["lab-v1"]
    algorithm_id: str
    focus_version: str
    items: list[FocusTargetResult]


class FocusProgressItem(Snake):
    id: str
    k: int = Field(ge=0)
    n: int = Field(ge=0)
    games: int = Field(ge=0)
    games_n: int = Field(ge=1)
    target: float = Field(ge=0, le=1)


class FocusProgressRequest(Snake):
    schema_version: Literal["lab-v1"] = SCHEMA_VERSION
    items: list[FocusProgressItem]


class Interval80(Snake):
    low: float
    high: float
    level: float


class ProgressGames(Snake):
    played: int
    of: int


class FocusVerdict(Camel):
    kind: Literal["on_track", "not_yet", "too_early"]
    more_chances: int | None = None

    @model_serializer
    def _wire(self) -> dict[str, str | int]:
        # goal.ts writes `moreChances` only on `too_early`.
        if self.more_chances is None:
            return {"kind": self.kind}
        return {"kind": self.kind, "moreChances": self.more_chances}


class Progress(Snake):
    """goal.ts `Progress`."""

    events: int
    chances: int
    rate: float | None
    interval: Interval80 | None
    games: ProgressGames
    verdict: FocusVerdict
    done: bool
    outcome: Literal["met", "not_met", "inconclusive"] | None


class FocusProgressResult(Snake):
    id: str
    progress: Progress


class FocusProgressResponse(Snake):
    schema_version: Literal["lab-v1"]
    algorithm_id: str
    focus_version: str
    items: list[FocusProgressResult]


class TrainerCandidate(Snake):
    """model.ts `Candidate`: a player's move at a weakness exemplar ply, or judged a blunder."""

    game_id: str
    ply: int = Field(ge=0)
    fen: str
    uci: str
    best_uci: str | None
    win_before: float
    win_after: float
    prev_san: str | None
    sources: list[SourceKey]


class TrainerSourcesRequest(Snake):
    schema_version: Literal["lab-v1"] = SCHEMA_VERSION
    candidates: list[TrainerCandidate]


class TrainerSourced(Snake):
    game_id: str
    ply: int
    kept: bool
    primary_source: SourceKey
    held_out: bool


class TrainerSourcesResponse(Snake):
    schema_version: Literal["lab-v1"]
    algorithm_id: str
    trainer_version: str
    sources: list[SourceKey]
    candidates: list[TrainerSourced]


#: Every message of the schema, by its `$defs` name.
MESSAGES: dict[str, type[BaseModel]] = {
    "AnalyseRequest": AnalyseRequest,
    "AnalyseResponse": AnalyseResponse,
    "ProfileRequest": ProfileRequest,
    "ProfileResponse": ProfileResponse,
    "RankRequest": RankRequest,
    "RankResponse": RankResponse,
    "TagMotifsRequest": TagMotifsRequest,
    "TagMotifsResponse": TagMotifsResponse,
    "PositionsRequest": PositionsRequest,
    "PositionsResponse": PositionsResponse,
    "EvaluateRequest": EvaluateRequest,
    "EvaluateResponse": EvaluateResponse,
    "JudgeRequest": JudgeRequest,
    "JudgeResponse": JudgeResponse,
    "ObserveRequest": ObserveRequest,
    "ObserveResponse": ObserveResponse,
    "TrajectoryRequest": TrajectoryRequest,
    "TrajectoryResponse": TrajectoryResponse,
    "TimelineRequest": TimelineRequest,
    "TimelineResponse": TimelineResponse,
    "FocusTargetRequest": FocusTargetRequest,
    "FocusTargetResponse": FocusTargetResponse,
    "FocusProgressRequest": FocusProgressRequest,
    "FocusProgressResponse": FocusProgressResponse,
    "TrainerSourcesRequest": TrainerSourcesRequest,
    "TrainerSourcesResponse": TrainerSourcesResponse,
}

#: Every object of the schema that a model mirrors field for field.
OBJECTS: dict[str, type[BaseModel]] = {
    **MESSAGES,
    "LabPoint": LabPoint,
    "LabOutcome": LabOutcome,
    "BandReferenceStat": BandReferenceStat,
    "PlayerFacts": PlayerFacts,
    "PlayerCheck": PlayerCheck,
    "LabGame": LabGame,
    "WeaknessEstimate": WeaknessEstimate,
    "PlayerProfile": PlayerProfile,
    "RankedWeakness": RankedWeakness,
    "PlayerRanking": PlayerRanking,
    "MotifLine": MotifLine,
    "LineAnalysis": LineAnalysis,
    "TaggedLine": TaggedLine,
    "DepthGame": DepthGame,
    "GamePositions": GamePositions,
    "Search": Search,
    "GivenSearch": GivenSearch,
    "JudgedMove": JudgedMove,
    "JudgedGame": JudgedGame,
    "ObserveGame": ObserveGame,
    "KeyCount": KeyCount,
    "ObservedSide": ObservedSide,
    "ObservedGame": ObservedGame,
    "RatedGame": RatedGame,
    "GamePoint": GamePoint,
    "LabPrior": LabPrior,
    "TrajectoryWeakness": TrajectoryWeakness,
    "TrajectoryPlayer": TrajectoryPlayer,
    "ColorMetric": ColorMetric,
    "ThenNowMetric": ThenNowMetric,
    "ThenNowWeakness": ThenNowWeakness,
    "TrajectoryWindow": TrajectoryWindow,
    "RatingAt": RatingAt,
    "MonthRating": MonthRating,
    "ThenVsNow": ThenVsNow,
    "Trajectory": Trajectory,
    "PlayerTrajectory": PlayerTrajectory,
    "TimelinePoint": TimelinePoint,
    "TimelinePlayer": TimelinePlayer,
    "Evidence": Evidence,
    "LabOnset": LabOnset,
    "LabTimelineRow": LabTimelineRow,
    "LabSeriesOutcome": LabSeriesOutcome,
    "PlayerTimeline": PlayerTimeline,
    "FocusTargetItem": FocusTargetItem,
    "Target": Target,
    "FocusTargetResult": FocusTargetResult,
    "FocusProgressItem": FocusProgressItem,
    "Interval80": Interval80,
    "ProgressGames": ProgressGames,
    "FocusVerdict": FocusVerdict,
    "Progress": Progress,
    "FocusProgressResult": FocusProgressResult,
    "TrainerCandidate": TrainerCandidate,
    "TrainerSourced": TrainerSourced,
}

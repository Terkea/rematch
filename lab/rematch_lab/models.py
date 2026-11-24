"""Typed messages of the lab bridge, `lab/schemas/lab-v1.schema.json` (ADR 0106).

The models mirror the schema's `$defs` field for field, and `tests/test_contract.py` checks that
they do: same names, same required fields, no extra field on either side. The envelope and the
lab's own objects are snake_case; the algorithm's own types (WeaknessEstimate, BandReferenceStat,
RankedWeakness) keep the camelCase the Rust code writes, through aliases.
"""

from __future__ import annotations

from datetime import datetime
from typing import Literal

from pydantic import BaseModel, ConfigDict, Field
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
}

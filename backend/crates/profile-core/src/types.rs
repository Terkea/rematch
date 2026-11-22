//! Shared profile types (final in R2 Task 1; later tasks add behaviour, not fields).
//!
//! Content source: TS M2 Task 1 `packages/chess-core/src/profile/types.ts` (plan text). Wire
//! strings (`as_str`, serde) are the TS strings; `contract_*` helpers give the M3 contract form.

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use chess_core::{Color, EngineScore, MoveRecord, TimeClass};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use time::OffsetDateTime;
use uuid::Uuid;

pub use coaching_input::enums::{AreaId as Area, BandId as Band, UnknownVariant, WeaknessState};

/// The seven areas, in contract order.
pub const AREAS: [Area; 7] = Area::ALL;

/// Serialises a type through its `Display` and deserialises through its `FromStr`, so it can be a
/// JSON object key (`BTreeMap<WeaknessKey, _>`) and matches the TS string form.
macro_rules! serde_via_str {
    ($name:ty) => {
        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.collect_str(self)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let s = <std::borrow::Cow<'de, str>>::deserialize(deserializer)?;
                s.parse().map_err(serde::de::Error::custom)
            }
        }
    };
}

/// Declares a fieldless enum with a wire string per variant, `ALL`, `as_str`, `Display`,
/// `FromStr` and string serde.
macro_rules! str_enum {
    ($(#[$meta:meta])* $name:ident, $kind:literal, { $($variant:ident => $wire:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum $name {
            $($variant),+
        }

        impl $name {
            /// Every variant, in declaration order (which is also `Ord`).
            pub const ALL: [$name; [$($wire),+].len()] = [$($name::$variant),+];

            /// The TS wire string.
            pub fn as_str(self) -> &'static str {
                match self {
                    $($name::$variant => $wire),+
                }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl FromStr for $name {
            type Err = UnknownVariant;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                match s {
                    $($wire => Ok($name::$variant),)+
                    _ => Err(UnknownVariant { kind: $kind, value: s.to_owned() }),
                }
            }
        }

        serde_via_str!($name);
    };
}

str_enum!(
    /// Tactical motif of a line. `as_str` is the TS camelCase tag, `contract_tag` the M3 snake_case.
    MotifTag, "motif tag", {
        HangingPiece => "hangingPiece",
        Fork => "fork",
        Pin => "pin",
        Skewer => "skewer",
        DiscoveredAttack => "discoveredAttack",
        Deflection => "deflection",
        Sacrifice => "sacrifice",
        Promotion => "promotion",
        Mate => "mate",
        MateIn1 => "mateIn1",
        MateIn2 => "mateIn2",
        MateIn3 => "mateIn3",
        MateIn4Plus => "mateIn4Plus",
        MatingNet => "matingNet",
        BackRankMate => "backRankMate",
        SmotheredMate => "smotheredMate",
    }
);

impl MotifTag {
    /// The contract (`ExemplarPosition.tags`) form: snake_case, digits as their own word.
    pub fn contract_tag(self) -> &'static str {
        match self {
            MotifTag::HangingPiece => "hanging_piece",
            MotifTag::Fork => "fork",
            MotifTag::Pin => "pin",
            MotifTag::Skewer => "skewer",
            MotifTag::DiscoveredAttack => "discovered_attack",
            MotifTag::Deflection => "deflection",
            MotifTag::Sacrifice => "sacrifice",
            MotifTag::Promotion => "promotion",
            MotifTag::Mate => "mate",
            MotifTag::MateIn1 => "mate_in_1",
            MotifTag::MateIn2 => "mate_in_2",
            MotifTag::MateIn3 => "mate_in_3",
            MotifTag::MateIn4Plus => "mate_in_4_plus",
            MotifTag::MatingNet => "mating_net",
            MotifTag::BackRankMate => "back_rank_mate",
            MotifTag::SmotheredMate => "smothered_mate",
        }
    }
}

str_enum!(
    /// Internal weakness key (improvement-engine section 6.2, TS M2 `WeaknessKey`).
    WeaknessKey, "weakness key", {
        W1 => "W1",
        W2 => "W2",
        W3Shallow => "W3.shallow",
        W3Deep => "W3.deep",
        W4Missed => "W4.missed",
        W4Allowed => "W4.allowed",
        W5 => "W5",
        W6 => "W6",
        W7Traps => "W7.traps",
        W7Prep => "W7.prep",
        W8Basics => "W8.basics",
        W8Advanced => "W8.advanced",
        W9a => "W9a",
        W9b => "W9b",
        W9c => "W9c",
        W9d => "W9d",
        W10 => "W10",
        W11 => "W11",
        W12 => "W12",
    }
);

impl WeaknessKey {
    /// Binomial (events / opportunities) keys: every key except the continuous W9d and W12.
    pub fn is_binomial(self) -> bool {
        !matches!(self, WeaknessKey::W9d | WeaknessKey::W12)
    }

    /// Contract `RankedWeakness.id` and `variant`: `W3.shallow` -> (`W3`, `shallow`), `W9a` ->
    /// (`W9a`, None).
    pub fn contract_id(self) -> (&'static str, Option<&'static str>) {
        match self.as_str().split_once('.') {
            Some((id, variant)) => (id, Some(variant)),
            None => (self.as_str(), None),
        }
    }
}

str_enum!(
    /// Endgame material signature (TS M2 `EndgameType`).
    EndgameType, "endgame type", {
        Pawn => "pawn",
        Rook => "rook",
        Minor => "minor",
        Queen => "queen",
        Mixed => "mixed",
    }
);

str_enum!(
    /// Platform a game was played on.
    PlatformId, "platform", {
        Lichess => "lichess",
        Chesscom => "chesscom",
    }
);

str_enum!(
    /// Kind of a strength exemplar ply (TS M2 `StrengthKind`).
    StrengthKind, "strength kind", {
        OnlyMoveFound => "only_move_found",
        TacticFound => "tactic_found",
        Conversion => "conversion",
        Defence => "defence",
    }
);

str_enum!(
    /// Outcome of an endgame episode (TS M2 `GameMetrics.endgame.outcome`).
    EndgameOutcome, "endgame outcome", {
        Converted => "converted",
        Held => "held",
        FailedConversion => "failed_conversion",
        FailedHold => "failed_hold",
    }
);

/// Returned when a [`SeriesKey`] or [`MetricId`] string does not parse.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("invalid {kind}: {value:?}")]
pub struct InvalidKey {
    pub kind: &'static str,
    pub value: String,
}

/// Key of a per-game series row (TS M2 `SeriesKey`). String forms: `W1`, `W3.shallow`,
/// `motif.missed:fork`, `motif.allowed:fork`, `opening:white:italian-game`, `endgame:rook`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SeriesKey {
    Weakness(WeaknessKey),
    MotifMissed(MotifTag),
    MotifAllowed(MotifTag),
    /// The opening `line_key` (`"{color}:{slug}"`, R-7).
    Opening(String),
    Endgame(EndgameType),
}

impl fmt::Display for SeriesKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SeriesKey::Weakness(k) => f.write_str(k.as_str()),
            SeriesKey::MotifMissed(t) => write!(f, "motif.missed:{t}"),
            SeriesKey::MotifAllowed(t) => write!(f, "motif.allowed:{t}"),
            SeriesKey::Opening(line) => write!(f, "opening:{line}"),
            SeriesKey::Endgame(e) => write!(f, "endgame:{e}"),
        }
    }
}

impl FromStr for SeriesKey {
    type Err = InvalidKey;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let invalid = || InvalidKey {
            kind: "series key",
            value: s.to_owned(),
        };
        if let Some(t) = s.strip_prefix("motif.missed:") {
            return t.parse().map(SeriesKey::MotifMissed).map_err(|_| invalid());
        }
        if let Some(t) = s.strip_prefix("motif.allowed:") {
            return t
                .parse()
                .map(SeriesKey::MotifAllowed)
                .map_err(|_| invalid());
        }
        if let Some(line) = s.strip_prefix("opening:") {
            if line.is_empty() {
                return Err(invalid());
            }
            return Ok(SeriesKey::Opening(line.to_owned()));
        }
        if let Some(e) = s.strip_prefix("endgame:") {
            return e.parse().map(SeriesKey::Endgame).map_err(|_| invalid());
        }
        s.parse().map(SeriesKey::Weakness).map_err(|_| invalid())
    }
}

serde_via_str!(SeriesKey);

/// Tablebase result from one side's point of view: -1 loss, 0 draw, 1 win.
pub type Wdl = i8;

/// Material swing realised by a line.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Swing {
    /// Material gained, in pawns.
    pub gain: f64,
    /// Plies until the swing is realised.
    pub depth: Option<u32>,
    pub mate_in: Option<u32>,
}

/// A swing plus the motifs of the line. JSON is flat like TS M2 (`LineAnalysis extends Swing`):
/// `{"gain", "depth", "mateIn", "tags"}`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LineAnalysis {
    #[serde(flatten)]
    pub swing: Swing,
    pub tags: Vec<MotifTag>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MultipvLine {
    pub uci: String,
    /// Mover's point of view.
    pub score: EngineScore,
    pub pv: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreatProbe {
    pub null_fen: String,
    pub pv: Vec<String>,
    /// Opponent's point of view.
    pub score: EngineScore,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Difficulty {
    pub p_best: f64,
    pub p_ok: f64,
    pub model: String,
}

/// Engine-side enrichment of one ply, as stored.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawMoveFeatures {
    pub ply: u32,
    pub multipv: Option<Vec<MultipvLine>>,
    pub best_pv: Vec<String>,
    pub refutation_pv: Vec<String>,
    pub threat: Option<ThreatProbe>,
    /// Mover's point of view.
    pub tb_wdl_before: Option<Wdl>,
    /// Mover's point of view.
    pub tb_wdl_after: Option<Wdl>,
    pub difficulty: Option<Difficulty>,
}

/// Features derived from [`RawMoveFeatures`] (Task 6). `None` = not computable.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveFeatures {
    pub is_critical: Option<bool>,
    pub multipv_gap: Option<f64>,
    pub playable: Option<u32>,
    pub in_top2: Option<bool>,
    pub best: Option<LineAnalysis>,
    pub refutation: Option<LineAnalysis>,
    pub threat: Option<LineAnalysis>,
    pub threat_existed: Option<bool>,
    pub threat_addressed: Option<bool>,
}

/// A move record with evaluations and features.
#[derive(Clone, Debug, PartialEq)]
pub struct EnrichedMove {
    pub rec: MoveRecord,
    pub fen_after: String,
    /// Mover's point of view.
    pub score_before: EngineScore,
    /// Mover's point of view.
    pub score_after: EngineScore,
    pub raw: Option<RawMoveFeatures>,
    pub f: MoveFeatures,
}

/// Per-game context from the player's point of view.
#[derive(Clone, Debug, PartialEq)]
pub struct GameContext {
    pub game_id: Uuid,
    pub platform_game_id: String,
    pub platform: PlatformId,
    pub played_at: OffsetDateTime,
    pub time_class: TimeClass,
    pub base_seconds: Option<u32>,
    pub increment_seconds: Option<u32>,
    pub rated: bool,
    pub player_color: Color,
    pub player_rating: Option<i32>,
    pub opponent_rating: Option<i32>,
    /// `"1-0"`, `"0-1"`, `"1/2-1/2"` or `"*"`.
    pub result: String,
    /// Player's point of view: 1, 0.5 or 0.
    pub score: Option<f64>,
    pub termination: Option<String>,
    pub accuracy: Option<f64>,
    pub protocol: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AnalysedGame {
    pub ctx: GameContext,
    /// Both colours, in ply order.
    pub moves: Vec<EnrichedMove>,
    pub book_exit_ply: Option<u32>,
    pub opening_line_key: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct BinomialObs {
    pub events: u32,
    pub opportunities: u32,
    pub epl: f64,
    pub exemplar_plies: Vec<u32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeriesObs {
    pub series_key: SeriesKey,
    pub events: u32,
    pub opportunities: u32,
    pub epl: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameObservation {
    pub game_id: Uuid,
    /// Binomial keys only ([`WeaknessKey::is_binomial`]); absent = no opportunity.
    pub binomial: BTreeMap<WeaknessKey, BinomialObs>,
    pub w9d_rho: Option<f64>,
    pub series: Vec<SeriesObs>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StrengthPly {
    pub kind: StrengthKind,
    pub ply: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub game_id: Uuid,
    pub session_id: u32,
    /// 1-based.
    pub index_in_session: u32,
    pub after_loss: bool,
    pub residual: Option<f64>,
}

/// Version of [`PlayerGameMetrics`] written by this code.
///
/// Version 2 (R2 Task 9 fix round): `tactics.missed` became `Option` (`None` = W3 unknown for
/// the game). Version-1 rows read it as `Some(n)` even when W3 was dropped, so they are stale.
///
/// Version 3 (analysis audit, ADR-FIX-C-1): `accuracy_by_phase` is lila's game accuracy over the
/// phase's moves (was a plain mean of move accuracies), `accuracy` follows lila's harmonic floor
/// and move-count window, and `player_rating` / `opponent_rating` / `expected_score` are
/// pre-game. Version-2 rows are stale; `mark_stale_metrics` queues their recompute.
pub const PLAYER_GAME_METRICS_VERSION: u32 = 3;

/// A value per game phase (TS `Record<Phase, number | null>`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PhaseValues {
    pub opening: Option<f64>,
    pub middlegame: Option<f64>,
    pub endgame: Option<f64>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ClockBucket {
    pub moves: u32,
    pub decisive: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ConversionMetric {
    pub episode: bool,
    pub converted: Option<bool>,
    pub drop_ply: Option<u32>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ResilienceMetric {
    pub episode: bool,
    pub saved: Option<bool>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MissCounts {
    pub opportunities: u32,
    pub misses: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TacticsCounts {
    /// W3 opportunities (0 when W3 is unknown for the game; see `missed`).
    pub opportunities: u32,
    /// W3 events; `None` when W3 was left out for the game (a missing best line, ADR-R2-4 §10)
    /// or the observation was unusable. Never read `None` as 0.
    pub missed: Option<u32>,
    pub allowed: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SharpCounts {
    pub positions: u32,
    pub errors: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct QuietLoss {
    pub moves: u32,
    /// Sum of player win% loss over the quiet moves.
    pub loss: f64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct OpeningMetrics {
    pub line_key: Option<String>,
    pub book_exit_ply: Option<u32>,
    /// Player-POV win% after the last opening ply.
    pub win_at_opening_end: Option<f64>,
    pub out_worse: Option<bool>,
    /// White-POV `score_to_cp` (mate = +-1000) after the last opening ply (R-8).
    pub eval_at_opening_end_white_cp: Option<i32>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct EndgameMetrics {
    #[serde(rename = "type")]
    pub kind: Option<EndgameType>,
    pub entry_win: Option<f64>,
    pub outcome: Option<EndgameOutcome>,
    pub tb_positions: u32,
    pub tb_errors: u32,
}

/// Per-game chess metrics, version 1, stored as `game_metrics.metrics` jsonb (serde camelCase).
/// Field list exactly TS M2 Task 1 `GameMetrics`, plus `opening.evalAtOpeningEndWhiteCp` (R-8).
/// Named `PlayerGameMetrics` to avoid confusion with R1's `analysis::metrics::GameMetrics` (timing).
///
/// Struct-level `#[serde(default)]`: a field added later deserialises from older jsonb rows as its
/// default. `version` still gates semantic changes (a changed meaning bumps it, and readers
/// recompute or reject rows with an older version).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PlayerGameMetrics {
    pub version: u32,
    pub score: Option<f64>,
    pub expected_score: Option<f64>,
    pub player_rating: Option<i32>,
    pub opponent_rating: Option<i32>,
    pub accuracy: Option<f64>,
    pub accuracy_by_phase: PhaseValues,
    pub moves: u32,
    pub blunders: u32,
    pub mistakes: u32,
    pub inaccuracies: u32,
    pub one_move_blunders: u32,
    pub low_clock: ClockBucket,
    pub high_clock: ClockBucket,
    pub conversion: ConversionMetric,
    pub resilience: ResilienceMetric,
    pub missed_punish: MissCounts,
    pub tactics: TacticsCounts,
    pub missed_by_motif: BTreeMap<MotifTag, MissCounts>,
    pub sharp: SharpCounts,
    pub quiet: QuietLoss,
    /// TS name `evalTrend15to35`: player-POV W after ply 70 minus W after ply 30.
    #[serde(rename = "evalTrend15to35")]
    pub eval_trend_15_to_35: Option<f64>,
    /// `None` for games without clocks.
    pub time_trouble: Option<bool>,
    pub lost_on_time: bool,
    pub time_per_move: PhaseValues,
    pub critical_time_ratio: Option<f64>,
    pub time_in_opening: Option<f64>,
    pub opening: OpeningMetrics,
    pub endgame: EndgameMetrics,
    pub resigned_saveable: Option<bool>,
    /// Binomial keys only.
    pub weakness: BTreeMap<WeaknessKey, BinomialObs>,
    pub w9d_rho: Option<f64>,
    pub strengths: Vec<StrengthPly>,
}

str_enum!(
    /// The 36 base rows of the KPI catalogue (TS M2 Task 19, R2 Task 15).
    BaseMetric, "base metric", {
        Rating => "rating",
        PerfVsExpected => "perf_vs_expected",
        Accuracy => "accuracy",
        AccuracyOpening => "accuracy.opening",
        AccuracyMiddlegame => "accuracy.middlegame",
        AccuracyEndgame => "accuracy.endgame",
        BlundersPerGame => "blunders_per_game",
        OneMoveBlundersPerGame => "one_move_blunders_per_game",
        BlunderRateLowClock => "blunder_rate_low_clock",
        ConversionRate => "conversion_rate",
        ResilienceRate => "resilience_rate",
        MissedPunishRate => "missed_punish_rate",
        TacticsMissedPerGame => "tactics_missed_per_game",
        TacticsAllowedPerGame => "tactics_allowed_per_game",
        SharpErrorRate => "sharp_error_rate",
        QuietLossPerMove => "quiet_loss_per_move",
        EvalTrend1535 => "eval_trend_15_35",
        TimeTroubleRate => "time_trouble_rate",
        LossesOnTimeRate => "losses_on_time_rate",
        TimePerMoveOpening => "time_per_move.opening",
        TimePerMoveMiddlegame => "time_per_move.middlegame",
        TimePerMoveEndgame => "time_per_move.endgame",
        CriticalTimeRatio => "critical_time_ratio",
        EvalAfterOpening => "eval_after_opening",
        OpeningOutWorseRate => "opening_out_worse_rate",
        BookExitPly => "book_exit_ply",
        TimeInOpening => "time_in_opening",
        EndgameConversionRate => "endgame_conversion_rate",
        EndgameHoldRate => "endgame_hold_rate",
        TbErrorsPerEndgame => "tb_errors_per_endgame",
        ScoreAfterLoss => "score_after_loss",
        ScoreAfterNonLoss => "score_after_non_loss",
        SessionAccuracyDecline => "session_accuracy_decline",
        FirstGameAccuracy => "first_game_accuracy",
        LaterGamesAccuracy => "later_games_accuracy",
        SaveableResignationRate => "saveable_resignation_rate",
    }
);

/// A KPI or band-reference metric id. String forms: a [`BaseMetric`] (`accuracy.opening`),
/// `motif_miss_rate:<MotifTag::as_str>`, `weakness:<WeaknessKey::as_str>`, `area:<area>`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MetricId {
    Base(BaseMetric),
    MotifMissRate(MotifTag),
    Weakness(WeaknessKey),
    Area(Area),
}

impl fmt::Display for MetricId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MetricId::Base(m) => f.write_str(m.as_str()),
            MetricId::MotifMissRate(t) => write!(f, "motif_miss_rate:{t}"),
            MetricId::Weakness(k) => write!(f, "weakness:{k}"),
            MetricId::Area(a) => write!(f, "area:{a}"),
        }
    }
}

impl FromStr for MetricId {
    type Err = InvalidKey;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let invalid = || InvalidKey {
            kind: "metric id",
            value: s.to_owned(),
        };
        if let Some(t) = s.strip_prefix("motif_miss_rate:") {
            return t
                .parse()
                .map(MetricId::MotifMissRate)
                .map_err(|_| invalid());
        }
        if let Some(k) = s.strip_prefix("weakness:") {
            return k.parse().map(MetricId::Weakness).map_err(|_| invalid());
        }
        if let Some(a) = s.strip_prefix("area:") {
            return a.parse().map(MetricId::Area).map_err(|_| invalid());
        }
        s.parse().map(MetricId::Base).map_err(|_| invalid())
    }
}

serde_via_str!(MetricId);

/// Band reference statistics of one metric.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BandReferenceStat {
    pub metric: MetricId,
    pub median: f64,
    pub p25: f64,
    pub p40: f64,
    pub p75: f64,
    pub pooled_rate: Option<f64>,
    pub epl_per_event: Option<f64>,
    pub opps_per_game: Option<f64>,
    /// OLS slope of the per-game value on `(opponent - own) / 100`; `None` when it could not be
    /// estimated (no adjustment then). Never NaN.
    pub opp_slope_per_100: Option<f64>,
    pub n_players: u32,
    pub n_games: u32,
    pub low_sample: bool,
}

pub type BandReference = BTreeMap<MetricId, BandReferenceStat>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_string_form_round_trips() {
        for t in MotifTag::ALL {
            assert_eq!(t.as_str().parse::<MotifTag>().unwrap(), t);
            for key in [SeriesKey::MotifMissed(t), SeriesKey::MotifAllowed(t)] {
                assert_eq!(key.to_string().parse::<SeriesKey>().unwrap(), key);
            }
            let m = MetricId::MotifMissRate(t);
            assert_eq!(m.to_string().parse::<MetricId>().unwrap(), m);
        }
        for k in WeaknessKey::ALL {
            let s = SeriesKey::Weakness(k);
            assert_eq!(s.to_string().parse::<SeriesKey>().unwrap(), s);
            let m = MetricId::Weakness(k);
            assert_eq!(m.to_string().parse::<MetricId>().unwrap(), m);
        }
        for e in EndgameType::ALL {
            let s = SeriesKey::Endgame(e);
            assert_eq!(s.to_string(), format!("endgame:{}", e.as_str()));
            assert_eq!(s.to_string().parse::<SeriesKey>().unwrap(), s);
        }
        for b in BaseMetric::ALL {
            let m = MetricId::Base(b);
            assert_eq!(m.to_string().parse::<MetricId>().unwrap(), m);
        }
        for a in AREAS {
            let m = MetricId::Area(a);
            assert_eq!(m.to_string(), format!("area:{}", a.as_str()));
            assert_eq!(m.to_string().parse::<MetricId>().unwrap(), m);
        }
        let o: SeriesKey = "opening:white:italian-game".parse().unwrap();
        assert_eq!(o, SeriesKey::Opening("white:italian-game".into()));
        assert_eq!(o.to_string(), "opening:white:italian-game");
        assert!("opening:".parse::<SeriesKey>().is_err());
        assert!("W13".parse::<SeriesKey>().is_err());
        assert!("motif.missed:nope".parse::<SeriesKey>().is_err());
        assert_eq!(BaseMetric::ALL.len(), 36);
        assert_eq!(MotifTag::ALL.len(), 16);
        assert_eq!(WeaknessKey::ALL.len(), 19);
    }

    #[test]
    fn metric_ids_are_unique_and_serde_matches_display() {
        let mut seen = std::collections::BTreeSet::new();
        for b in BaseMetric::ALL {
            assert!(seen.insert(b.as_str()));
        }
        let m = MetricId::Weakness(WeaknessKey::W3Shallow);
        assert_eq!(serde_json::to_value(m).unwrap(), "weakness:W3.shallow");
    }

    #[test]
    fn player_game_metrics_json_is_camel_case() {
        let mut m = PlayerGameMetrics {
            version: PLAYER_GAME_METRICS_VERSION,
            ..Default::default()
        };
        m.weakness
            .insert(WeaknessKey::W3Shallow, BinomialObs::default());
        m.missed_by_motif
            .insert(MotifTag::BackRankMate, MissCounts::default());
        m.endgame.kind = Some(EndgameType::Rook);
        m.strengths.push(StrengthPly {
            kind: StrengthKind::OnlyMoveFound,
            ply: 12,
        });
        let v = serde_json::to_value(&m).unwrap();
        assert_eq!(v["version"], 3);
        assert!(v.get("accuracyByPhase").is_some());
        assert!(v.get("evalTrend15to35").is_some());
        assert!(v["opening"].get("evalAtOpeningEndWhiteCp").is_some());
        assert_eq!(v["endgame"]["type"], "rook");
        assert!(v["weakness"].get("W3.shallow").is_some());
        assert!(v["missedByMotif"].get("backRankMate").is_some());
        assert_eq!(v["strengths"][0]["kind"], "only_move_found");
        let back: PlayerGameMetrics = serde_json::from_value(v).unwrap();
        assert_eq!(back, m);
    }

    #[test]
    fn line_analysis_json_is_flat_like_ts() {
        let l = LineAnalysis {
            swing: Swing {
                gain: 3.0,
                depth: Some(2),
                mate_in: None,
            },
            tags: vec![MotifTag::Fork, MotifTag::MateIn4Plus],
        };
        let json = r#"{"gain":3.0,"depth":2,"mateIn":null,"tags":["fork","mateIn4Plus"]}"#;
        assert_eq!(serde_json::to_string(&l).unwrap(), json);
        assert_eq!(serde_json::from_str::<LineAnalysis>(json).unwrap(), l);
    }

    #[test]
    fn old_player_game_metrics_json_without_newer_fields_deserialises() {
        let mut v = serde_json::to_value(PlayerGameMetrics {
            version: PLAYER_GAME_METRICS_VERSION,
            blunders: 2,
            ..Default::default()
        })
        .unwrap();
        // Simulate a row written before these fields existed.
        let obj = v.as_object_mut().unwrap();
        obj.remove("strengths");
        obj.remove("w9dRho");
        obj["opening"]
            .as_object_mut()
            .unwrap()
            .remove("evalAtOpeningEndWhiteCp");
        let m: PlayerGameMetrics = serde_json::from_value(v).unwrap();
        assert_eq!(m.version, PLAYER_GAME_METRICS_VERSION);
        assert_eq!(m.blunders, 2);
        assert!(m.strengths.is_empty());
        assert_eq!(m.w9d_rho, None);
        assert_eq!(m.opening.eval_at_opening_end_white_cp, None);
    }
}

/// Which of the player's games a profile row covers (the colour split, `side_scope` in SQL):
/// every game, or only those the player had White or Black in. Per-side rows obey the same
/// minimum-evidence rules as `Both`, counted over that side's games. A `Both` benchmark is never
/// applied to one colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Side {
    Both,
    White,
    Black,
}

impl Side {
    /// In storage order: `Both` first.
    pub const ALL: [Side; 3] = [Side::Both, Side::White, Side::Black];

    /// The `side_scope` enum text.
    pub fn as_str(self) -> &'static str {
        match self {
            Side::Both => "both",
            Side::White => "white",
            Side::Black => "black",
        }
    }

    /// The side of one colour.
    pub fn of(color: Color) -> Side {
        match color {
            Color::White => Side::White,
            Color::Black => Side::Black,
        }
    }

    /// True when a game the player had `color` in belongs to this side.
    pub fn covers(self, color: Color) -> bool {
        self == Side::Both || self == Side::of(color)
    }
}

impl fmt::Display for Side {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Side {
    type Err = UnknownVariant;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Side::ALL
            .into_iter()
            .find(|x| x.as_str() == s)
            .ok_or_else(|| UnknownVariant {
                kind: "side",
                value: s.to_owned(),
            })
    }
}

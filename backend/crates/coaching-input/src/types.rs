//! Contract types: the M3 zod `CoachingInput` (`packages/ai/src/input/schema.ts`), field for
//! field (ADR-R2-2).
//!
//! Every object is strict (`deny_unknown_fields`), every key is present on the wire, and a
//! zod `.nullable()` field is an `Option` that serialises as `null`. Nullable fields deserialise
//! through [`nullable`], so a missing key is an error as in zod (serde would otherwise read it as
//! `None`). The schemars attributes carry zod's bounds; the refinements JSON Schema cannot express
//! live in `validate.rs`.

use std::borrow::Cow;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize};
use time::{Date, OffsetDateTime};
use uuid::Uuid;

use crate::enums::{AreaId, BandId, WeaknessState};
use crate::limits::{
    ECO_MAX_CHARS, FEN_MAX_CHARS, INPUT_VERSION, LIMITS, MAX_BASE_SECONDS, MAX_COUNT, MAX_CP,
    MAX_EXCESS_EPL, MAX_INCREMENT_SECONDS, MAX_MATE, MAX_PLY, MAX_PRIORITY, MAX_RATING,
    UCI_PATTERN, WEAKNESS_ID_MAX_CHARS, WEAKNESS_ID_PATTERN,
};

/// Deserialises a zod `.nullable()` field: `null` or a value, but the key must be present.
/// (`#[serde(deserialize_with)]` turns off serde's missing-`Option`-is-`None` rule.)
fn nullable<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}

/// `YYYY-MM-DD` (zod `z.iso.date()`).
mod iso_date {
    use serde::{Deserialize, Deserializer, Serializer, de, ser};
    use time::Date;
    use time::format_description::FormatItem;
    use time::macros::format_description;

    const FORMAT: &[FormatItem<'static>] = format_description!("[year]-[month]-[day]");

    pub fn serialize<S: Serializer>(date: &Date, serializer: S) -> Result<S::Ok, S::Error> {
        let text = date.format(FORMAT).map_err(ser::Error::custom)?;
        serializer.serialize_str(&text)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Date, D::Error> {
        let text = String::deserialize(deserializer)?;
        Date::parse(&text, FORMAT).map_err(de::Error::custom)
    }
}

/// `inputVersion`: the literal `"coaching-input-v1"` ([`INPUT_VERSION`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum InputVersion {
    #[default]
    #[serde(rename = "coaching-input-v1")]
    V1,
}

impl InputVersion {
    pub fn as_str(self) -> &'static str {
        INPUT_VERSION
    }
}

impl JsonSchema for InputVersion {
    fn inline_schema() -> bool {
        true
    }

    fn schema_name() -> Cow<'static, str> {
        "InputVersion".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({ "type": "string", "const": INPUT_VERSION })
    }
}

/// `CoachingInput.platform`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Lichess,
    Chesscom,
}

/// `CoachingInput.timeClass`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum TimeClass {
    Bullet,
    Blitz,
    Rapid,
    Classical,
    Daily,
}

impl From<chess_core::TimeClass> for TimeClass {
    fn from(tc: chess_core::TimeClass) -> Self {
        match tc {
            chess_core::TimeClass::Bullet => TimeClass::Bullet,
            chess_core::TimeClass::Blitz => TimeClass::Blitz,
            chess_core::TimeClass::Rapid => TimeClass::Rapid,
            chess_core::TimeClass::Classical => TimeClass::Classical,
            chess_core::TimeClass::Daily => TimeClass::Daily,
        }
    }
}

/// Typical time control in seconds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TimeControl {
    #[schemars(range(max = MAX_BASE_SECONDS))]
    pub base_seconds: u32,
    #[schemars(range(max = MAX_INCREMENT_SECONDS))]
    pub increment_seconds: u32,
}

/// An engine evaluation from WHITE's point of view: `{"cp": n}` (centipawns, positive = White
/// better) or `{"mate": n}` (moves to mate, positive = White mates).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Score {
    Cp(#[schemars(range(min = -MAX_CP, max = MAX_CP))] i32),
    Mate(#[schemars(range(min = -MAX_MATE, max = MAX_MATE))] i32),
}

/// A move as SAN (display) and UCI (machine, e.g. `e2e4` or `e7e8q`).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MoveRef {
    #[schemars(length(min = 1, max = LIMITS.san_chars))]
    pub san: String,
    #[schemars(regex(pattern = UCI_PATTERN))]
    pub uci: String,
}

/// A concrete position from one of the player's games, cited as evidence.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExemplarPosition {
    /// Platform game id (R-7).
    #[schemars(length(min = 1, max = LIMITS.id_chars))]
    pub game_id: String,
    /// 1-based half-move number of the position's side to move: odd = White, even = Black.
    #[schemars(range(min = 1, max = MAX_PLY))]
    pub ply: u32,
    #[serde(with = "common::js_time::iso")]
    #[schemars(schema_with = "crate::schema::iso_datetime")]
    pub played_at: OffsetDateTime,
    /// Position BEFORE `played`, standard FEN.
    #[schemars(length(max = FEN_MAX_CHARS))]
    pub fen: String,
    pub played: MoveRef,
    /// The engine's preferred move, or null when `played` was already best.
    #[serde(deserialize_with = "nullable")]
    pub best: Option<MoveRef>,
    /// Engine line starting with the best move from `fen` (UCI).
    #[schemars(length(max = LIMITS.pv_plies), inner(regex(pattern = UCI_PATTERN)))]
    pub best_pv: Vec<String>,
    /// Engine line starting from the position AFTER `played` (UCI).
    #[schemars(length(max = LIMITS.pv_plies), inner(regex(pattern = UCI_PATTERN)))]
    pub refutation_pv: Vec<String>,
    /// Short classifier tags such as `hanging_piece`.
    #[schemars(length(max = LIMITS.tags), inner(length(min = 1, max = LIMITS.tag_chars)))]
    pub tags: Vec<String>,
    pub eval_before: Score,
    pub eval_after: Score,
    /// The mover's win percentage before the move, 0..100.
    #[schemars(range(min = 0, max = 100))]
    pub win_before: f64,
    /// The mover's win percentage after the move, 0..100.
    #[schemars(range(min = 0, max = 100))]
    pub win_after: f64,
}

/// `TimelineChange.examples[]`: an exemplar plus which side of the change point it is from.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChangeExample {
    #[serde(flatten)]
    pub position: ExemplarPosition,
    pub side: ExampleSide,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ExampleSide {
    Before,
    After,
}

/// What a metric's `value`, `low`, `high` and `bandMedian` measure: `rate` (events per
/// opportunity, 0..1), `per_game` (a count per game, 0..1000), `pct` (0..100), `seconds`
/// (0..100 000) or `elo` (a rating-point quantity, 0..4000).
// No variant doc comments: they would turn the schema's `enum` into a `oneOf` of consts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MetricUnit {
    Rate,
    PerGame,
    Pct,
    Seconds,
    Elo,
}

impl MetricUnit {
    pub const ALL: [MetricUnit; 5] = [
        MetricUnit::Rate,
        MetricUnit::PerGame,
        MetricUnit::Pct,
        MetricUnit::Seconds,
        MetricUnit::Elo,
    ];

    /// zod `UNIT_MAX`: the upper end of the unit's range (the lower end is 0).
    pub fn max(self) -> f64 {
        match self {
            MetricUnit::Rate => 1.0,
            MetricUnit::PerGame => 1000.0,
            MetricUnit::Pct => 100.0,
            MetricUnit::Seconds => 100_000.0,
            MetricUnit::Elo => 4000.0,
        }
    }

    /// The wire string.
    pub fn as_str(self) -> &'static str {
        match self {
            MetricUnit::Rate => "rate",
            MetricUnit::PerGame => "per_game",
            MetricUnit::Pct => "pct",
            MetricUnit::Seconds => "seconds",
            MetricUnit::Elo => "elo",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Metric {
    /// Stable metric key.
    #[schemars(length(min = 1, max = LIMITS.id_chars))]
    pub key: String,
    pub area: AreaId,
    #[schemars(length(min = 1, max = LIMITS.label_chars))]
    pub label: String,
    /// Point estimate in `unit` (range checked by `validate`).
    pub value: f64,
    pub unit: MetricUnit,
    /// Number of observations behind the estimate.
    #[schemars(range(max = MAX_COUNT))]
    pub n: u32,
    /// Lower end of the credible interval, in `unit`.
    #[serde(deserialize_with = "nullable")]
    pub low: Option<f64>,
    /// Upper end of the credible interval, in `unit`.
    #[serde(deserialize_with = "nullable")]
    pub high: Option<f64>,
    /// Median of the player's rating band for this metric, in `unit`.
    #[serde(deserialize_with = "nullable")]
    pub band_median: Option<f64>,
    /// Percentile within the band, 0..100.
    #[serde(deserialize_with = "nullable")]
    #[schemars(range(min = 0, max = 100))]
    pub band_percentile: Option<f64>,
    /// True when `n` meets the minimum sample for this metric.
    pub sufficient: bool,
    /// More observations needed to become sufficient; null when sufficient.
    #[serde(deserialize_with = "nullable")]
    #[schemars(range(max = MAX_COUNT))]
    pub need_more: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RankedWeakness {
    /// Weakness catalogue id `W1`..`W12`, optionally with a variant letter (`W9b`).
    #[schemars(length(max = WEAKNESS_ID_MAX_CHARS), regex(pattern = WEAKNESS_ID_PATTERN))]
    pub id: String,
    /// Variant name within the weakness (e.g. `fork`).
    #[serde(deserialize_with = "nullable")]
    #[schemars(length(min = 1, max = LIMITS.tag_chars))]
    pub variant: Option<String>,
    #[schemars(length(min = 1, max = LIMITS.label_chars))]
    pub label: String,
    pub area: AreaId,
    /// 1 = highest priority; unique across `weaknesses`.
    #[schemars(range(min = 1, max = LIMITS.weaknesses))]
    pub rank: u32,
    /// Posterior event rate, events per opportunity.
    #[schemars(range(min = 0, max = 1))]
    pub posterior_rate: f64,
    #[schemars(range(min = 0, max = 1))]
    pub interval_low: f64,
    #[schemars(range(min = 0, max = 1))]
    pub interval_high: f64,
    /// Median event rate of the player's band.
    #[schemars(range(min = 0, max = 1))]
    pub band_median: f64,
    #[schemars(range(min = 0, max = 100))]
    pub band_percentile: f64,
    /// Expected points lost per game beyond the band median.
    #[schemars(range(min = 0, max = MAX_EXCESS_EPL))]
    pub excess_epl_per_game: f64,
    #[schemars(range(min = 0, max = 1))]
    pub confidence: f64,
    #[schemars(range(min = 0, max = MAX_PRIORITY))]
    pub priority: f64,
    pub state: WeaknessState,
    /// Probability the player is improving.
    #[serde(deserialize_with = "nullable")]
    #[schemars(range(min = 0, max = 1))]
    pub p_imp: Option<f64>,
    #[schemars(range(max = MAX_COUNT))]
    pub events: u32,
    #[schemars(range(max = MAX_COUNT))]
    pub opportunities: u32,
    #[schemars(length(max = LIMITS.exemplars_per_item))]
    pub exemplars: Vec<ExemplarPosition>,
}

/// A strength cited with exemplar positions (R-1: exactly these four fields).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Strength {
    pub area: AreaId,
    #[schemars(length(min = 1, max = LIMITS.label_chars))]
    pub label: String,
    /// Key of the supporting `Metric`, when there is one.
    #[serde(deserialize_with = "nullable")]
    #[schemars(length(min = 1, max = LIMITS.id_chars))]
    pub metric_key: Option<String>,
    #[schemars(length(max = LIMITS.exemplars_per_item))]
    pub exemplars: Vec<ExemplarPosition>,
}

/// The player's colour in an opening line's games.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum PlayerColor {
    White,
    Black,
}

/// `chosen`: the player plays the line; `faced`: the opponent plays it against the player.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum LineSide {
    Chosen,
    Faced,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Verdict {
    Keep,
    Fix,
    Learn,
    Replace,
}

/// `OpeningVerdict.alternatives[]`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Alternative {
    #[schemars(length(min = 1, max = LIMITS.label_chars))]
    pub name: String,
    #[schemars(length(max = LIMITS.moves_san), inner(length(min = 1, max = LIMITS.san_chars)))]
    pub moves_san: Vec<String>,
    /// Band-average score with this alternative, 0..1.
    #[schemars(range(min = 0, max = 1))]
    pub band_score: f64,
    #[schemars(range(max = MAX_COUNT))]
    pub games: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OpeningVerdict {
    /// Stable key for the line (R-7).
    #[schemars(length(min = 1, max = LIMITS.id_chars))]
    pub line_key: String,
    #[schemars(length(min = 1, max = LIMITS.label_chars))]
    pub name: String,
    /// ECO code such as `B90`.
    #[serde(deserialize_with = "nullable")]
    #[schemars(length(min = 1, max = ECO_MAX_CHARS))]
    pub eco: Option<String>,
    pub color: PlayerColor,
    pub side: LineSide,
    /// Moves of the line in SAN from the starting position.
    #[schemars(length(max = LIMITS.moves_san), inner(length(min = 1, max = LIMITS.san_chars)))]
    pub moves_san: Vec<String>,
    #[schemars(range(max = MAX_COUNT))]
    pub games: u32,
    /// Average result for the player, 0..1.
    #[schemars(range(min = 0, max = 1))]
    pub score: f64,
    /// `score` minus the expected score at this rating, -1..1.
    #[serde(deserialize_with = "nullable")]
    #[schemars(range(min = -1, max = 1))]
    pub performance_vs_expected: Option<f64>,
    /// Evaluation at the end of the opening phase, White POV (R-8).
    #[serde(deserialize_with = "nullable")]
    pub avg_eval_at_opening_end: Option<Score>,
    pub verdict: Verdict,
    /// Ply where the player left theory.
    #[serde(deserialize_with = "nullable")]
    #[schemars(range(min = 1, max = MAX_PLY))]
    pub deviation_ply: Option<u32>,
    /// The move played at `deviationPly`, SAN.
    #[serde(deserialize_with = "nullable")]
    #[schemars(length(min = 1, max = LIMITS.san_chars))]
    pub deviation_move_san: Option<String>,
    #[schemars(length(max = LIMITS.alternatives))]
    pub alternatives: Vec<Alternative>,
    #[schemars(length(max = LIMITS.examples_per_opening))]
    pub examples: Vec<ExemplarPosition>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AreaScore {
    pub area: AreaId,
    /// 0..100 (higher = stronger); null when there is not enough data.
    #[serde(deserialize_with = "nullable")]
    #[schemars(range(min = 0, max = 100))]
    pub score: Option<f64>,
    /// Observations behind the score.
    #[schemars(range(max = MAX_COUNT))]
    pub n: u32,
    pub sufficient: bool,
    /// More observations needed to reach sufficiency; null when sufficient.
    #[serde(deserialize_with = "nullable")]
    #[schemars(range(max = MAX_COUNT))]
    pub need_more: Option<u32>,
}

/// `CandidateCause.kind` (M3's names).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CauseKind {
    TimeControlMix,
    NewOpening,
    SessionLength,
    RatingLevel,
    ProtocolVersion,
}

/// `CandidateCause.before`/`after`: a short text or a number.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum CauseValue {
    Text(#[schemars(length(max = LIMITS.label_chars))] String),
    Number(f64),
}

/// A correlated possible cause of a change, never proven.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CandidateCause {
    pub kind: CauseKind,
    #[schemars(length(min = 1, max = LIMITS.description_chars))]
    pub description: String,
    /// Value before the change point.
    pub before: CauseValue,
    /// Value after the change point.
    pub after: CauseValue,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TimelineChange {
    /// Key of the time series the change was detected on.
    #[schemars(length(min = 1, max = LIMITS.id_chars))]
    pub series_key: String,
    #[serde(deserialize_with = "nullable")]
    #[schemars(length(max = WEAKNESS_ID_MAX_CHARS), regex(pattern = WEAKNESS_ID_PATTERN))]
    pub weakness_id: Option<String>,
    #[schemars(length(min = 1, max = LIMITS.label_chars))]
    pub label: String,
    pub from_state: WeaknessState,
    pub to_state: WeaknessState,
    /// Game at which the change is placed.
    #[schemars(length(min = 1, max = LIMITS.id_chars))]
    pub onset_game_id: String,
    #[serde(with = "iso_date")]
    #[schemars(schema_with = "crate::schema::iso_date")]
    pub onset_date: Date,
    /// Uncertainty window for the onset: `onsetFrom <= onsetDate <= onsetTo`.
    #[serde(with = "iso_date")]
    #[schemars(schema_with = "crate::schema::iso_date")]
    pub onset_from: Date,
    #[serde(with = "iso_date")]
    #[schemars(schema_with = "crate::schema::iso_date")]
    pub onset_to: Date,
    /// Event rate (0..1) before the change.
    #[schemars(range(min = 0, max = 1))]
    pub rate_before: f64,
    /// Event rate (0..1) after the change.
    #[schemars(range(min = 0, max = 1))]
    pub rate_after: f64,
    /// Posterior probability that a change happened, 0..1.
    #[schemars(range(min = 0, max = 1))]
    pub posterior_prob: f64,
    #[schemars(length(max = LIMITS.candidate_causes))]
    pub candidate_causes: Vec<CandidateCause>,
    #[schemars(length(max = LIMITS.examples_per_change))]
    pub examples: Vec<ChangeExample>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Basis {
    /// Games with engine analysis, the evidence window size.
    #[schemars(range(max = MAX_COUNT))]
    pub analysed_games: u32,
    /// Games imported from the platform (>= `analysedGames`).
    #[schemars(range(max = MAX_COUNT))]
    pub imported_games: u32,
    #[serde(with = "iso_date")]
    #[schemars(schema_with = "crate::schema::iso_date")]
    pub window_start: Date,
    #[serde(with = "iso_date")]
    #[schemars(schema_with = "crate::schema::iso_date")]
    pub window_end: Date,
    /// Whether the full history import finished.
    pub backfill_complete: bool,
}

/// What R2 produces for the AI coach (R3) and the web: the M3 `CoachingInput`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CoachingInput {
    pub input_version: InputVersion,
    #[schemars(schema_with = "crate::schema::uuid")]
    pub account_id: Uuid,
    pub platform: Platform,
    pub time_class: TimeClass,
    /// Typical time control; null when mixed or unknown.
    #[serde(deserialize_with = "nullable")]
    pub time_control: Option<TimeControl>,
    pub band: BandId,
    /// Current rating; null when unknown.
    #[serde(deserialize_with = "nullable")]
    #[schemars(range(max = MAX_RATING))]
    pub rating: Option<u16>,
    #[serde(with = "common::js_time::iso")]
    #[schemars(schema_with = "crate::schema::iso_datetime")]
    pub generated_at: OffsetDateTime,
    pub basis: Basis,
    /// Each `AreaId` exactly once.
    #[schemars(length(max = AreaId::ALL.len()))]
    pub area_scores: Vec<AreaScore>,
    #[schemars(length(max = LIMITS.metrics))]
    pub metrics: Vec<Metric>,
    /// Ranked weaknesses; `rank` values are unique.
    #[schemars(length(max = LIMITS.weaknesses))]
    pub weaknesses: Vec<RankedWeakness>,
    #[schemars(length(max = LIMITS.strengths))]
    pub strengths: Vec<Strength>,
    #[schemars(length(max = LIMITS.openings))]
    pub openings: Vec<OpeningVerdict>,
    #[schemars(length(max = LIMITS.changes))]
    pub changes: Vec<TimelineChange>,
}

impl CoachingInput {
    /// The wire JSON. Every `Option` is present as `null`.
    pub fn to_json(&self) -> serde_json::Value {
        // Every field serialises infallibly: string keys, plain numbers (a non-finite f64
        // becomes null, which `validate` rejects) and always-formattable timestamps.
        serde_json::to_value(self).expect("CoachingInput serialises to JSON")
    }
}

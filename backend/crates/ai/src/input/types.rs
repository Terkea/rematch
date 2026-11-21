//! A field-for-field serde mirror of the M3 zod `CoachingInput`
//! (`.worktrees/m3/packages/ai/src/input/schema.ts`, m3/base@4ff8220).
//!
//! Every object is strict (`deny_unknown_fields`) and every key is present on the wire: a zod
//! `.nullable()` field is an `Option` that serialises as `null` and must be present when
//! deserialising (see [`nullable`]). Bounds and refinements are checked by `input::validate`
//! (Task 2); this module only fixes the shape. `tests/input_shape.rs` compares the schemars schema
//! of [`CoachingInput`] with the zod export in `fixtures/ai/ts-export/coaching-input.schema.json`.

use std::borrow::Cow;
use std::fmt;
use std::str::FromStr;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::de::{self, MapAccess, Visitor};
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use uuid::Uuid;

/// zod `z.literal("coaching-input-v1")`.
pub const INPUT_VERSION: &str = "coaching-input-v1";

/// zod `LIMITS`: size bounds applied at the trust boundary (lengths in Unicode code points).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    pub pv_plies: usize,
    pub tags: usize,
    pub tag_chars: usize,
    pub metrics: usize,
    pub weaknesses: usize,
    pub strengths: usize,
    pub openings: usize,
    pub changes: usize,
    pub alternatives: usize,
    pub candidate_causes: usize,
    pub moves_san: usize,
    pub san_chars: usize,
    pub label_chars: usize,
    pub description_chars: usize,
    pub id_chars: usize,
    pub exemplars_per_item: usize,
    pub examples_per_opening: usize,
    pub examples_per_change: usize,
}

pub const LIMITS: Limits = Limits {
    pv_plies: 16,
    tags: 12,
    tag_chars: 40,
    metrics: 80,
    weaknesses: 24,
    strengths: 12,
    openings: 30,
    changes: 20,
    alternatives: 5,
    candidate_causes: 6,
    moves_san: 40,
    san_chars: 12,
    label_chars: 120,
    description_chars: 600,
    id_chars: 64,
    exemplars_per_item: 5,
    examples_per_opening: 3,
    examples_per_change: 6,
};

/// Deserialises a zod `.nullable()` field: `null` or a value, but the key must be present
/// (`deserialize_with` turns off serde's missing-`Option`-is-`None` rule).
fn nullable<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}

/// The JSON schema of a zod `.nullable()` field: `Option<T>`'s schema (`T` or `null`), but the
/// key is required. schemars treats an `Option` field as optional and `#[schemars(required)]`
/// drops the `null`, so nullable fields use `#[schemars(with = "Nullable<T>")]`.
pub struct Nullable<T>(std::marker::PhantomData<T>);

impl<T: JsonSchema> JsonSchema for Nullable<T> {
    fn inline_schema() -> bool {
        true
    }

    fn schema_name() -> Cow<'static, str> {
        format!("Nullable_{}", T::schema_name()).into()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        Option::<T>::json_schema(generator)
    }
}

/// zod `z.literal("coaching-input-v1")`.
fn input_version_schema(_: &mut SchemaGenerator) -> Schema {
    json_schema!({ "type": "string", "const": INPUT_VERSION })
}

/// Returned by the `FromStr` impls of this module for an unknown wire string.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("unknown {kind}: {value:?}")]
pub struct UnknownVariant {
    pub kind: &'static str,
    pub value: String,
}

/// A string enum with `ALL`, `as_str`, `Display` and `FromStr`. The wire strings are listed
/// explicitly so serde, `as_str` and the JSON schema cannot disagree. Variants carry no doc
/// comments: schemars would turn the schema's `enum` into a `oneOf` of consts.
macro_rules! wire_enum {
    ($(#[$meta:meta])* $name:ident, $kind:literal, { $($variant:ident => $wire:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(
            Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
            JsonSchema,
        )]
        pub enum $name {
            $(#[serde(rename = $wire)] $variant),+
        }

        impl $name {
            /// Every variant, in declaration (zod) order.
            pub const ALL: [$name; [$($wire),+].len()] = [$($name::$variant),+];

            /// The wire string.
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
    };
}

wire_enum!(
    /// zod `AREA_IDS`.
    AreaId, "area id", {
        Openings => "openings",
        Middlegame => "middlegame",
        Endgame => "endgame",
        Tactics => "tactics",
        Calculation => "calculation",
        Time => "time",
        Mindset => "mindset",
    }
);

wire_enum!(
    /// zod `BAND_IDS`: the rating band on the Lichess-equivalent scale.
    BandId, "band id", {
        B0 => "B0",
        B1 => "B1",
        B2 => "B2",
        B3 => "B3",
        B4 => "B4",
        B5 => "B5",
    }
);

wire_enum!(
    /// zod `WEAKNESS_STATES`.
    WeaknessState, "weakness state", {
        NotAnIssue => "not_an_issue",
        Emerging => "emerging",
        Active => "active",
        Improving => "improving",
        Resolved => "resolved",
        Relapsed => "relapsed",
    }
);

wire_enum!(
    /// zod `METRIC_UNITS`.
    MetricUnit, "metric unit", {
        Rate => "rate",
        PerGame => "per_game",
        Pct => "pct",
        Seconds => "seconds",
        Elo => "elo",
    }
);

impl MetricUnit {
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
}

wire_enum!(
    /// `OpeningVerdict.verdict`.
    Verdict, "verdict", {
        Keep => "keep",
        Fix => "fix",
        Learn => "learn",
        Replace => "replace",
    }
);

wire_enum!(
    /// `OpeningVerdict.side`: `chosen` = the player plays the line, `faced` = the opponent does.
    Side, "side", {
        Chosen => "chosen",
        Faced => "faced",
    }
);

wire_enum!(
    /// `OpeningVerdict.color`: the player's colour in the line's games.
    Color, "color", {
        White => "white",
        Black => "black",
    }
);

wire_enum!(
    /// `CandidateCause.kind`.
    CauseKind, "cause kind", {
        TimeControlMix => "time_control_mix",
        NewOpening => "new_opening",
        SessionLength => "session_length",
        RatingLevel => "rating_level",
        ProtocolVersion => "protocol_version",
    }
);

wire_enum!(
    /// `CoachingInput.timeClass`.
    TimeClass, "time class", {
        Bullet => "bullet",
        Blitz => "blitz",
        Rapid => "rapid",
        Classical => "classical",
        Daily => "daily",
    }
);

wire_enum!(
    /// `CoachingInput.platform`.
    Platform, "platform", {
        Lichess => "lichess",
        Chesscom => "chesscom",
    }
);

wire_enum!(
    /// `TimelineChange.examples[].side`: which side of the change point the example is from.
    ExampleSide, "example side", {
        Before => "before",
        After => "after",
    }
);

/// Weakness catalogue id `W1`..`W12` with an optional variant letter (pattern checked in Task 2).
pub type WeaknessId = String;

/// Typical time control in seconds.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TimeControl {
    pub base_seconds: i64,
    pub increment_seconds: i64,
}

/// An engine evaluation from WHITE's point of view. Strict: exactly `{"cp": n}` or
/// `{"mate": n}`; any other or additional key is rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Score {
    /// Centipawns, positive = White better.
    Cp(i64),
    /// Moves to mate, positive = White mates.
    Mate(i64),
}

impl Serialize for Score {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(1))?;
        match self {
            Score::Cp(v) => map.serialize_entry("cp", v)?,
            Score::Mate(v) => map.serialize_entry("mate", v)?,
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for Score {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ScoreVisitor;

        impl<'de> Visitor<'de> for ScoreVisitor {
            type Value = Score;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(r#"{"cp": <integer>} or {"mate": <integer>}"#)
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Score, A::Error> {
                let Some(key) = map.next_key::<String>()? else {
                    return Err(de::Error::invalid_length(0, &self));
                };
                let score = match key.as_str() {
                    "cp" => Score::Cp(map.next_value()?),
                    "mate" => Score::Mate(map.next_value()?),
                    other => return Err(de::Error::unknown_field(other, &["cp", "mate"])),
                };
                if let Some(extra) = map.next_key::<String>()? {
                    return Err(de::Error::unknown_field(&extra, &[]));
                }
                Ok(score)
            }
        }

        deserializer.deserialize_map(ScoreVisitor)
    }
}

impl JsonSchema for Score {
    fn schema_name() -> Cow<'static, str> {
        "Score".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "anyOf": [
                {
                    "type": "object",
                    "properties": { "cp": { "type": "integer" } },
                    "required": ["cp"],
                    "additionalProperties": false,
                },
                {
                    "type": "object",
                    "properties": { "mate": { "type": "integer" } },
                    "required": ["mate"],
                    "additionalProperties": false,
                },
            ]
        })
    }
}

/// A move as SAN (display) and UCI (machine, e.g. `e2e4` or `e7e8q`).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MoveRef {
    pub san: String,
    pub uci: String,
}

/// A concrete position from one of the player's games, cited as evidence.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExemplarPosition {
    pub game_id: String,
    /// 1-based half-move number of the position's side to move: odd = White, even = Black.
    pub ply: u32,
    /// ISO datetime.
    pub played_at: String,
    /// Position BEFORE `played`, standard FEN.
    pub fen: String,
    pub played: MoveRef,
    /// The engine's preferred move, or null when `played` was already best.
    #[serde(deserialize_with = "nullable")]
    #[schemars(with = "Nullable<MoveRef>")]
    pub best: Option<MoveRef>,
    /// Engine line starting with the best move from `fen` (UCI).
    pub best_pv: Vec<String>,
    /// Engine line starting from the position AFTER `played` (UCI).
    pub refutation_pv: Vec<String>,
    pub tags: Vec<String>,
    pub eval_before: Score,
    pub eval_after: Score,
    /// The mover's win percentage before the move, 0..100.
    pub win_before: f64,
    /// The mover's win percentage after the move, 0..100.
    pub win_after: f64,
}

/// `TimelineChange.examples[]`: the exemplar fields plus `side`. The fields are repeated rather
/// than flattened because serde's `flatten` does not combine with `deny_unknown_fields`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChangeExample {
    pub game_id: String,
    pub ply: u32,
    pub played_at: String,
    pub fen: String,
    pub played: MoveRef,
    #[serde(deserialize_with = "nullable")]
    #[schemars(with = "Nullable<MoveRef>")]
    pub best: Option<MoveRef>,
    pub best_pv: Vec<String>,
    pub refutation_pv: Vec<String>,
    pub tags: Vec<String>,
    pub eval_before: Score,
    pub eval_after: Score,
    pub win_before: f64,
    pub win_after: f64,
    pub side: ExampleSide,
}

impl ChangeExample {
    /// The exemplar part of the example.
    pub fn exemplar(&self) -> ExemplarPosition {
        ExemplarPosition {
            game_id: self.game_id.clone(),
            ply: self.ply,
            played_at: self.played_at.clone(),
            fen: self.fen.clone(),
            played: self.played.clone(),
            best: self.best.clone(),
            best_pv: self.best_pv.clone(),
            refutation_pv: self.refutation_pv.clone(),
            tags: self.tags.clone(),
            eval_before: self.eval_before,
            eval_after: self.eval_after,
            win_before: self.win_before,
            win_after: self.win_after,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Metric {
    pub key: String,
    pub area: AreaId,
    pub label: String,
    pub value: f64,
    pub unit: MetricUnit,
    pub n: u32,
    #[serde(deserialize_with = "nullable")]
    #[schemars(with = "Nullable<f64>")]
    pub low: Option<f64>,
    #[serde(deserialize_with = "nullable")]
    #[schemars(with = "Nullable<f64>")]
    pub high: Option<f64>,
    #[serde(deserialize_with = "nullable")]
    #[schemars(with = "Nullable<f64>")]
    pub band_median: Option<f64>,
    #[serde(deserialize_with = "nullable")]
    #[schemars(with = "Nullable<f64>")]
    pub band_percentile: Option<f64>,
    pub sufficient: bool,
    #[serde(deserialize_with = "nullable")]
    #[schemars(with = "Nullable<u32>")]
    pub need_more: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RankedWeakness {
    pub id: WeaknessId,
    #[serde(deserialize_with = "nullable")]
    #[schemars(with = "Nullable<String>")]
    pub variant: Option<String>,
    pub label: String,
    pub area: AreaId,
    /// 1 = highest priority; unique across `weaknesses`.
    pub rank: i64,
    pub posterior_rate: f64,
    pub interval_low: f64,
    pub interval_high: f64,
    pub band_median: f64,
    pub band_percentile: f64,
    pub excess_epl_per_game: f64,
    pub confidence: f64,
    pub priority: f64,
    pub state: WeaknessState,
    #[serde(deserialize_with = "nullable")]
    #[schemars(with = "Nullable<f64>")]
    pub p_imp: Option<f64>,
    pub events: u32,
    pub opportunities: u32,
    pub exemplars: Vec<ExemplarPosition>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Strength {
    pub area: AreaId,
    pub label: String,
    #[serde(deserialize_with = "nullable")]
    #[schemars(with = "Nullable<String>")]
    pub metric_key: Option<String>,
    pub exemplars: Vec<ExemplarPosition>,
}

/// `OpeningVerdict.alternatives[]`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Alternative {
    pub name: String,
    pub moves_san: Vec<String>,
    pub band_score: f64,
    pub games: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OpeningVerdict {
    pub line_key: String,
    pub name: String,
    #[serde(deserialize_with = "nullable")]
    #[schemars(with = "Nullable<String>")]
    pub eco: Option<String>,
    pub color: Color,
    pub side: Side,
    pub moves_san: Vec<String>,
    pub games: u32,
    pub score: f64,
    #[serde(deserialize_with = "nullable")]
    #[schemars(with = "Nullable<f64>")]
    pub performance_vs_expected: Option<f64>,
    #[serde(deserialize_with = "nullable")]
    #[schemars(with = "Nullable<Score>")]
    pub avg_eval_at_opening_end: Option<Score>,
    pub verdict: Verdict,
    #[serde(deserialize_with = "nullable")]
    #[schemars(with = "Nullable<u32>")]
    pub deviation_ply: Option<u32>,
    #[serde(deserialize_with = "nullable")]
    #[schemars(with = "Nullable<String>")]
    pub deviation_move_san: Option<String>,
    pub alternatives: Vec<Alternative>,
    pub examples: Vec<ExemplarPosition>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AreaScore {
    pub area: AreaId,
    #[serde(deserialize_with = "nullable")]
    #[schemars(with = "Nullable<f64>")]
    pub score: Option<f64>,
    pub n: u32,
    pub sufficient: bool,
    #[serde(deserialize_with = "nullable")]
    #[schemars(with = "Nullable<u32>")]
    pub need_more: Option<u32>,
}

/// `CandidateCause.before`/`after`: a short text or a number.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum StrOrNum {
    Str(String),
    Num(f64),
}

/// A correlated possible cause of a change, never proven.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CandidateCause {
    pub kind: CauseKind,
    pub description: String,
    pub before: StrOrNum,
    pub after: StrOrNum,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TimelineChange {
    pub series_key: String,
    #[serde(deserialize_with = "nullable")]
    #[schemars(with = "Nullable<WeaknessId>")]
    pub weakness_id: Option<WeaknessId>,
    pub label: String,
    pub from_state: WeaknessState,
    pub to_state: WeaknessState,
    pub onset_game_id: String,
    /// ISO date `YYYY-MM-DD`; `onsetFrom <= onsetDate <= onsetTo`.
    pub onset_date: String,
    pub onset_from: String,
    pub onset_to: String,
    pub rate_before: f64,
    pub rate_after: f64,
    pub posterior_prob: f64,
    pub candidate_causes: Vec<CandidateCause>,
    pub examples: Vec<ChangeExample>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Basis {
    /// Games with engine analysis: the evidence window size.
    pub analysed_games: u32,
    pub imported_games: u32,
    /// ISO date.
    pub window_start: String,
    /// ISO date.
    pub window_end: String,
    pub backfill_complete: bool,
}

/// What the profile engine (R2) produces for the AI coach: the M3 `CoachingInput`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CoachingInput {
    /// Always [`INPUT_VERSION`] (checked in Task 2).
    #[schemars(schema_with = "input_version_schema")]
    pub input_version: String,
    pub account_id: Uuid,
    pub platform: Platform,
    pub time_class: TimeClass,
    #[serde(deserialize_with = "nullable")]
    #[schemars(with = "Nullable<TimeControl>")]
    pub time_control: Option<TimeControl>,
    pub band: BandId,
    #[serde(deserialize_with = "nullable")]
    #[schemars(with = "Nullable<i64>")]
    pub rating: Option<i64>,
    /// ISO datetime.
    pub generated_at: String,
    pub basis: Basis,
    pub area_scores: Vec<AreaScore>,
    pub metrics: Vec<Metric>,
    pub weaknesses: Vec<RankedWeakness>,
    pub strengths: Vec<Strength>,
    pub openings: Vec<OpeningVerdict>,
    pub changes: Vec<TimelineChange>,
}

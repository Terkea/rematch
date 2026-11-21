//! The `games` table.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

/// A text value that is not a member of the Postgres enum it should come from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseEnumError {
    pub kind: &'static str,
    pub value: String,
}

impl fmt::Display for ParseEnumError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown {} value {:?}", self.kind, self.value)
    }
}

impl std::error::Error for ParseEnumError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisStatus {
    Pending,
    Analysing,
    Done,
    Failed,
    Skipped,
    /// Imported but not (yet) selected for analysis (R2 scheduler).
    Imported,
}

impl AnalysisStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Analysing => "analysing",
            Self::Done => "done",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
            Self::Imported => "imported",
        }
    }
}

impl FromStr for AnalysisStatus {
    type Err = ParseEnumError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "pending" => Self::Pending,
            "analysing" => Self::Analysing,
            "done" => Self::Done,
            "failed" => Self::Failed,
            "skipped" => Self::Skipped,
            "imported" => Self::Imported,
            _ => {
                return Err(ParseEnumError {
                    kind: "analysis_status",
                    value: s.to_owned(),
                });
            }
        })
    }
}

impl TryFrom<String> for AnalysisStatus {
    type Error = ParseEnumError;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}

/// A game to insert. Enum columns are given as their Postgres text values
/// (`platform`: lichess | chesscom, `time_class`, `player_color`: white | black, `result`).
#[derive(Debug, Clone)]
pub struct NewGame {
    pub account_id: Uuid,
    pub platform: String,
    pub platform_game_id: String,
    pub pgn: String,
    pub played_at: OffsetDateTime,
    pub time_class: String,
    pub base_seconds: Option<i32>,
    pub increment_seconds: Option<i32>,
    pub rated: bool,
    pub player_color: String,
    pub white_name: String,
    pub black_name: String,
    pub white_rating: Option<i32>,
    pub black_rating: Option<i32>,
    pub result: String,
    pub termination: Option<String>,
    pub eco: Option<String>,
    pub opening_name: Option<String>,
    pub lichess_evals: Option<serde_json::Value>,
    /// Status of a newly inserted row (`Pending` is the R1 behaviour). The R2 importer
    /// passes `Imported` for games the scheduler has not selected. Ignored on conflict.
    pub status: AnalysisStatus,
    pub ply_count: Option<i32>,
}

/// Every `games` column.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct GameRow {
    pub id: Uuid,
    pub account_id: Uuid,
    pub platform: String,
    pub platform_game_id: String,
    pub pgn: String,
    pub played_at: OffsetDateTime,
    pub time_class: String,
    pub base_seconds: Option<i32>,
    pub increment_seconds: Option<i32>,
    pub rated: bool,
    pub player_color: String,
    pub white_name: String,
    pub black_name: String,
    pub white_rating: Option<i32>,
    pub black_rating: Option<i32>,
    pub result: String,
    pub termination: Option<String>,
    pub eco: Option<String>,
    pub opening_name: Option<String>,
    pub lichess_evals: Option<serde_json::Value>,
    #[sqlx(try_from = "String")]
    pub analysis_status: AnalysisStatus,
    pub analysis_protocol: Option<String>,
    pub analysis_error: Option<String>,
    pub white_accuracy: Option<f32>,
    pub black_accuracy: Option<f32>,
    pub analysis_started_at: Option<OffsetDateTime>,
    pub heartbeat_at: Option<OffsetDateTime>,
    pub analysis_attempts: i32,
    pub pending_since: Option<OffsetDateTime>,
    pub analysis_generation: i32,
    pub analysed_generation: Option<i32>,
    pub ply_count: Option<i32>,
    pub analysis_tier: Option<i16>,
    pub analysis_priority: Option<i16>,
    pub selected_at: Option<OffsetDateTime>,
    pub analysed_at: Option<OffsetDateTime>,
    pub enrichment_protocol: Option<String>,
}

const GAME_COLUMNS: &str = "id, account_id, platform::text AS platform, platform_game_id, pgn, \
    played_at, time_class::text AS time_class, base_seconds, increment_seconds, rated, \
    player_color::text AS player_color, white_name, black_name, white_rating, black_rating, \
    result::text AS result, termination, eco, opening_name, lichess_evals, \
    analysis_status::text AS analysis_status, analysis_protocol, analysis_error, \
    white_accuracy, black_accuracy, analysis_started_at, heartbeat_at, analysis_attempts, \
    pending_since, analysis_generation, analysed_generation, \
    ply_count, analysis_tier, analysis_priority, selected_at, analysed_at, enrichment_protocol";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UpsertResult {
    pub id: Uuid,
    /// True only when this call created the row.
    pub inserted: bool,
    /// The row's current status, so a re-import can re-enqueue games that are still pending.
    pub analysis_status: AnalysisStatus,
}

/// Inserts a game, or returns the existing one for the same (platform, platform_game_id).
pub async fn upsert_game(pool: &PgPool, g: &NewGame) -> Result<UpsertResult, sqlx::Error> {
    let created: Option<(Uuid, String)> = sqlx::query_as(
        "INSERT INTO games (account_id, platform, platform_game_id, pgn, played_at, time_class, \
         base_seconds, increment_seconds, rated, player_color, white_name, black_name, \
         white_rating, black_rating, result, termination, eco, opening_name, lichess_evals, \
         analysis_status, ply_count) \
         VALUES ($1, $2::platform, $3, $4, $5, $6::time_class, $7, $8, $9, $10::color, $11, $12, \
         $13, $14, $15::result, $16, $17, $18, $19, $20::analysis_status, $21) \
         ON CONFLICT (platform, platform_game_id) DO NOTHING \
         RETURNING id, analysis_status::text",
    )
    .bind(g.account_id)
    .bind(&g.platform)
    .bind(&g.platform_game_id)
    .bind(&g.pgn)
    .bind(g.played_at)
    .bind(&g.time_class)
    .bind(g.base_seconds)
    .bind(g.increment_seconds)
    .bind(g.rated)
    .bind(&g.player_color)
    .bind(&g.white_name)
    .bind(&g.black_name)
    .bind(g.white_rating)
    .bind(g.black_rating)
    .bind(&g.result)
    .bind(&g.termination)
    .bind(&g.eco)
    .bind(&g.opening_name)
    .bind(&g.lichess_evals)
    .bind(g.status.as_str())
    .bind(g.ply_count)
    .fetch_optional(pool)
    .await?;
    let (id, status, inserted) = match created {
        Some((id, status)) => (id, status, true),
        None => {
            let (id, status): (Uuid, String) = sqlx::query_as(
                "SELECT id, analysis_status::text FROM games \
                 WHERE platform = $1::platform AND platform_game_id = $2",
            )
            .bind(&g.platform)
            .bind(&g.platform_game_id)
            .fetch_optional(pool)
            .await?
            .ok_or_else(|| {
                sqlx::Error::Protocol("upsert_game: conflicting row vanished".to_owned())
            })?;
            (id, status, false)
        }
    };
    let analysis_status = status
        .parse()
        .map_err(|e: ParseEnumError| sqlx::Error::Decode(Box::new(e)))?;
    Ok(UpsertResult {
        id,
        inserted,
        analysis_status,
    })
}

pub async fn get_game(pool: &PgPool, id: Uuid) -> Result<Option<GameRow>, sqlx::Error> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {GAME_COLUMNS} FROM games WHERE id = $1"
    )))
    .bind(id)
    .fetch_optional(pool)
    .await
}

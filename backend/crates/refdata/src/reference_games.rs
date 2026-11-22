//! Reference game rows (R2 Task 17): the dump games selected for a band-reference build
//! (Task 21), their analysis status, and one observation per analysed side (Task 32).
//!
//! Players appear only as salted hashes (`white_hash`, `black_hash`, `player_hash`); no column
//! here takes a username.

use std::collections::HashSet;

use chess_core::{Color, TimeClass};
use db::games::AnalysisStatus;
use profile_core::types::{Band, PlayerGameMetrics};
use sqlx::{PgConnection, PgPool, Postgres, Transaction};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::builds::{RefdataError, Result, ensure_building};

/// Rows per insert statement in [`insert_reference_games`] (PGNs are a few KB each).
pub const INSERT_CHUNK: usize = 500;

/// A build's outstanding (`pending` or `analysing`) reference games. Parameter: `$1` build id.
pub const PENDING_REFERENCE_SQL: &str = "SELECT count(*) FROM reference_games \
     WHERE build_id = $1 AND status IN ('pending', 'analysing')";

/// One selected dump game.
#[derive(Clone, Debug, PartialEq)]
pub struct NewReferenceGame {
    pub lichess_game_id: String,
    pub pgn: String,
    pub time_class: TimeClass,
    pub base_seconds: Option<i32>,
    pub increment_seconds: Option<i32>,
    pub white_rating: Option<i32>,
    pub black_rating: Option<i32>,
    /// `sha256(salt ‖ lower(name))`, hex.
    pub white_hash: String,
    pub black_hash: String,
    /// Which sides were sampled as reference players (at least one). Only these are observed:
    /// the opponent of a sampled player is not a reference player (T21 review I1).
    pub white_selected: bool,
    pub black_selected: bool,
    /// The `result` enum label: `1-0`, `0-1`, `1/2-1/2` or `*`.
    pub result: String,
    pub termination: Option<String>,
    pub played_at: OffsetDateTime,
}

/// Inserts the games of a `building` build with status `pending` and returns their ids in input
/// order. A game already in the build (same `lichess_game_id`, e.g. a retried enqueue) keeps its
/// row and returns its existing id.
pub async fn insert_reference_games(
    pool: &PgPool,
    build_id: Uuid,
    games: &[NewReferenceGame],
) -> Result<Vec<Uuid>> {
    if games.is_empty() {
        return Ok(Vec::new());
    }
    let mut tx = pool.begin().await?;
    ensure_building(&mut tx, build_id).await?;
    let mut ids = Vec::with_capacity(games.len());
    for chunk in games.chunks(INSERT_CHUNK) {
        let col = |f: fn(&NewReferenceGame) -> Option<i32>| chunk.iter().map(f).collect::<Vec<_>>();
        let lichess_ids: Vec<&str> = chunk.iter().map(|g| g.lichess_game_id.as_str()).collect();
        sqlx::query(
            "INSERT INTO reference_games (build_id, lichess_game_id, pgn, time_class, \
               base_seconds, increment_seconds, white_rating, black_rating, white_hash, \
               black_hash, result, termination, played_at, status, white_selected, \
               black_selected) \
             SELECT $1, gid, pgn, tc::time_class, bs, inc, wr, br, wh, bh, res::result, term, \
               at, 'pending', ws, bsel \
             FROM UNNEST($2::text[], $3::text[], $4::text[], $5::int4[], $6::int4[], \
                         $7::int4[], $8::int4[], $9::text[], $10::text[], $11::text[], \
                         $12::text[], $13::timestamptz[], $14::bool[], $15::bool[]) \
               AS x(gid, pgn, tc, bs, inc, wr, br, wh, bh, res, term, at, ws, bsel) \
             ON CONFLICT (build_id, lichess_game_id) DO NOTHING",
        )
        .bind(build_id)
        .bind(&lichess_ids)
        .bind(chunk.iter().map(|g| g.pgn.as_str()).collect::<Vec<_>>())
        .bind(
            chunk
                .iter()
                .map(|g| g.time_class.as_str())
                .collect::<Vec<_>>(),
        )
        .bind(col(|g| g.base_seconds))
        .bind(col(|g| g.increment_seconds))
        .bind(col(|g| g.white_rating))
        .bind(col(|g| g.black_rating))
        .bind(
            chunk
                .iter()
                .map(|g| g.white_hash.as_str())
                .collect::<Vec<_>>(),
        )
        .bind(
            chunk
                .iter()
                .map(|g| g.black_hash.as_str())
                .collect::<Vec<_>>(),
        )
        .bind(chunk.iter().map(|g| g.result.as_str()).collect::<Vec<_>>())
        .bind(
            chunk
                .iter()
                .map(|g| g.termination.as_deref())
                .collect::<Vec<_>>(),
        )
        .bind(chunk.iter().map(|g| g.played_at).collect::<Vec<_>>())
        .bind(chunk.iter().map(|g| g.white_selected).collect::<Vec<_>>())
        .bind(chunk.iter().map(|g| g.black_selected).collect::<Vec<_>>())
        .execute(&mut *tx)
        .await?;
        let found: Vec<(String, Uuid)> = sqlx::query_as(
            "SELECT lichess_game_id, id FROM reference_games \
             WHERE build_id = $1 AND lichess_game_id = ANY($2::text[])",
        )
        .bind(build_id)
        .bind(&lichess_ids)
        .fetch_all(&mut *tx)
        .await?;
        let found: std::collections::HashMap<String, Uuid> = found.into_iter().collect();
        for gid in &lichess_ids {
            ids.push(*found.get(*gid).ok_or_else(|| {
                RefdataError::Decode(format!("reference game {gid} missing after insert"))
            })?);
        }
    }
    tx.commit().await?;
    Ok(ids)
}

/// Sets a reference game's analysis status (and error, cleared when `None`).
pub async fn set_reference_game_status(
    conn: &mut PgConnection,
    id: Uuid,
    status: AnalysisStatus,
    error: Option<&str>,
) -> Result<()> {
    let build: Option<Uuid> =
        sqlx::query_scalar("SELECT build_id FROM reference_games WHERE id = $1")
            .bind(id)
            .fetch_optional(&mut *conn)
            .await?;
    let build = build.ok_or(RefdataError::NotFound(id))?;
    ensure_building(conn, build).await?;
    sqlx::query(
        "UPDATE reference_games SET status = $2::analysis_status, error = $3 WHERE id = $1",
    )
    .bind(id)
    .bind(status.as_str())
    .bind(error)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// The build's games still `pending` or `analysing`; 0 means the build can be finalised.
pub async fn pending_reference_count(pool: &PgPool, build_id: Uuid) -> Result<i64> {
    let n = sqlx::query_scalar(PENDING_REFERENCE_SQL)
        .bind(build_id)
        .fetch_one(pool)
        .await?;
    Ok(n)
}

/// One analysed side of a reference game.
#[derive(Clone, Debug, PartialEq)]
pub struct ObservationRow {
    pub reference_game_id: Uuid,
    pub color: Color,
    pub player_hash: String,
    pub band: Band,
    pub time_class: TimeClass,
    pub player_rating: i32,
    pub opponent_rating: i32,
    /// Stored as JSON with its `version`; readers must treat another version as stale.
    pub metrics: PlayerGameMetrics,
}

/// Upserts observations (one per reference game and colour) of a `building` build inside the
/// caller's transaction (the build's share lock holds until it commits). Every game must belong
/// to `build_id` ([`RefdataError::BuildMismatch`] otherwise), and a row is never moved between
/// builds. A re-analysed side replaces its row.
pub async fn write_observations(
    tx: &mut Transaction<'_, Postgres>,
    build_id: Uuid,
    rows: &[ObservationRow],
) -> Result<()> {
    if rows.is_empty() {
        return Ok(());
    }
    let conn: &mut PgConnection = tx;
    ensure_building(conn, build_id).await?;
    let games: Vec<Uuid> = rows.iter().map(|r| r.reference_game_id).collect();
    let owned: HashSet<Uuid> =
        sqlx::query_scalar("SELECT id FROM reference_games WHERE id = ANY($1) AND build_id = $2")
            .bind(&games)
            .bind(build_id)
            .fetch_all(&mut *conn)
            .await?
            .into_iter()
            .collect();
    if let Some(game) = games.iter().find(|g| !owned.contains(g)) {
        return Err(RefdataError::BuildMismatch {
            game: *game,
            build: build_id,
        });
    }
    let metrics = rows
        .iter()
        .map(|r| serde_json::to_value(&r.metrics))
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| RefdataError::Decode(e.to_string()))?;
    sqlx::query(
        "INSERT INTO reference_observations (build_id, reference_game_id, color, player_hash, \
           band, time_class, player_rating, opponent_rating, metrics) \
         SELECT $1, gid, c::color, ph, b::band, tc::time_class, pr, opr, m \
         FROM UNNEST($2::uuid[], $3::text[], $4::text[], $5::text[], $6::text[], $7::int4[], \
                     $8::int4[], $9::jsonb[]) AS x(gid, c, ph, b, tc, pr, opr, m) \
         ON CONFLICT (reference_game_id, color) DO UPDATE SET \
           player_hash = excluded.player_hash, \
           band = excluded.band, time_class = excluded.time_class, \
           player_rating = excluded.player_rating, opponent_rating = excluded.opponent_rating, \
           metrics = excluded.metrics \
         WHERE reference_observations.build_id = excluded.build_id",
    )
    .bind(build_id)
    .bind(&games)
    .bind(rows.iter().map(|r| r.color.as_str()).collect::<Vec<_>>())
    .bind(
        rows.iter()
            .map(|r| r.player_hash.as_str())
            .collect::<Vec<_>>(),
    )
    .bind(rows.iter().map(|r| r.band.as_str()).collect::<Vec<_>>())
    .bind(
        rows.iter()
            .map(|r| r.time_class.as_str())
            .collect::<Vec<_>>(),
    )
    .bind(rows.iter().map(|r| r.player_rating).collect::<Vec<_>>())
    .bind(rows.iter().map(|r| r.opponent_rating).collect::<Vec<_>>())
    .bind(metrics)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

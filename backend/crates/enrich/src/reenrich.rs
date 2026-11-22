//! Re-enriching `pending_maia` difficulty once Maia is healthy (R2 Task 25 hand-off, owner
//! override, ADR-R2-14).
//!
//! A bounded, paged sweep over `move_features` rows with `difficulty_status = 'pending_maia'`:
//!
//! 1. Maia's health is checked first with a fresh `/healthz` ([`DifficultySource::healthy`]).
//!    When it is down, nothing is read or changed and no attempt is counted.
//! 2. Rows are read in `(game_id, ply)` order, at most `page_size` per page and `max_pages`
//!    pages per sweep. Plies `<= 10` are never re-enriched (they are `opening_ply`).
//! 3. Each page's inputs are rebuilt from the stored row (`moves`, `games` Elos, stored MultiPV)
//!    exactly as [`crate::enrich_game`] builds them, and sent to Maia:
//!    - `ok`: `p_best`, `p_ok`, `difficulty_model` are set;
//!    - `not_applicable`: final;
//!    - `pending_maia/invalid_response` (Maia healthy but its answer is invalid): one attempt is
//!      counted; at [`MAX_DIFFICULTY_ATTEMPTS`] the row becomes `not_applicable/model_error`;
//!    - any other pending reason means Maia failed mid-sweep: the row is left as it was, no
//!      attempt is counted, and the sweep stops.
//!
//! The sweep never writes an estimate that did not come from Maia. Callers recompute the
//! returned games' metrics.

use std::collections::BTreeSet;

use chess_core::Color;
use profile_core::features::DIFFICULTY_MIN_PLY;
use profile_core::types::MultipvLine;
use sqlx::PgPool;
use sqlx::types::Json;
use uuid::Uuid;

use crate::difficulty::{DifficultyOutcome, DifficultySource, PendingReason};
use crate::enrich_game::difficulty_input;

/// Invalid answers from a healthy Maia before a position is final (`not_applicable/model_error`).
pub const MAX_DIFFICULTY_ATTEMPTS: i16 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SweepOptions {
    pub page_size: i64,
    pub max_pages: u32,
}

impl Default for SweepOptions {
    fn default() -> Self {
        Self {
            page_size: 256,
            max_pages: 20,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SweepReport {
    /// `/healthz` answered with a Maia model id.
    pub maia_healthy: bool,
    /// Pending rows sent to Maia.
    pub checked: u32,
    /// Rows that got a Maia difficulty.
    pub resolved: u32,
    /// Rows Maia says are not applicable.
    pub not_applicable: u32,
    /// Rows that reached the attempt cap (`not_applicable/model_error`).
    pub capped: u32,
    /// Rows still pending after an invalid answer (below the cap).
    pub still_pending: u32,
    /// Maia failed during the sweep, which stopped.
    pub stopped_early: bool,
    /// Games whose rows changed, ascending (their metrics need recomputing).
    pub games: Vec<Uuid>,
}

#[derive(sqlx::FromRow)]
struct PendingRow {
    game_id: Uuid,
    ply: i32,
    multipv: Option<Json<Vec<MultipvLine>>>,
    attempts: i16,
    fen_before: String,
    uci: String,
    best_uci: Option<String>,
    win_before: f64,
    win_after: f64,
    color: String,
    white_rating: Option<i32>,
    black_rating: Option<i32>,
}

/// One bounded sweep. See the module docs.
pub async fn reenrich_pending_difficulty(
    pool: &PgPool,
    source: &dyn DifficultySource,
    opts: SweepOptions,
) -> Result<SweepReport, sqlx::Error> {
    let mut report = SweepReport {
        maia_healthy: source.healthy().await,
        ..SweepReport::default()
    };
    if !report.maia_healthy {
        tracing::info!("maia not healthy; pending difficulty left for a later sweep");
        return Ok(report);
    }
    let mut games = BTreeSet::new();
    let mut cursor: (Uuid, i32) = (Uuid::nil(), i32::MIN);
    'pages: for _ in 0..opts.max_pages {
        let rows: Vec<PendingRow> = sqlx::query_as(
            "SELECT mf.game_id, mf.ply, mf.multipv, mf.difficulty_attempts AS attempts, \
               m.fen_before, m.uci, m.best_uci, m.win_before, m.win_after, m.color::text AS color, \
               g.white_rating, g.black_rating \
             FROM move_features mf \
             JOIN moves m ON m.game_id = mf.game_id AND m.ply = mf.ply \
             JOIN games g ON g.id = mf.game_id \
             WHERE mf.difficulty_status = 'pending_maia' AND mf.ply >= $1 \
               AND (mf.game_id, mf.ply) > ($2, $3) \
             ORDER BY mf.game_id, mf.ply \
             LIMIT $4",
        )
        .bind(DIFFICULTY_MIN_PLY as i32)
        .bind(cursor.0)
        .bind(cursor.1)
        .bind(opts.page_size.max(1))
        .fetch_all(pool)
        .await?;
        let Some(last) = rows.last() else {
            break;
        };
        cursor = (last.game_id, last.ply);
        let inputs: Vec<_> = rows
            .iter()
            .map(|r| {
                let (elo_self, elo_oppo) = if r.color == Color::White.as_str() {
                    (r.white_rating, r.black_rating)
                } else {
                    (r.black_rating, r.white_rating)
                };
                difficulty_input(
                    &r.fen_before,
                    &r.uci,
                    r.best_uci.as_deref(),
                    r.win_before,
                    r.win_after,
                    r.multipv.as_ref().map(|Json(l)| l.as_slice()),
                    elo_self,
                    elo_oppo,
                )
            })
            .collect();
        let outcomes = source.estimate(&inputs).await;
        report.checked += rows.len() as u32;
        let mut tx = pool.begin().await?;
        for (r, o) in rows.iter().zip(&outcomes) {
            match o {
                DifficultyOutcome::Ok { difficulty: d } => {
                    sqlx::query(
                        "UPDATE move_features SET p_best = $3, p_ok = $4, difficulty_model = $5, \
                           difficulty_status = 'ok', difficulty_reason = NULL \
                         WHERE game_id = $1 AND ply = $2",
                    )
                    .bind(r.game_id)
                    .bind(r.ply)
                    .bind(d.p_best as f32)
                    .bind(d.p_ok as f32)
                    .bind(&d.model)
                    .execute(&mut *tx)
                    .await?;
                    report.resolved += 1;
                }
                DifficultyOutcome::NotApplicable { reason } => {
                    set_status(&mut tx, r, "not_applicable", reason.as_str(), r.attempts).await?;
                    report.not_applicable += 1;
                }
                DifficultyOutcome::PendingMaia {
                    reason: PendingReason::InvalidResponse,
                } => {
                    let attempts = r.attempts.saturating_add(1);
                    if attempts >= MAX_DIFFICULTY_ATTEMPTS {
                        set_status(&mut tx, r, "not_applicable", "model_error", attempts).await?;
                        report.capped += 1;
                    } else {
                        set_status(&mut tx, r, "pending_maia", "invalid_response", attempts)
                            .await?;
                        report.still_pending += 1;
                    }
                }
                DifficultyOutcome::PendingMaia { reason } => {
                    tracing::warn!(
                        reason = reason.as_str(),
                        "maia failed during the re-enrichment sweep; stopping"
                    );
                    report.stopped_early = true;
                    continue;
                }
            }
            games.insert(r.game_id);
        }
        tx.commit().await?;
        if report.stopped_early {
            break 'pages;
        }
        if (rows.len() as i64) < opts.page_size {
            break;
        }
    }
    report.games = games.into_iter().collect();
    Ok(report)
}

async fn set_status(
    tx: &mut sqlx::PgConnection,
    r: &PendingRow,
    status: &str,
    reason: &str,
    attempts: i16,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE move_features SET difficulty_status = $3, difficulty_reason = $4, \
           difficulty_attempts = $5 WHERE game_id = $1 AND ply = $2",
    )
    .bind(r.game_id)
    .bind(r.ply)
    .bind(status)
    .bind(reason)
    .bind(attempts)
    .execute(tx)
    .await?;
    Ok(())
}

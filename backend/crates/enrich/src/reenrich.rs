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
//!    - `pending_maia/invalid_response` (an item-level invalid answer about this position): one
//!      attempt is counted; at [`MAX_DIFFICULTY_ATTEMPTS`] the row becomes
//!      `not_applicable/model_error`;
//!    - any other pending outcome is a call-level failure (transport, timeout, or an unusable
//!      whole answer such as a wrong item count): no attempt is counted, the reason is left as it
//!      was, and the sweep stops. When Maia is still healthy afterwards, those rows are charged
//!      one call failure and go to the back of later sweeps (parked at [`MAX_CALL_FAILURES`]).
//!
//! Every update re-checks that the row is still pending with the attempt and call-failure counts
//! read, so a concurrent re-analysis or a second sweep is never overwritten or double-counted.
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

/// Failed calls (transport, timeout, an unusable whole answer) charged to a row while Maia
/// stayed healthy. Such rows move to the back of each sweep, so they never block the others; at
/// this cap the row is parked: it stays `pending_maia` (never final, never estimated) and the
/// sweep skips it. [`SweepReport::parked`] and [`count_parked`] show them, and
/// [`unpark_call_failures`] makes them eligible again (for example after a Maia redeploy).
pub const MAX_CALL_FAILURES: i16 = 5;

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
    /// Pending rows Maia answered about (call-level failures excluded).
    pub checked: u32,
    /// Rows that got a Maia difficulty.
    pub resolved: u32,
    /// Rows Maia says are not applicable.
    pub not_applicable: u32,
    /// Rows that reached the attempt cap (`not_applicable/model_error`).
    pub capped: u32,
    /// Rows still pending after an invalid answer (below the cap).
    pub still_pending: u32,
    /// A Maia call failed during the sweep, which stopped.
    pub stopped_early: bool,
    /// Games whose rows changed, ascending (their metrics need recomputing).
    pub games: Vec<Uuid>,
    /// Pending rows parked at [`MAX_CALL_FAILURES`], skipped by every sweep until unparked.
    pub parked: u64,
}

#[derive(sqlx::FromRow)]
struct PendingRow {
    game_id: Uuid,
    ply: i32,
    multipv: Option<Json<Vec<MultipvLine>>>,
    attempts: i16,
    call_failures: i16,
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
        report_parked(pool, &mut report).await?;
        return Ok(report);
    }
    let mut games = BTreeSet::new();
    let page_size = opts.page_size.max(1);
    // Keyset on (call failures, game, ply): rows whose calls failed go to the back.
    let mut cursor: (i16, Uuid, i32) = (i16::MIN, Uuid::nil(), i32::MIN);
    for _ in 0..opts.max_pages {
        let rows: Vec<PendingRow> = sqlx::query_as(
            "SELECT mf.game_id, mf.ply, mf.multipv, mf.difficulty_attempts AS attempts, \
               mf.difficulty_call_failures AS call_failures, \
               m.fen_before, m.uci, m.best_uci, m.win_before, m.win_after, m.color::text AS color, \
               g.white_rating, g.black_rating \
             FROM move_features mf \
             JOIN moves m ON m.game_id = mf.game_id AND m.ply = mf.ply \
             JOIN games g ON g.id = mf.game_id \
             WHERE mf.difficulty_status = 'pending_maia' AND mf.ply >= $1 \
               AND mf.difficulty_call_failures < $2 \
               AND (mf.difficulty_call_failures, mf.game_id, mf.ply) > ($3, $4, $5) \
             ORDER BY mf.difficulty_call_failures, mf.game_id, mf.ply \
             LIMIT $6",
        )
        .bind(DIFFICULTY_MIN_PLY as i32)
        .bind(MAX_CALL_FAILURES)
        .bind(cursor.0)
        .bind(cursor.1)
        .bind(cursor.2)
        .bind(page_size)
        .fetch_all(pool)
        .await?;
        let Some(last) = rows.last() else {
            break;
        };
        cursor = (last.call_failures, last.game_id, last.ply);
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

        // A call-level failure (transport, timeout, unusable answer): the rows it covers were
        // not answered about their positions, so they never gain an attempt.
        let call_failed: Vec<&PendingRow> = rows
            .iter()
            .zip(&outcomes)
            .filter(|(_, o)| is_call_failure(o))
            .map(|(r, _)| r)
            .collect();
        // Still healthy after the failure: the failure follows these rows, so they are charged
        // one call failure and move to the back. Down now: an outage, nothing is charged.
        let charge = !call_failed.is_empty() && source.healthy().await;

        let mut tx = pool.begin().await?;
        for (r, o) in rows.iter().zip(&outcomes) {
            let changed = match o {
                DifficultyOutcome::Ok { difficulty: d } => {
                    let n = sqlx::query(
                        "UPDATE move_features SET p_best = $3, p_ok = $4, difficulty_model = $5, \
                           difficulty_status = 'ok', difficulty_reason = NULL \
                         WHERE game_id = $1 AND ply = $2 AND difficulty_status = 'pending_maia' \
                           AND difficulty_attempts = $6 AND difficulty_call_failures = $7",
                    )
                    .bind(r.game_id)
                    .bind(r.ply)
                    .bind(d.p_best as f32)
                    .bind(d.p_ok as f32)
                    .bind(&d.model)
                    .bind(r.attempts)
                    .bind(r.call_failures)
                    .execute(&mut *tx)
                    .await?
                    .rows_affected();
                    report.resolved += n as u32;
                    n > 0
                }
                DifficultyOutcome::NotApplicable { reason } => {
                    let changed = set_status(
                        &mut tx,
                        r,
                        "not_applicable",
                        reason.as_str(),
                        r.attempts,
                        r.call_failures,
                    )
                    .await?;
                    report.not_applicable += u32::from(changed);
                    changed
                }
                DifficultyOutcome::PendingMaia {
                    reason: PendingReason::InvalidResponse,
                } => {
                    // An item-level invalid answer about this position: one attempt.
                    let attempts = r.attempts.saturating_add(1);
                    if attempts >= MAX_DIFFICULTY_ATTEMPTS {
                        let changed = set_status(
                            &mut tx,
                            r,
                            "not_applicable",
                            "model_error",
                            attempts,
                            r.call_failures,
                        )
                        .await?;
                        report.capped += u32::from(changed);
                        changed
                    } else {
                        let changed = set_status(
                            &mut tx,
                            r,
                            "pending_maia",
                            "invalid_response",
                            attempts,
                            r.call_failures,
                        )
                        .await?;
                        report.still_pending += u32::from(changed);
                        changed
                    }
                }
                DifficultyOutcome::PendingMaia { .. } => {
                    if charge {
                        sqlx::query(
                            "UPDATE move_features \
                               SET difficulty_call_failures = difficulty_call_failures + 1 \
                             WHERE game_id = $1 AND ply = $2 \
                               AND difficulty_status = 'pending_maia' \
                               AND difficulty_attempts = $3 AND difficulty_call_failures = $4",
                        )
                        .bind(r.game_id)
                        .bind(r.ply)
                        .bind(r.attempts)
                        .bind(r.call_failures)
                        .execute(&mut *tx)
                        .await?;
                    }
                    continue;
                }
            };
            report.checked += 1;
            if changed {
                games.insert(r.game_id);
            }
        }
        tx.commit().await?;
        if let Some(first) = call_failed.first() {
            let reason = match outcomes.iter().find(|o| is_call_failure(o)) {
                Some(DifficultyOutcome::PendingMaia { reason }) => reason.as_str(),
                _ => "unknown",
            };
            tracing::warn!(
                reason,
                rows = call_failed.len(),
                game_id = %first.game_id,
                ply = first.ply,
                maia_healthy_after = charge,
                "maia call failed during the re-enrichment sweep; stopping"
            );
            report.stopped_early = true;
            break;
        }
        if (rows.len() as i64) < page_size {
            break;
        }
    }
    report.games = games.into_iter().collect();
    report_parked(pool, &mut report).await?;
    Ok(report)
}

/// A pending outcome that is not an item-level invalid answer: the call, not the position,
/// failed (transport, timeout, an unusable whole answer, Maia not configured).
fn is_call_failure(o: &DifficultyOutcome) -> bool {
    matches!(o, DifficultyOutcome::PendingMaia { reason } if *reason != PendingReason::InvalidResponse)
}

/// Sets status, reason and attempts on a row still pending as read. True when it changed.
async fn set_status(
    tx: &mut sqlx::PgConnection,
    r: &PendingRow,
    status: &str,
    reason: &str,
    attempts: i16,
    call_failures: i16,
) -> Result<bool, sqlx::Error> {
    let n = sqlx::query(
        "UPDATE move_features SET difficulty_status = $3, difficulty_reason = $4, \
           difficulty_attempts = $5 \
         WHERE game_id = $1 AND ply = $2 AND difficulty_status = 'pending_maia' \
           AND difficulty_attempts = $6 AND difficulty_call_failures = $7",
    )
    .bind(r.game_id)
    .bind(r.ply)
    .bind(status)
    .bind(reason)
    .bind(attempts)
    .bind(r.attempts)
    .bind(call_failures)
    .execute(tx)
    .await?
    .rows_affected();
    Ok(n > 0)
}

/// Makes parked rows (`difficulty_call_failures >= MAX_CALL_FAILURES`, still `pending_maia`)
/// eligible for the sweep again by resetting their count to 0. Rows below the cap keep their
/// count, and so their place at the back of the sweep. Returns the rows reset.
pub async fn unpark_call_failures(pool: &PgPool) -> Result<u64, sqlx::Error> {
    Ok(sqlx::query(
        "UPDATE move_features SET difficulty_call_failures = 0 \
         WHERE difficulty_status = 'pending_maia' AND difficulty_call_failures >= $1",
    )
    .bind(MAX_CALL_FAILURES)
    .execute(pool)
    .await?
    .rows_affected())
}

/// Pending rows parked at [`MAX_CALL_FAILURES`] (skipped by the sweep until unparked).
pub async fn count_parked(pool: &PgPool) -> Result<u64, sqlx::Error> {
    let n: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM move_features \
         WHERE difficulty_status = 'pending_maia' AND difficulty_call_failures >= $1",
    )
    .bind(MAX_CALL_FAILURES)
    .fetch_one(pool)
    .await?;
    Ok(n.max(0) as u64)
}

/// Sets `report.parked` and warns when any row is parked.
async fn report_parked(pool: &PgPool, report: &mut SweepReport) -> Result<(), sqlx::Error> {
    report.parked = count_parked(pool).await?;
    if report.parked > 0 {
        tracing::warn!(
            parked = report.parked,
            "pending difficulty rows parked after repeated failed maia calls; \
             run unpark_call_failures once maia is fixed"
        );
    }
    Ok(())
}

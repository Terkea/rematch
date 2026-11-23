//! Coaching-input snapshots (ADR 0086): build-profile stores the `CoachingInput` of the profile it
//! just wrote in `coaching_inputs`, so the coach reads an input as current as the profile (finding
//! Q5: chat read the newest report's input, which is throttled and can be an older contract
//! version). Snapshots are immutable and pruned to the newest [`SNAPSHOTS_KEPT`] per account ×
//! time class, except any a stored chat answer cites (`coach_messages.coaching_input_id`).

use chess_core::TimeClass;
use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

use super::builder::{BuildError, build_coaching_input};

/// Snapshots kept per account × time class besides the ones an answer cites.
pub const SNAPSHOTS_KEPT: usize = 5;

/// Builds the account × time class input at `now` (the same builder the report job uses),
/// stores it and prunes the old ones. Returns the new snapshot's id; `None` without analysed
/// games (there is no input to coach from).
pub async fn store_coaching_snapshot(
    pool: &PgPool,
    account_id: Uuid,
    tc: TimeClass,
    now: OffsetDateTime,
) -> Result<Option<Uuid>, BuildError> {
    // The builder's future is not `Send` (it holds a `RefCell` across awaits), so it runs to
    // completion on a blocking thread, driven by this runtime's handle.
    let rt = tokio::runtime::Handle::current();
    let p = pool.clone();
    let input = tokio::task::spawn_blocking(move || {
        rt.block_on(build_coaching_input(&p, account_id, tc, now))
    })
    .await
    .map_err(|e| BuildError::Db(sqlx::Error::Protocol(format!("snapshot builder task: {e}"))))??;
    if input.basis.analysed_games == 0 {
        return Ok(None);
    }
    let id: Uuid = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "{}INSERT INTO coaching_inputs (account_id, time_class, input_version, generated_at, \
         input, algorithm_id) VALUES ($1, $2::time_class, $3, $4, $5, $6) RETURNING id",
        common::algorithm::register_cte(6, 7)
    )))
    .bind(account_id)
    .bind(tc.as_str())
    .bind(input.input_version.as_str())
    .bind(input.generated_at)
    .bind(input.to_json())
    .bind(common::algorithm::id())
    .bind(common::algorithm::manifest())
    .fetch_one(pool)
    .await?;
    prune_coaching_snapshots(pool, account_id, tc).await?;
    Ok(Some(id))
}

/// Deletes the account × time class's snapshots older than the newest [`SNAPSHOTS_KEPT`] that no
/// chat answer cites. Returns how many were deleted.
pub async fn prune_coaching_snapshots(
    pool: &PgPool,
    account_id: Uuid,
    tc: TimeClass,
) -> sqlx::Result<u64> {
    let r = sqlx::query(
        "DELETE FROM coaching_inputs c WHERE c.account_id = $1 AND c.time_class = $2::time_class \
         AND c.id NOT IN (SELECT k.id FROM coaching_inputs k WHERE k.account_id = $1 \
           AND k.time_class = $2::time_class ORDER BY k.created_at DESC, k.id DESC LIMIT $3) \
         AND NOT EXISTS (SELECT 1 FROM coach_messages m WHERE m.coaching_input_id = c.id)",
    )
    .bind(account_id)
    .bind(tc.as_str())
    .bind(i64::try_from(SNAPSHOTS_KEPT).unwrap_or(i64::MAX))
    .execute(pool)
    .await?;
    Ok(r.rows_affected())
}

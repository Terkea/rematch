//! The `user_events` table: commit-ordered inserts, replay after a cursor, and pruning with a
//! watermark that tells stale clients to refetch (ADR 0034, ADR 0029).

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{Connection, PgConnection, PgPool};
use time::OffsetDateTime;
use uuid::Uuid;

/// Advisory lock key that serialises user-event inserts (same value as the TS reference).
pub const USER_EVENTS_LOCK_KEY: i64 = 7_262_011;

/// The LISTEN/NOTIFY channel every user event is announced on.
pub const CHANNEL: &str = "user_events";

/// How long events are kept for replay.
pub const EVENT_RETENTION: Duration = Duration::from_secs(7 * 24 * 3600);

/// NOTIFY payloads above this many bytes are sent without `payload` (Postgres's limit is 8000).
pub(crate) const MAX_NOTIFY_BYTES: usize = 7900;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),
    #[error("malformed user event notification: {0}")]
    Json(#[from] serde_json::Error),
    /// The LISTEN connection was lost and re-established; notifications in the gap are gone,
    /// so the caller must replay with `list_events_after` from its cursor.
    #[error("user events listener reconnected; replay from the last seen id")]
    Reconnected,
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum UserEventType {
    #[serde(rename = "import.progress")]
    ImportProgress,
    #[serde(rename = "game.analysed")]
    GameAnalysed,
    #[serde(rename = "job.failed")]
    JobFailed,
}

impl UserEventType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ImportProgress => "import.progress",
            Self::GameAnalysed => "game.analysed",
            Self::JobFailed => "job.failed",
        }
    }
}

/// What an insert returns: the id and the row's own `created_at`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StoredEvent {
    pub id: i64,
    pub created_at: OffsetDateTime,
}

/// One stored event. It serialises to the same JSON as the live
/// [`LiveEvent`](crate::listener::LiveEvent), byte for byte, so replay and live delivery look
/// identical to clients.
#[derive(Clone, Debug, PartialEq, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct UserEventRow {
    pub id: i64,
    pub account_id: Uuid,
    #[serde(rename = "type")]
    pub ty: String,
    #[serde(with = "common::js_time::iso_micros")]
    pub created_at: OffsetDateTime,
    pub payload: Value,
}

/// One page of replay. `reset` means the cursor predates pruned events: refetch state.
#[derive(Clone, Debug, PartialEq)]
pub struct EventsPage {
    pub reset: bool,
    pub events: Vec<UserEventRow>,
}

/// The NOTIFY JSON (camelCase). `payload` is left out and `truncated` set when it would not fit.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Notification {
    pub id: i64,
    pub account_id: Uuid,
    #[serde(rename = "type")]
    pub ty: String,
    /// UTC with exactly six fractional digits, so it equals the row's `created_at` exactly.
    #[serde(with = "common::js_time::iso_micros")]
    pub created_at: OffsetDateTime,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<Value>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub truncated: bool,
}

impl Notification {
    pub(crate) fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string(self)?)
    }
}

pub(crate) fn notification_json(
    stored: StoredEvent,
    account_id: Uuid,
    ty: &str,
    payload: &Value,
) -> Result<String> {
    let mut note = Notification {
        id: stored.id,
        account_id,
        ty: ty.to_owned(),
        created_at: stored.created_at,
        payload: Some(payload.clone()),
        truncated: false,
    };
    let full = note.to_json()?;
    if full.len() <= MAX_NOTIFY_BYTES {
        return Ok(full);
    }
    note.payload = None;
    note.truncated = true;
    note.to_json()
}

/// Inserts one event and queues its NOTIFY. Runs in a transaction: a savepoint when `conn` is
/// already inside one (the usual case, so the event commits with the state change it
/// describes), otherwise its own. The advisory lock is held until the outermost transaction
/// ends, so ids commit, and NOTIFYs are delivered, in id order.
///
/// Make this the last statement before the commit. Every event writer queues on the one
/// advisory lock, so taking it early holds up all other writers for the rest of the
/// transaction, and taking row locks after it can deadlock with a writer that holds those rows
/// and is waiting for the lock.
pub async fn insert_user_event(
    conn: &mut PgConnection,
    account_id: Uuid,
    ty: &str,
    payload: &Value,
) -> Result<StoredEvent> {
    let mut tx = conn.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(USER_EVENTS_LOCK_KEY)
        .execute(&mut *tx)
        .await?;
    let (id, created_at): (i64, OffsetDateTime) = sqlx::query_as(
        "INSERT INTO user_events (account_id, type, payload) VALUES ($1, $2, $3) \
         RETURNING id, created_at",
    )
    .bind(account_id)
    .bind(ty)
    .bind(payload)
    .fetch_one(&mut *tx)
    .await?;
    let stored = StoredEvent { id, created_at };
    let json = notification_json(stored, account_id, ty, payload)?;
    sqlx::query("SELECT pg_notify($1, $2)")
        .bind(CHANNEL)
        .bind(json)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(stored)
}

/// Records an event in its own transaction and returns its id. Call it only after the state
/// change it describes has committed (or use `insert_user_event` inside that transaction).
pub async fn publish(
    pool: &PgPool,
    account_id: Uuid,
    ty: UserEventType,
    payload: Value,
) -> Result<i64> {
    let mut tx = pool.begin().await?;
    let stored = insert_user_event(&mut tx, account_id, ty.as_str(), &payload).await?;
    tx.commit().await?;
    Ok(stored.id)
}

/// Events for the given accounts with `id > after_id`, oldest first, plus the reset marker
/// (`after_id < pruned_through_id`). Both are read from one snapshot.
pub async fn list_events_after(
    pool: &PgPool,
    account_ids: &[Uuid],
    after_id: i64,
    limit: i64,
) -> Result<EventsPage> {
    if account_ids.is_empty() {
        return Ok(EventsPage {
            reset: false,
            events: Vec::new(),
        });
    }
    let mut tx = pool
        .begin_with("BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .await?;
    let pruned_through: i64 =
        sqlx::query_scalar("SELECT pruned_through_id FROM user_events_watermark")
            .fetch_one(&mut *tx)
            .await?;
    let events: Vec<UserEventRow> = sqlx::query_as(
        "SELECT id, account_id, type AS ty, payload, created_at FROM user_events \
         WHERE account_id = ANY($1) AND id > $2 ORDER BY id LIMIT $3",
    )
    .bind(account_ids)
    .bind(after_id)
    .bind(limit)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(EventsPage {
        reset: after_id < pruned_through,
        events,
    })
}

/// Deletes events older than `older_than` and raises the watermark to the highest deleted id.
/// Returns how many rows were deleted.
pub async fn prune_events(pool: &PgPool, older_than: Duration) -> Result<u64> {
    let deleted: i64 = sqlx::query_scalar(
        "WITH d AS (DELETE FROM user_events \
                    WHERE created_at < now() - $1::float8 * interval '1 second' RETURNING id) \
         UPDATE user_events_watermark \
            SET pruned_through_id = GREATEST(pruned_through_id, coalesce((SELECT max(id) FROM d), 0)) \
         RETURNING (SELECT count(*) FROM d)",
    )
    .bind(older_than.as_secs_f64())
    .fetch_one(pool)
    .await?;
    u64::try_from(deleted).map_err(|e| {
        Error::Db(sqlx::Error::Decode(
            format!("prune count {deleted}: {e}").into(),
        ))
    })
}

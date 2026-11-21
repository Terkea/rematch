//! Live user events over `LISTEN user_events` (ADR 0034).
//!
//! # Consumer protocol (SSE route, polling fallback)
//!
//! 1. **Subscribe and buffer first.** Hold a running [`EventListener`] (or register with the
//!    process-wide one) and buffer what it delivers before reading anything from the table.
//!    A client without a `Last-Event-ID` reads its starting cursor,
//!    `SELECT coalesce(max(id), 0) FROM user_events`, only *after* subscribing, so no event can
//!    fall between the cursor and the subscription.
//! 2. **Replay.** Call [`list_events_after`](crate::store::list_events_after) with
//!    `(accounts, last_id, limit)` in a loop, advancing `last_id` to the last returned id, until a page has
//!    fewer than `limit` events. On `reset == true`, tell the client to refetch its state, then
//!    carry on from the newest id.
//! 3. **Drain, then go live.** Deliver the buffered events, dropping every `id <= last_id`, and
//!    then every new event, again dropping `id <= last_id` and advancing `last_id`.
//!
//! The dedupe in step 3 is exact because live delivery is in id order: the advisory lock in
//! [`insert_user_event`](crate::store::insert_user_event) makes commit order equal id order,
//! and Postgres delivers NOTIFYs in commit order. Once an id above `last_id` has arrived, no
//! lower id can follow.
//!
//! # Errors from [`EventListener::next`]
//!
//! - [`Error::Reconnected`]: the LISTEN connection was lost and has already been re-established.
//!   Events in the gap were not delivered live, so go back to step 2 (replay from `last_id`)
//!   and keep calling `next`.
//! - [`Error::Db`]: a reconnect or a payload fetch failed. Nothing is lost: keep calling `next`
//!   with a backoff. It retries the same reconnect or fetch, and a successful reconnect then
//!   returns `Reconnected`.
//! - [`Error::Json`]: one malformed notification was skipped. Keep calling `next`.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgPool;
use sqlx::postgres::PgListener;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::store::{CHANNEL, Error, Notification, Result, UserEventRow};

/// One event as delivered to clients, live or replayed (the single DTO; see
/// `From<UserEventRow>`). `payload` is `None` only when the NOTIFY was truncated and the row was
/// pruned before it could be fetched.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveEvent {
    pub id: i64,
    pub account_id: Uuid,
    #[serde(rename = "type")]
    pub ty: String,
    #[serde(with = "common::js_time::iso_micros")]
    pub created_at: OffsetDateTime,
    pub payload: Option<Value>,
}

impl From<UserEventRow> for LiveEvent {
    fn from(row: UserEventRow) -> Self {
        Self {
            id: row.id,
            account_id: row.account_id,
            ty: row.ty,
            created_at: row.created_at,
            payload: Some(row.payload),
        }
    }
}

impl LiveEvent {
    fn from_wire(wire: Notification, payload: Option<Value>) -> Self {
        Self {
            id: wire.id,
            account_id: wire.account_id,
            ty: wire.ty,
            created_at: wire.created_at,
            payload,
        }
    }
}

/// A `LISTEN user_events` subscription for every account (callers filter by account). It holds
/// one connection of the pool it was created from for as long as it lives, and uses a second
/// one briefly to fetch truncated payloads. See the module docs for the consumer protocol.
pub struct EventListener {
    pool: PgPool,
    /// `None` after the connection was lost and re-establishing it failed.
    listener: Option<PgListener>,
    /// A truncated notification whose payload is still being fetched. It stays here until
    /// `next` returns it, so a cancelled `next` loses nothing.
    pending: Option<Notification>,
}

async fn listen(pool: &PgPool) -> Result<PgListener> {
    let mut listener = PgListener::connect_with(pool).await?;
    // A lost connection is rebuilt here, not inside sqlx, so the gap is always reported.
    listener.eager_reconnect(false);
    listener.listen(CHANNEL).await?;
    Ok(listener)
}

impl EventListener {
    pub async fn connect(pool: &PgPool) -> Result<Self> {
        Ok(Self {
            pool: pool.clone(),
            listener: Some(listen(pool).await?),
            pending: None,
        })
    }

    /// The next event, in id order. A truncated NOTIFY is completed by reading the row by id.
    ///
    /// Cancel-safe: dropping the future loses no event that was already received. Errors are
    /// described in the module docs. `Reconnected` is returned only after the new LISTEN is
    /// in place.
    pub async fn next(&mut self) -> Result<LiveEvent> {
        if self.listener.is_none() {
            return self.reconnect().await;
        }
        if self.pending.is_none() {
            let listener = self.listener.as_mut().expect("checked above");
            let note = match listener.try_recv().await {
                Ok(Some(note)) => note,
                Ok(None) => {
                    tracing::warn!("user events LISTEN connection lost");
                    self.listener = None;
                    return self.reconnect().await;
                }
                Err(error) => {
                    tracing::warn!(%error, "user events LISTEN connection failed");
                    self.listener = None;
                    return self.reconnect().await;
                }
            };
            let wire: Notification = serde_json::from_str(note.payload())?;
            if !wire.truncated {
                let payload = wire.payload.clone();
                return Ok(LiveEvent::from_wire(wire, payload));
            }
            self.pending = Some(wire);
        }
        let id = self.pending.as_ref().expect("set above").id;
        let payload: Option<Value> =
            sqlx::query_scalar("SELECT payload FROM user_events WHERE id = $1")
                .bind(id)
                .fetch_optional(&self.pool)
                .await?;
        let wire = self.pending.take().expect("still pending");
        Ok(LiveEvent::from_wire(wire, payload))
    }

    /// Re-establishes the LISTEN. On success, returns `Err(Reconnected)` so the caller
    /// replays. On failure, returns `Err(Db)` and the next call tries again.
    async fn reconnect(&mut self) -> Result<LiveEvent> {
        let listener = listen(&self.pool).await?;
        self.listener = Some(listener);
        Err(Error::Reconnected)
    }
}

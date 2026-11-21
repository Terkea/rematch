//! Live user events over `LISTEN user_events` (ADR 0034).

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgPool;
use sqlx::postgres::PgListener;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::store::{CHANNEL, Error, Notification, Result};

/// One event as delivered live. `payload` is `None` only when the NOTIFY was truncated and the
/// row was pruned before it could be fetched.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveEvent {
    pub id: i64,
    pub account_id: Uuid,
    #[serde(rename = "type")]
    pub ty: String,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    pub payload: Option<Value>,
}

/// A `LISTEN user_events` subscription (one dedicated pool connection).
pub struct EventListener {
    pool: PgPool,
    listener: PgListener,
}

impl EventListener {
    pub async fn connect(pool: &PgPool) -> Result<Self> {
        let mut listener = PgListener::connect_with(pool).await?;
        listener.listen(CHANNEL).await?;
        Ok(Self {
            pool: pool.clone(),
            listener,
        })
    }

    /// The next event, for every account (callers filter by account). A truncated NOTIFY is
    /// completed by reading the row by id. Returns [`Error::Reconnected`] once after the LISTEN
    /// connection was lost and re-established: replay from the cursor, then keep calling.
    pub async fn next(&mut self) -> Result<LiveEvent> {
        let Some(note) = self.listener.try_recv().await? else {
            return Err(Error::Reconnected);
        };
        let wire: Notification = serde_json::from_str(note.payload())?;
        let payload = if wire.truncated {
            sqlx::query_scalar("SELECT payload FROM user_events WHERE id = $1")
                .bind(wire.id)
                .fetch_optional(&self.pool)
                .await?
        } else {
            wire.payload
        };
        Ok(LiveEvent {
            id: wire.id,
            account_id: wire.account_id,
            ty: wire.ty,
            created_at: wire.created_at,
            payload,
        })
    }
}

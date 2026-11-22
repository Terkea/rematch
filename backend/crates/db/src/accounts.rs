//! The `linked_accounts` table.

use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

/// `linked_accounts.import_cursor`: camelCase keys with ISO strings, the shape TS writes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportCursor {
    #[serde(default, with = "common::js_time::iso::option")]
    pub oldest_reached: Option<OffsetDateTime>,
    #[serde(default, with = "common::js_time::iso::option")]
    pub newest_seen_at: Option<OffsetDateTime>,
    /// An open frontier left by a run that stopped at `max`: everything above `newest_seen_at`
    /// is imported, and the next run continues below `oldest_reached` with no lower bound.
    /// Optional in the JSON (TS `backfill?: boolean`); omitted when false.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub backfill: bool,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AccountRow {
    pub id: Uuid,
    pub user_id: Option<Uuid>,
    pub platform: String,
    /// Always lower-cased.
    pub username: String,
    pub display_name: String,
    pub verified_at: Option<OffsetDateTime>,
    pub last_synced_at: Option<OffsetDateTime>,
    #[sqlx(json(nullable))]
    pub import_cursor: Option<ImportCursor>,
    pub created_at: OffsetDateTime,
}

const ACCOUNT_COLUMNS: &str = "id, user_id, platform::text AS platform, username, display_name, \
    verified_at, last_synced_at, import_cursor, created_at";

pub async fn get_account(pool: &PgPool, id: Uuid) -> Result<Option<AccountRow>, sqlx::Error> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {ACCOUNT_COLUMNS} FROM linked_accounts WHERE id = $1"
    )))
    .bind(id)
    .fetch_optional(pool)
    .await
}

/// Returns the account for (platform, lower-cased username), creating it when missing. The display
/// name is kept as given on creation and left alone afterwards.
pub async fn find_or_create_account(
    pool: &PgPool,
    platform: &str,
    username: &str,
) -> Result<AccountRow, sqlx::Error> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "INSERT INTO linked_accounts (platform, username, display_name) \
         VALUES ($1::platform, $2, $3) \
         ON CONFLICT (platform, username) DO UPDATE SET display_name = linked_accounts.display_name \
         RETURNING {ACCOUNT_COLUMNS}"
    )))
    .bind(platform)
    .bind(username.to_lowercase())
    .bind(username)
    .fetch_one(pool)
    .await
}

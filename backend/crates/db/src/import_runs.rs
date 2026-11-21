//! The `import_runs` table.

use std::str::FromStr;

use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::games::ParseEnumError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportRunStatus {
    Running,
    Done,
    Failed,
    Paused,
}

impl ImportRunStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Done => "done",
            Self::Failed => "failed",
            Self::Paused => "paused",
        }
    }
}

impl FromStr for ImportRunStatus {
    type Err = ParseEnumError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "running" => Self::Running,
            "done" => Self::Done,
            "failed" => Self::Failed,
            "paused" => Self::Paused,
            _ => {
                return Err(ParseEnumError {
                    kind: "import_run_status",
                    value: s.to_owned(),
                });
            }
        })
    }
}

impl TryFrom<String> for ImportRunStatus {
    type Error = ParseEnumError;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}

/// The stable codes stored in `import_runs.error_code`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportErrorCode {
    NotFound,
    RateLimited,
    Timeout,
    CircuitOpen,
    Shutdown,
    Unknown,
}

impl ImportErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotFound => "not_found",
            Self::RateLimited => "rate_limited",
            Self::Timeout => "timeout",
            Self::CircuitOpen => "circuit_open",
            Self::Shutdown => "shutdown",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ImportRunRow {
    pub id: Uuid,
    pub account_id: Uuid,
    #[sqlx(try_from = "String")]
    pub status: ImportRunStatus,
    pub imported: i32,
    pub started_at: OffsetDateTime,
    pub heartbeat_at: OffsetDateTime,
    pub finished_at: Option<OffsetDateTime>,
    pub last_error: Option<String>,
    /// An [`ImportErrorCode::as_str`] value (kept as text so an unknown code never breaks a read).
    pub error_code: Option<String>,
}

/// The account's runs, newest first.
pub async fn list_runs(pool: &PgPool, account_id: Uuid) -> Result<Vec<ImportRunRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, account_id, status::text AS status, imported, started_at, heartbeat_at, \
         finished_at, last_error, error_code FROM import_runs \
         WHERE account_id = $1 ORDER BY started_at DESC, id DESC",
    )
    .bind(account_id)
    .fetch_all(pool)
    .await
}

//! Connection pool with a server-side statement timeout on every connection.

use std::str::FromStr;
use std::time::Duration;

use sqlx::PgPool;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};

/// Postgres `statement_timeout` on every worker connection, so no query can hang a job forever.
pub const WORKER_STATEMENT_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Debug)]
pub struct DbOptions {
    pub max_connections: u32,
    pub statement_timeout: Duration,
    pub acquire_timeout: Duration,
}

impl Default for DbOptions {
    fn default() -> Self {
        Self {
            max_connections: 10,
            statement_timeout: WORKER_STATEMENT_TIMEOUT,
            acquire_timeout: Duration::from_secs(10),
        }
    }
}

/// Connects a pool. `statement_timeout` is set through the connect options (a startup parameter),
/// so it applies to every connection the pool ever opens.
pub async fn connect(url: &str, opts: DbOptions) -> Result<PgPool, sqlx::Error> {
    let connect_options = PgConnectOptions::from_str(url)?.options([(
        "statement_timeout",
        opts.statement_timeout.as_millis().to_string(),
    )]);
    PgPoolOptions::new()
        .max_connections(opts.max_connections)
        .acquire_timeout(opts.acquire_timeout)
        .connect_with(connect_options)
        .await
}

/// Applies every pending migration from `db/migrations/`.
pub async fn migrate(pool: &PgPool) -> Result<(), sqlx::migrate::MigrateError> {
    crate::MIGRATOR.run(pool).await
}

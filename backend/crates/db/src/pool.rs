//! Connection pool with a server-side statement timeout and TCP keepalives on every connection.

use std::str::FromStr;
use std::time::Duration;

use sqlx::PgPool;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};

/// Postgres `statement_timeout` on every worker connection, so no query can hang a job forever.
pub const WORKER_STATEMENT_TIMEOUT: Duration = Duration::from_secs(30);

/// Server-side TCP keepalives on every connection: `(idle s, interval s, count)`. A peer that
/// vanished without closing its socket is detected after `idle + interval * count` (90 s), so
/// its session (and any advisory lock it held) ends.
pub const TCP_KEEPALIVES: (u32, u32, u32) = (60, 10, 3);

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
    let connect_options = PgConnectOptions::from_str(url)?.options([
        (
            "statement_timeout",
            opts.statement_timeout.as_millis().to_string(),
        ),
        ("tcp_keepalives_idle", TCP_KEEPALIVES.0.to_string()),
        ("tcp_keepalives_interval", TCP_KEEPALIVES.1.to_string()),
        ("tcp_keepalives_count", TCP_KEEPALIVES.2.to_string()),
    ]);
    PgPoolOptions::new()
        .max_connections(opts.max_connections)
        .acquire_timeout(opts.acquire_timeout)
        .connect_with(connect_options)
        .await
}

/// A pool for running migrations: `statement_timeout` is disabled (`0`), because a migration such
/// as an index build on a large table can legitimately run longer than the 30 s worker limit.
/// Use it for [`migrate`] and drop it afterwards; the worker and CLI then use [`connect`].
/// Caveat: the startup parameter is not accepted by pgbouncer in transaction mode, so point
/// migrations at Postgres directly (ADR 0033).
pub async fn connect_for_migrations(url: &str) -> Result<PgPool, sqlx::Error> {
    connect(
        url,
        DbOptions {
            max_connections: 1,
            statement_timeout: Duration::ZERO,
            ..DbOptions::default()
        },
    )
    .await
}

/// Applies every pending migration from `db/migrations/`. Pass a pool from
/// [`connect_for_migrations`], not a worker pool: the worker's 30 s `statement_timeout` would abort
/// a long migration halfway.
pub async fn migrate(pool: &PgPool) -> Result<(), sqlx::migrate::MigrateError> {
    crate::MIGRATOR.run(pool).await
}

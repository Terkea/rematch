//! Postgres access (sqlx, runtime-checked queries). The schema lives in `db/migrations/`.

pub mod accounts;
pub mod baseline;
pub mod games;
pub mod import_runs;
pub mod pool;

/// The SQL migrations in the repository's `db/migrations/`, embedded at compile time.
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../../../db/migrations");

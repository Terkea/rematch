//! Postgres access (sqlx, runtime-checked queries). The schema lives in `db/migrations/`.

pub mod accounts;
pub mod algorithm;
pub mod baseline;
pub mod games;
pub mod import_runs;
pub mod pool;
pub mod profile_rows;
pub mod stage_metrics;

/// The SQL migrations in the repository's `db/migrations/`, embedded at compile time.
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../../../db/migrations");

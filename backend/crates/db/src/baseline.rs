//! Adopting a database that Drizzle migrated before the sqlx migrations existed (ADR 0033).

use sqlx::PgPool;

/// The version of the baseline migration (the concatenated three Drizzle migrations).
pub const BASELINE_VERSION: i64 = 20261121000000;
/// How many migrations Drizzle had applied when the baseline was cut.
const DRIZZLE_MIGRATION_COUNT: i64 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdoptOutcome {
    /// sqlx already manages this database, or it is empty and `migrate` will create everything.
    NotNeeded,
    /// `_sqlx_migrations` now records the baseline as applied.
    Adopted,
}

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
    /// The database is in a state that is neither managed by sqlx nor a Drizzle baseline.
    #[error("cannot adopt the database: {0}")]
    UnexpectedState(String),
}

/// If `_sqlx_migrations` has no rows and `drizzle.__drizzle_migrations` has exactly three rows,
/// records the baseline as applied. Run it before [`crate::pool::migrate`].
pub async fn adopt_drizzle_baseline(pool: &PgPool) -> Result<AdoptOutcome, DbError> {
    let mut tx = pool.begin().await?;
    // Serialise concurrent starters (worker and CLI) on the same database.
    sqlx::query("SELECT pg_advisory_xact_lock(7262012)")
        .execute(&mut *tx)
        .await?;

    let has_sqlx_table: bool =
        sqlx::query_scalar("SELECT to_regclass('public._sqlx_migrations') IS NOT NULL")
            .fetch_one(&mut *tx)
            .await?;
    if has_sqlx_table {
        let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM _sqlx_migrations")
            .fetch_one(&mut *tx)
            .await?;
        if rows > 0 {
            return Ok(AdoptOutcome::NotNeeded);
        }
    }

    let has_drizzle: bool =
        sqlx::query_scalar("SELECT to_regclass('drizzle.__drizzle_migrations') IS NOT NULL")
            .fetch_one(&mut *tx)
            .await?;
    let has_games: bool = sqlx::query_scalar("SELECT to_regclass('public.games') IS NOT NULL")
        .fetch_one(&mut *tx)
        .await?;

    if !has_drizzle {
        if has_games {
            return Err(DbError::UnexpectedState(
                "table games exists but neither _sqlx_migrations nor drizzle.__drizzle_migrations \
                 has history"
                    .to_owned(),
            ));
        }
        return Ok(AdoptOutcome::NotNeeded); // fresh database
    }

    let drizzle_rows: i64 = sqlx::query_scalar("SELECT count(*) FROM drizzle.__drizzle_migrations")
        .fetch_one(&mut *tx)
        .await?;
    if drizzle_rows != DRIZZLE_MIGRATION_COUNT {
        return Err(DbError::UnexpectedState(format!(
            "drizzle.__drizzle_migrations has {drizzle_rows} rows, expected {DRIZZLE_MIGRATION_COUNT}"
        )));
    }
    if !has_games {
        return Err(DbError::UnexpectedState(
            "drizzle.__drizzle_migrations has 3 rows but table games is missing".to_owned(),
        ));
    }

    let baseline = crate::MIGRATOR
        .iter()
        .find(|m| m.version == BASELINE_VERSION)
        .ok_or_else(|| {
            DbError::UnexpectedState(format!(
                "baseline migration {BASELINE_VERSION} is not embedded"
            ))
        })?;

    // sqlx's own DDL for its bookkeeping table.
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS _sqlx_migrations (\
         version BIGINT PRIMARY KEY, \
         description TEXT NOT NULL, \
         installed_on TIMESTAMPTZ NOT NULL DEFAULT now(), \
         success BOOLEAN NOT NULL, \
         checksum BYTEA NOT NULL, \
         execution_time BIGINT NOT NULL)",
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time) \
         VALUES ($1, $2, TRUE, $3, 0)",
    )
    .bind(baseline.version)
    .bind(baseline.description.as_ref())
    .bind(baseline.checksum.as_ref())
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(AdoptOutcome::Adopted)
}

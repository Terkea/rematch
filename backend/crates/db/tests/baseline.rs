//! Adopting a database that Drizzle already migrated (ADR 0033).

use db::baseline::{AdoptOutcome, DbError, adopt_drizzle_baseline};
use db::pool::migrate;
use sqlx::PgPool;

const BASELINE_SQL: &str = include_str!("../../../../db/migrations/20261121000000_m1_baseline.sql");

async fn fake_drizzle_history(pool: &PgPool, rows: usize) {
    sqlx::raw_sql(
        "CREATE SCHEMA drizzle; \
         CREATE TABLE drizzle.__drizzle_migrations \
         (id serial PRIMARY KEY, hash text NOT NULL, created_at bigint);",
    )
    .execute(pool)
    .await
    .unwrap();
    for i in 0..rows {
        sqlx::query("INSERT INTO drizzle.__drizzle_migrations (hash, created_at) VALUES ($1, $2)")
            .bind(format!("hash{i}"))
            .bind(i as i64)
            .execute(pool)
            .await
            .unwrap();
    }
}

async fn applied_versions(pool: &PgPool) -> Vec<i64> {
    sqlx::query_scalar("SELECT version FROM _sqlx_migrations WHERE success ORDER BY version")
        .fetch_all(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrations = false)]
async fn adopts_a_drizzle_migrated_database(pool: PgPool) {
    fake_drizzle_history(&pool, 3).await;
    sqlx::raw_sql(BASELINE_SQL).execute(&pool).await.unwrap();

    assert_eq!(
        adopt_drizzle_baseline(&pool).await.unwrap(),
        AdoptOutcome::Adopted
    );
    assert_eq!(applied_versions(&pool).await, vec![20261121000000]);

    // The baseline is not re-run (it would fail on existing types); only later ones apply.
    migrate(&pool).await.unwrap();
    let expected: Vec<i64> = db::MIGRATOR.iter().map(|m| m.version).collect();
    assert_eq!(applied_versions(&pool).await, expected);

    // A second call is a no-op.
    assert_eq!(
        adopt_drizzle_baseline(&pool).await.unwrap(),
        AdoptOutcome::NotNeeded
    );
}

#[sqlx::test(migrations = false)]
async fn adopt_is_not_needed_after_sqlx_migrated_the_database(pool: PgPool) {
    migrate(&pool).await.unwrap();
    assert_eq!(
        adopt_drizzle_baseline(&pool).await.unwrap(),
        AdoptOutcome::NotNeeded
    );
}

#[sqlx::test(migrations = false)]
async fn adopt_is_not_needed_on_a_fresh_database(pool: PgPool) {
    assert_eq!(
        adopt_drizzle_baseline(&pool).await.unwrap(),
        AdoptOutcome::NotNeeded
    );
    migrate(&pool).await.unwrap();
}

#[sqlx::test(migrations = false)]
async fn adopt_names_an_unexpected_drizzle_history(pool: PgPool) {
    fake_drizzle_history(&pool, 2).await;
    sqlx::raw_sql(BASELINE_SQL).execute(&pool).await.unwrap();
    let err = adopt_drizzle_baseline(&pool).await.unwrap_err();
    assert!(matches!(err, DbError::UnexpectedState(_)), "{err:?}");
    assert!(err.to_string().contains("2 rows"), "{err}");
}

#[sqlx::test(migrations = false)]
async fn adopt_refuses_unmanaged_tables_without_history(pool: PgPool) {
    sqlx::raw_sql(BASELINE_SQL).execute(&pool).await.unwrap();
    let err = adopt_drizzle_baseline(&pool).await.unwrap_err();
    assert!(matches!(err, DbError::UnexpectedState(_)), "{err:?}");
}

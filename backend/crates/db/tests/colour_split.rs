//! The colour split migration (`20261123000000_colour_split.sql`): it applies on a database that
//! already holds profile rows, those rows become `side = 'both'`, and the new keys let one
//! account × time class hold a row per side.

use std::path::PathBuf;

use db::accounts::find_or_create_account;
use db::profile_rows::{
    KpiSnapshotRow, WeaknessEstimateRow, append_kpi_snapshot, upsert_weakness_estimate,
};
use sqlx::PgPool;
use uuid::Uuid;

const COLOUR_SPLIT: &str = "20261123000000_colour_split.sql";

fn migrations_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../db/migrations")
}

/// The migration files in version order (fixed-width timestamps sort lexically).
fn migration_files() -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(migrations_dir())
        .expect("read db/migrations")
        .map(|e| e.expect("dir entry").path())
        .filter(|p| p.extension().is_some_and(|x| x == "sql"))
        .collect();
    files.sort();
    files
}

async fn apply(pool: &PgPool, file: &PathBuf) {
    let sql = std::fs::read_to_string(file).expect("read migration");
    sqlx::raw_sql(sqlx::AssertSqlSafe(sql))
        .execute(pool)
        .await
        .unwrap_or_else(|e| panic!("apply {}: {e}", file.display()));
}

/// Applies every migration before the colour split.
async fn migrate_before_split(pool: &PgPool) -> PathBuf {
    let files = migration_files();
    let at = files
        .iter()
        .position(|f| f.file_name().is_some_and(|n| n == COLOUR_SPLIT))
        .expect("the colour split migration exists");
    for f in &files[..at] {
        apply(pool, f).await;
    }
    files[at].clone()
}

async fn seed_pre_split_rows(pool: &PgPool) -> (Uuid, Uuid) {
    let account = find_or_create_account(pool, "chesscom", "presplit")
        .await
        .unwrap()
        .id;
    sqlx::query(
        "INSERT INTO weakness_estimates (account_id, time_class, series_key, k, n, \
         n_unweighted, k_unweighted, alpha, beta, mean, ci_low, ci_high, confidence, priority, \
         state) VALUES ($1, 'rapid', 'W1', 1, 10, 10, 1, 2, 9, 0.18, 0.05, 0.4, 0.5, 0.1, \
         'active')",
    )
    .bind(account)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO kpi_snapshot (account_id, time_class, metric, window_kind, window_key, \
         value, n, protocol) VALUES ($1, 'rapid', 'blunders_per_game', 'last_n', 'last50', \
         0.5, 40, 'p')",
    )
    .bind(account)
    .execute(pool)
    .await
    .unwrap();
    let build: Uuid = sqlx::query_scalar(
        "INSERT INTO reference_builds (kind, status, source_month, source_sha256) \
         VALUES ('band_reference', 'active', '2026-10', 'c') RETURNING id",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO band_reference (build_id, band, time_class, metric, median, p25, p40, p75, \
         opp_slope_per_100, n_players, n_games, low_sample) \
         VALUES ($1, 'B2', 'rapid', 'weakness:W1', 0.02, 0.01, 0.015, 0.03, 0.001, 40, 400, \
         false)",
    )
    .bind(build)
    .execute(pool)
    .await
    .unwrap();
    (account, build)
}

fn estimate(account_id: Uuid, side: &str) -> WeaknessEstimateRow {
    WeaknessEstimateRow {
        account_id,
        time_class: "rapid".into(),
        side: side.into(),
        series_key: "W1".into(),
        k: 1.0,
        n: 10.0,
        n_unweighted: 10,
        k_unweighted: 1.0,
        alpha: 2.0,
        beta: 9.0,
        mean: 0.18,
        ci_low: 0.05,
        ci_high: 0.4,
        band_median: None,
        band_p40: None,
        confidence: 0.0,
        excess: None,
        priority: 0.0,
        percentile: None,
        state: "not_an_issue".into(),
        p_imp: None,
    }
}

#[sqlx::test(migrations = false)]
async fn applies_on_existing_rows_and_marks_them_both(pool: PgPool) {
    let split = migrate_before_split(&pool).await;
    let (account, build) = seed_pre_split_rows(&pool).await;
    apply(&pool, &split).await;

    for table in ["weakness_estimates", "kpi_snapshot", "band_reference"] {
        let sides: Vec<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "SELECT side::text FROM {table}"
        )))
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(sides, vec!["both".to_owned()], "{table}");
    }
    let labels: Vec<String> =
        sqlx::query_scalar("SELECT unnest(enum_range(NULL::side_scope))::text")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(labels, ["both", "white", "black"]);

    // The keys now include the side: one row per side, the both row untouched.
    let mut conn = pool.acquire().await.unwrap();
    for side in ["white", "black"] {
        upsert_weakness_estimate(&mut conn, &estimate(account, side))
            .await
            .unwrap();
    }
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT side::text, state::text FROM weakness_estimates WHERE account_id = $1 \
         ORDER BY side",
    )
    .bind(account)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        rows,
        [
            ("both".to_owned(), "active".to_owned()),
            ("white".to_owned(), "not_an_issue".to_owned()),
            ("black".to_owned(), "not_an_issue".to_owned()),
        ]
    );
    sqlx::query(
        "INSERT INTO band_reference (build_id, band, time_class, side, metric, median, p25, \
         p40, p75, n_players, n_games, low_sample) \
         VALUES ($1, 'B2', 'rapid', 'white', 'weakness:W1', 0.03, 0.01, 0.02, 0.04, 40, 400, \
         false)",
    )
    .bind(build)
    .execute(&pool)
    .await
    .expect("a white row beside the both row");
    let pk: String = sqlx::query_scalar(
        "SELECT pg_get_constraintdef(oid) FROM pg_constraint \
         WHERE conname = 'weakness_estimates_pkey'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(pk, "PRIMARY KEY (account_id, time_class, side, series_key)");
    let idx: String = sqlx::query_scalar(
        "SELECT indexdef FROM pg_indexes WHERE indexname = 'kpi_snapshot_lookup_idx'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        idx.contains("(account_id, time_class, side, metric, window_key, computed_at DESC)"),
        "{idx}"
    );
    let id = append_kpi_snapshot(
        &mut conn,
        &KpiSnapshotRow {
            user_id: None,
            account_id: account,
            time_class: "rapid".into(),
            side: "black".into(),
            metric: "blunders_per_game".into(),
            window_kind: "last_n".into(),
            window_key: "last50".into(),
            value: Some(0.7),
            value_adjusted: None,
            n: 12,
            ci_low: None,
            ci_high: None,
            protocol: "p".into(),
        },
    )
    .await
    .unwrap();
    let side: String = sqlx::query_scalar("SELECT side::text FROM kpi_snapshot WHERE id = $1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(side, "black");
}

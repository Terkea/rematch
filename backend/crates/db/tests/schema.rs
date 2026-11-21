//! Port of packages/db/test/schema.test.ts. `guard.test.ts` is dropped: `#[sqlx::test]` creates and
//! drops its own database per test, so there is nothing to truncate and no `_test` name to guard.
//! The `insertUserEvent`/`listEventsAfter` test moves to the events crate (Task 16).

use std::time::Duration;

use db::accounts::{ImportCursor, find_or_create_account, get_account};
use db::games::{AnalysisStatus, NewGame, get_game, upsert_game};
use db::import_runs::{ImportErrorCode, ImportRunStatus, list_runs};
use db::pool::{DbOptions, WORKER_STATEMENT_TIMEOUT, connect, connect_for_migrations};
use sqlx::PgPool;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use time::macros::datetime;
use uuid::Uuid;

async fn make_account(pool: &PgPool) -> Uuid {
    find_or_create_account(pool, "lichess", "Magnus")
        .await
        .unwrap()
        .id
}

fn game_row(account_id: Uuid, platform_game_id: &str) -> NewGame {
    NewGame {
        account_id,
        platform: "lichess".into(),
        platform_game_id: platform_game_id.into(),
        pgn: "1. e4 e5 *".into(),
        played_at: datetime!(2026-01-01 00:00:00 UTC),
        time_class: "blitz".into(),
        base_seconds: Some(180),
        increment_seconds: Some(2),
        rated: true,
        player_color: "white".into(),
        white_name: "Magnus".into(),
        black_name: "Hikaru".into(),
        white_rating: None,
        black_rating: None,
        result: "*".into(),
        termination: None,
        eco: None,
        opening_name: None,
        lichess_evals: None,
        ..Default::default()
    }
}

async fn count(pool: &PgPool, table: &str) -> i64 {
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT count(*) FROM {table}")))
        .fetch_one(pool)
        .await
        .unwrap()
}

/// The URL of the sqlx::test database, built from the master `DATABASE_URL`.
fn test_db_url(opts: &PgConnectOptions) -> String {
    let master = std::env::var("DATABASE_URL").expect("DATABASE_URL");
    let (base, _) = master
        .rsplit_once('/')
        .expect("database name in DATABASE_URL");
    format!(
        "{base}/{}",
        opts.get_database().expect("test database name")
    )
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn upsert_game_is_idempotent_on_platform_and_platform_game_id(pool: PgPool) {
    let acc = make_account(&pool).await;
    let first = upsert_game(&pool, &game_row(acc, "abc123")).await.unwrap();
    let second = upsert_game(&pool, &game_row(acc, "abc123")).await.unwrap();
    assert!(first.inserted);
    assert!(!second.inserted);
    assert_eq!(second.id, first.id);
    assert_eq!(count(&pool, "games").await, 1);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn upsert_game_returns_the_current_analysis_status_of_an_existing_row(pool: PgPool) {
    let acc = make_account(&pool).await;
    let first = upsert_game(&pool, &game_row(acc, "abc123")).await.unwrap();
    assert_eq!(first.analysis_status, AnalysisStatus::Pending);
    sqlx::query("UPDATE games SET analysis_status = 'done' WHERE id = $1")
        .bind(first.id)
        .execute(&pool)
        .await
        .unwrap();
    let again = upsert_game(&pool, &game_row(acc, "abc123")).await.unwrap();
    assert_eq!(again.analysis_status, AnalysisStatus::Done);
    assert!(!again.inserted);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn new_recovery_columns_default_sensibly_import_runs_and_user_events_exist(pool: PgPool) {
    let acc = make_account(&pool).await;
    assert!(
        get_account(&pool, acc)
            .await
            .unwrap()
            .unwrap()
            .import_cursor
            .is_none()
    );

    let id = upsert_game(&pool, &game_row(acc, "abc123"))
        .await
        .unwrap()
        .id;
    let g = get_game(&pool, id).await.unwrap().unwrap();
    assert_eq!(g.analysis_attempts, 0);
    assert!(g.heartbeat_at.is_none());
    assert!(g.analysis_started_at.is_none());

    sqlx::query("INSERT INTO import_runs (account_id) VALUES ($1)")
        .bind(acc)
        .execute(&pool)
        .await
        .unwrap();
    let runs = list_runs(&pool, acc).await.unwrap();
    assert_eq!(runs.len(), 1);
    let run = &runs[0];
    assert_eq!(run.status, ImportRunStatus::Running);
    assert_eq!(run.imported, 0);
    assert!(run.finished_at.is_none());
    assert!(run.last_error.is_none());

    let ids: Vec<i64> = sqlx::query_scalar(
        "INSERT INTO user_events (account_id, type, payload) \
         VALUES ($1, 'a', '{}'), ($1, 'b', '{\"n\":1}') RETURNING id",
    )
    .bind(acc)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(ids[1] > ids[0]);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn connect_sets_statement_timeout(_pool_opts: PgPoolOptions, opts: PgConnectOptions) {
    let url = test_db_url(&opts);

    // The worker default is 30 s on every connection.
    assert_eq!(WORKER_STATEMENT_TIMEOUT, Duration::from_secs(30));
    assert_eq!(
        DbOptions::default().statement_timeout,
        Duration::from_secs(30)
    );
    let worker = connect(&url, DbOptions::default()).await.unwrap();
    let shown: String = sqlx::query_scalar("SHOW statement_timeout")
        .fetch_one(&worker)
        .await
        .unwrap();
    assert_eq!(shown, "30s");

    // A short timeout really cancels a slow query.
    let limited = connect(
        &url,
        DbOptions {
            max_connections: 2,
            statement_timeout: Duration::from_millis(200),
            ..DbOptions::default()
        },
    )
    .await
    .unwrap();
    let err = sqlx::query("SELECT pg_sleep(5)")
        .execute(&limited)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("statement timeout"), "{err}");
    let shown: String = sqlx::query_scalar("SHOW statement_timeout")
        .fetch_one(&limited)
        .await
        .unwrap();
    assert_eq!(shown, "200ms");
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn games_get_pending_since_when_inserted(pool: PgPool) {
    let acc = make_account(&pool).await;
    let id = upsert_game(&pool, &game_row(acc, "abc123"))
        .await
        .unwrap()
        .id;
    let g = get_game(&pool, id).await.unwrap().unwrap();
    assert!(g.pending_since.is_some());

    sqlx::query(
        "INSERT INTO import_runs (account_id, status, error_code) \
         VALUES ($1, 'paused', $2)",
    )
    .bind(acc)
    .bind(ImportErrorCode::CircuitOpen.as_str())
    .execute(&pool)
    .await
    .unwrap();
    let run = list_runs(&pool, acc).await.unwrap().remove(0);
    assert_eq!(run.status, ImportRunStatus::Paused);
    assert_eq!(run.error_code.as_deref(), Some("circuit_open"));
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn defaults_analysis_status_to_pending(pool: PgPool) {
    let acc = make_account(&pool).await;
    let id = upsert_game(&pool, &game_row(acc, "abc123"))
        .await
        .unwrap()
        .id;
    let g = get_game(&pool, id).await.unwrap().unwrap();
    assert_eq!(g.analysis_status, AnalysisStatus::Pending);
    assert_eq!(g.analysis_generation, 0);
    assert_eq!(g.analysed_generation, None);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn cascades_moves_when_the_game_is_deleted(pool: PgPool) {
    let acc = make_account(&pool).await;
    let id = upsert_game(&pool, &game_row(acc, "abc123"))
        .await
        .unwrap()
        .id;
    sqlx::query(
        "INSERT INTO moves (game_id, ply, color, san, uci, fen_before, best_uci, win_before, \
         win_after, delta_w, accuracy, judgement, phase, clock_before, time_spent, decided) \
         VALUES ($1, 1, 'white', 'e4', 'e2e4', \
         'rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1', 'e2e4', 50, 52, 0, 100, \
         NULL, 'opening', 180, 1.5, false)",
    )
    .bind(id)
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(count(&pool, "moves").await, 1);
    sqlx::query("DELETE FROM games WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(count(&pool, "moves").await, 0);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn rejects_a_duplicate_fen_key_protocol_in_position_evals(pool: PgPool) {
    let insert = "INSERT INTO position_evals (fen_key, protocol, score_cp, pv, source) \
                  VALUES ('rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq -', \
                  'sf19-n300k-v1', 20, ARRAY['e2e4'], 'stockfish')";
    sqlx::query(insert).execute(&pool).await.unwrap();
    let err = sqlx::query(insert).execute(&pool).await.unwrap_err();
    let code = err
        .as_database_error()
        .and_then(|e| e.code().map(|c| c.to_string()));
    assert_eq!(code.as_deref(), Some("23505"));
}

// Extra coverage for the Rust-only accounts helpers.

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn find_or_create_account_lower_cases_the_username_and_keeps_the_display_name(pool: PgPool) {
    let a = find_or_create_account(&pool, "lichess", "MagnusCarlsen")
        .await
        .unwrap();
    assert_eq!(a.username, "magnuscarlsen");
    assert_eq!(a.display_name, "MagnusCarlsen");
    assert_eq!(a.platform, "lichess");
    let b = find_or_create_account(&pool, "lichess", "MAGNUSCARLSEN")
        .await
        .unwrap();
    assert_eq!(b.id, a.id);
    assert_eq!(b.display_name, "MagnusCarlsen");
    assert_eq!(count(&pool, "linked_accounts").await, 1);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn import_cursor_reads_the_camel_case_iso_json_that_ts_writes(pool: PgPool) {
    let acc = make_account(&pool).await;
    sqlx::query(
        "UPDATE linked_accounts SET import_cursor = \
         '{\"oldestReached\":\"2026-01-01T00:00:00.000Z\",\"newestSeenAt\":null}'::jsonb \
         WHERE id = $1",
    )
    .bind(acc)
    .execute(&pool)
    .await
    .unwrap();
    let cursor = get_account(&pool, acc)
        .await
        .unwrap()
        .unwrap()
        .import_cursor
        .unwrap();
    assert_eq!(
        cursor.oldest_reached,
        Some(datetime!(2026-01-01 00:00:00 UTC))
    );
    assert_eq!(cursor.newest_seen_at, None);
    let json = serde_json::to_value(&cursor).unwrap();
    assert_eq!(
        json,
        serde_json::json!({"oldestReached": "2026-01-01T00:00:00.000Z", "newestSeenAt": null})
    );
    let _: ImportCursor = serde_json::from_value(json).unwrap();
}

#[test]
fn import_error_codes_are_snake_case() {
    let all = [
        (ImportErrorCode::NotFound, "not_found"),
        (ImportErrorCode::RateLimited, "rate_limited"),
        (ImportErrorCode::Timeout, "timeout"),
        (ImportErrorCode::CircuitOpen, "circuit_open"),
        (ImportErrorCode::Shutdown, "shutdown"),
        (ImportErrorCode::Unknown, "unknown"),
    ];
    for (code, s) in all {
        assert_eq!(code.as_str(), s);
    }
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn connect_for_migrations_disables_the_statement_timeout(
    _pool_opts: PgPoolOptions,
    opts: PgConnectOptions,
) {
    let pool = connect_for_migrations(&test_db_url(&opts)).await.unwrap();
    let shown: String = sqlx::query_scalar("SHOW statement_timeout")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(shown, "0");
}

// Task 19 adds the generation columns. Until then `GameRow` carries placeholder constants.
#[sqlx::test(migrator = "db::MIGRATOR")]
#[ignore = "enable in Task 19: generation columns"]
async fn generation_round_trips(pool: PgPool) {
    let acc = make_account(&pool).await;
    let id = upsert_game(&pool, &game_row(acc, "gen1")).await.unwrap().id;
    sqlx::query("UPDATE games SET analysis_generation = 3, analysed_generation = 2 WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    let g = get_game(&pool, id).await.unwrap().unwrap();
    assert_eq!(g.analysis_generation, 3);
    assert_eq!(g.analysed_generation, Some(2));
}

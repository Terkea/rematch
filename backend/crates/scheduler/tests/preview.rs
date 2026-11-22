//! Preview pass (R2 Task 28, R-13): 30k nodes, MultiPV 1, no cache, own table, marked event.

use std::sync::Arc;

use queue::queue::Queue;
use queue::{Job, JobError, KIND_PREVIEW, PREVIEW_PRIORITY, PreviewPayload};
use scheduler::preview::{
    PREVIEW_NODES, PREVIEW_PROTOCOL, PreviewDeps, PreviewHandler, PreviewOutcome, enqueue_previews,
    handle_preview,
};
use serde_json::{Value, json};
use sqlx::PgPool;
use testkit::fake_engine::FakeAnalyser;
use testkit::{GameSeed, insert_account, insert_game};
use time::OffsetDateTime;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

fn job(game_id: Uuid) -> Job {
    let now = OffsetDateTime::now_utc();
    Job {
        id: 1,
        kind: KIND_PREVIEW.to_owned(),
        payload: json!({ "gameId": game_id }),
        dedupe_key: None,
        user_key: None,
        priority: PREVIEW_PRIORITY,
        attempts: 1,
        max_attempts: 2,
        lease_token: Uuid::new_v4(),
        run_at: now,
        created_at: now,
        started_at: now,
    }
}

fn deps(pool: &PgPool, fake: &Arc<FakeAnalyser>) -> PreviewDeps {
    PreviewDeps {
        pool: pool.clone(),
        analyser: fake.clone(),
        concurrency: 2,
    }
}

async fn run(d: &PreviewDeps, id: Uuid) -> Result<PreviewOutcome, JobError> {
    handle_preview(
        d,
        &job(id),
        PreviewPayload { game_id: id },
        CancellationToken::new(),
    )
    .await
}

async fn count(pool: &PgPool, table: &str) -> i64 {
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT count(*) FROM {table}")))
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn counts(pool: &PgPool) -> Vec<i64> {
    let mut v = Vec::new();
    for t in [
        "position_evals",
        "moves",
        "game_metrics",
        "weakness_series_point",
    ] {
        v.push(count(pool, t).await);
    }
    v
}

async fn setup(pool: &PgPool, status: &'static str) -> (Uuid, Uuid) {
    let acc = insert_account(pool, "lichess", "hero").await;
    let g = insert_game(
        pool,
        acc,
        GameSeed {
            status,
            ..GameSeed::default()
        },
    )
    .await;
    (acc, g)
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn preview_uses_30k_nodes_multipv_1(pool: PgPool) {
    let (_, g) = setup(&pool, "pending").await;
    let fake = Arc::new(FakeAnalyser::new());
    let out = run(&deps(&pool, &fake), g).await.unwrap();
    assert_eq!(out, PreviewOutcome::Written);
    let calls = fake.calls.lock().unwrap().clone();
    assert!(!calls.is_empty());
    assert!(calls.iter().all(|c| c.1 == 30_000 && c.2 == 1));
    assert_eq!(PREVIEW_NODES, 30_000);
    // Deduplicated by fen_key within the game.
    let mut fens: Vec<_> = calls.iter().map(|c| c.0.clone()).collect();
    fens.sort();
    fens.dedup();
    assert_eq!(fens.len(), calls.len());
    let (proto, moves): (String, Value) =
        sqlx::query_as("SELECT protocol, moves FROM game_previews WHERE game_id = $1")
            .bind(g)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(proto, PREVIEW_PROTOCOL);
    let first = &moves.as_array().unwrap()[0];
    for k in ["ply", "judgement", "deltaW", "accuracy"] {
        assert!(first.get(k).is_some(), "missing {k}");
    }
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn preview_never_writes_position_evals_or_game_metrics(pool: PgPool) {
    let (_, g) = setup(&pool, "pending").await;
    let before = counts(&pool).await;
    let fake = Arc::new(FakeAnalyser::new());
    run(&deps(&pool, &fake), g).await.unwrap();
    assert_eq!(counts(&pool).await, before);
    let status: String = sqlx::query_scalar("SELECT analysis_status::text FROM games WHERE id=$1")
        .bind(g)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(status, "pending");
    let m: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM job_metrics WHERE kind = 'preview-game' AND game_id = $1 \
         AND stages ?& ARRAY['parse_ms','engine_wall_ms','persist_ms']",
    )
    .bind(g)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(m, 1);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn preview_event_is_marked(pool: PgPool) {
    let (acc, g) = setup(&pool, "pending").await;
    let fake = Arc::new(FakeAnalyser::new());
    run(&deps(&pool, &fake), g).await.unwrap();
    let p: Value = sqlx::query_scalar(
        "SELECT payload FROM user_events WHERE account_id = $1 AND type = 'game.analysed'",
    )
    .bind(acc)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(p["preview"], json!(true));
    assert_eq!(p["gameId"], json!(g));
    assert!(p["accuracy"].is_number());
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn done_game_is_skipped(pool: PgPool) {
    let (acc, g) = setup(&pool, "done").await;
    let fake = Arc::new(FakeAnalyser::new());
    assert_eq!(
        run(&deps(&pool, &fake), g).await.unwrap(),
        PreviewOutcome::SkippedDone
    );
    assert_eq!(fake.call_count(), 0);
    assert_eq!(count(&pool, "game_previews").await, 0);
    let ev: i64 = sqlx::query_scalar("SELECT count(*) FROM user_events WHERE account_id = $1")
        .bind(acc)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(ev, 0);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn rerun_overwrites_one_row(pool: PgPool) {
    let (_, g) = setup(&pool, "pending").await;
    let fake = Arc::new(FakeAnalyser::new());
    run(&deps(&pool, &fake), g).await.unwrap();
    run(&deps(&pool, &fake), g).await.unwrap();
    assert_eq!(count(&pool, "game_previews").await, 1);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn enqueue_previews_dedupes(pool: PgPool) {
    let acc = insert_account(&pool, "lichess", "hero").await;
    let mut ids = Vec::new();
    for _ in 0..20 {
        ids.push(insert_game(&pool, acc, GameSeed::default()).await);
    }
    let q = Queue::new(pool.clone());
    assert_eq!(enqueue_previews(&q, acc, &ids).await.unwrap(), 20);
    assert_eq!(enqueue_previews(&q, acc, &ids).await.unwrap(), 0);
    assert_eq!(q.live_count(&[KIND_PREVIEW], Some(acc)).await.unwrap(), 20);
    let _ = PreviewHandler(Arc::new(deps(&pool, &Arc::new(FakeAnalyser::new()))));
}

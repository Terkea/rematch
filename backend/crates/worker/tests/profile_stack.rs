//! R2 Task 35: the profile engine wired into `start_workers` (ADR-R2-25): the enrichment hook on
//! the analyse runner, the fair claim for analyses, the R2 runners and the start-up maintenance
//! (requeue of unenriched games, the recurring difficulty sweeps).

mod common;

use std::collections::BTreeMap;
use std::time::Duration;

use common::{RUY, env_for, wait_until};
use queue::runner::RunnerConfig;
use queue::strategy::{PriorityFifo, SelectStrategy};
use scheduler::fair::FairRoundRobin;
use sqlx::PgPool;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use testkit::{GameSeed, insert_account, insert_game};
use worker::shutdown::{ShutdownOptions, shutdown};
use worker::start::{RunningWorkers, StartOptions, start_workers};

fn opts(pool_size: usize) -> StartOptions {
    StartOptions {
        pool_size: Some(pool_size),
        runner: Some(RunnerConfig::fast_for_tests()),
        with_reconciler: false,
        // The R2 runners alone; the AI runners (R3) are counted in `stack.rs`.
        ai: worker::ai_jobs::AiSetup::None,
        ..StartOptions::default()
    }
}

async fn stop(running: RunningWorkers) {
    let code = shutdown(
        running.into_shutdown_handles(),
        ShutdownOptions::with_grace(Duration::from_secs(5)),
    )
    .await;
    assert_eq!(code, 0);
}

/// `kind -> (concurrency, ORDER BY of its claim)`.
fn claims(running: &RunningWorkers) -> BTreeMap<&'static str, (usize, &'static str)> {
    running
        .claims
        .iter()
        .map(|c| (c.kind, (c.concurrency, c.strategy.order_by())))
        .collect()
}

async fn live_jobs(pool: &PgPool, kind: &str) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM jobs WHERE kind = $1 AND state IN ('queued', 'running', 'paused')",
    )
    .bind(kind)
    .fetch_one(pool)
    .await
    .unwrap()
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn start_workers_boots_without_syzygy_or_maia(pool_opts: PgPoolOptions, o: PgConnectOptions) {
    let Some(env) = env_for(&o) else {
        return;
    };
    assert_eq!(
        (env.syzygy_path.as_ref(), env.maia_url.as_ref()),
        (None, None)
    );
    let pool: PgPool = pool_opts.connect_with(o).await.unwrap();
    let running = start_workers(&env, opts(1)).await.expect("start_workers");
    assert!(running.tablebase.is_none());
    assert!(!running.maia_configured);
    let kinds: Vec<&str> = claims(&running).into_keys().collect();
    assert_eq!(
        kinds,
        vec![
            "analyse-game",
            "analyse-reference-game",
            "build-profile",
            "difficulty-sweep",
            "import-account",
            "preview-game",
            "reference-difficulty-sweep",
            "schedule-analysis",
        ]
    );
    assert_eq!(running.runners.len(), kinds.len());
    // The recurring sweeps are scheduled once (deduped).
    assert_eq!(live_jobs(&pool, "difficulty-sweep").await, 1);
    assert_eq!(live_jobs(&pool, "reference-difficulty-sweep").await, 1);
    stop(running).await;
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn start_workers_uses_fair_strategy_for_analyse(
    _pool_opts: PgPoolOptions,
    o: PgConnectOptions,
) {
    let Some(env) = env_for(&o) else {
        return;
    };
    // With the AI runners too (disabled client): they claim like every other non-analyse kind.
    let running = start_workers(
        &env,
        StartOptions {
            ai: worker::ai_jobs::AiSetup::With(None),
            ..opts(4)
        },
    )
    .await
    .expect("start_workers");
    let c = claims(&running);
    for kind in worker::ai_jobs::AI_RUNNER_KINDS {
        assert!(c.contains_key(kind), "{kind}");
    }
    let fair = FairRoundRobin::default().order_by();
    let fifo = PriorityFifo.order_by();
    assert_ne!(fair, fifo);
    assert_eq!(c["analyse-game"], (4, fair));
    // Every other kind keeps R1's claim (the fair round robin is analyse-only, ADR-R2-15).
    for (kind, (_, order)) in &c {
        if *kind != "analyse-game" {
            assert_eq!(*order, fifo, "{kind}");
        }
    }
    assert_eq!(c["preview-game"].0, 2, "max(1, pool / 2)");
    assert_eq!(c["schedule-analysis"].0, 2);
    assert_eq!(c["build-profile"].0, 2);
    assert_eq!(c["analyse-reference-game"].0, 1);
    assert_eq!(c["difficulty-sweep"].0, 1);
    assert_eq!(c["reference-difficulty-sweep"].0, 1);
    stop(running).await;
}

/// A `done` game without the current enrichment (analysed before R2, or by a worker whose hook
/// failed) is requeued once at start, and the hook enriches it: `enr-v1`, move features, metrics
/// and a profile build for its account.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn unenriched_games_are_requeued_at_start_and_enriched_by_the_hook(
    pool_opts: PgPoolOptions,
    o: PgConnectOptions,
) {
    let Some(env) = env_for(&o) else {
        return;
    };
    let pool: PgPool = pool_opts.connect_with(o).await.unwrap();
    let acct = insert_account(&pool, "chesscom", "prer2").await;
    let id = insert_game(
        &pool,
        acct,
        GameSeed {
            pgn: RUY.to_owned(),
            white: "prer2".to_owned(),
            status: "done",
            ..GameSeed::default()
        },
    )
    .await;
    let running = start_workers(&env, opts(1)).await.expect("start_workers");
    let protocol = wait_until(
        "the hook to enrich the game",
        Duration::from_secs(120),
        || {
            let pool = pool.clone();
            async move {
                let row: (String, Option<String>) = sqlx::query_as(
                    "SELECT analysis_status::text, enrichment_protocol FROM games WHERE id = $1",
                )
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap();
                (row.0 == "done" && row.1.is_some()).then_some(row.1)
            }
        },
    )
    .await;
    assert_eq!(protocol.as_deref(), Some("enr-v1"));
    let features: i64 = sqlx::query_scalar("SELECT count(*) FROM move_features WHERE game_id = $1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(features > 0, "move features written by the hook");
    let metrics: i64 = sqlx::query_scalar("SELECT count(*) FROM game_metrics WHERE game_id = $1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(metrics, 1);
    // `after_commit` enqueues it once the done transaction has committed.
    wait_until(
        "the hook to enqueue the profile build",
        Duration::from_secs(30),
        || {
            let pool = pool.clone();
            async move {
                let n: i64 = sqlx::query_scalar(
                    "SELECT count(*) FROM jobs WHERE kind = 'build-profile' AND user_key = $1",
                )
                .bind(acct)
                .fetch_one(&pool)
                .await
                .unwrap();
                (n == 1).then_some(())
            }
        },
    )
    .await;
    stop(running).await;
}

/// T21 hand-off I4: a verified `building` band-reference build whose jobs were lost (a crash of
/// `lichess-db build` between the row insert and the last job batch) gets them back at start.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn start_resumes_lost_reference_game_jobs(pool_opts: PgPoolOptions, o: PgConnectOptions) {
    use refdata::builds::{BuildKind, NewBuild, create_build, set_source_sha};
    use refdata::reference_games::{NewReferenceGame, insert_reference_games};
    let Some(env) = env_for(&o) else {
        return;
    };
    let pool: PgPool = pool_opts.connect_with(o).await.unwrap();
    let build = create_build(
        &pool,
        &NewBuild {
            kind: BuildKind::BandReference,
            source_month: "2026-10".to_owned(),
            source_url: None,
            params: serde_json::json!({}),
            protocol: None,
            enrichment_protocol: None,
            difficulty_model: None,
        },
    )
    .await
    .unwrap();
    set_source_sha(&pool, build, &"ab".repeat(32), 2)
        .await
        .unwrap();
    let games: Vec<NewReferenceGame> = (0..2)
        .map(|i| NewReferenceGame {
            lichess_game_id: format!("lost-{i}"),
            pgn: format!("[Event \"Rated Blitz game\"]\n[Result \"1/2-1/2\"]\n\n{RUY} 1/2-1/2\n"),
            time_class: chess_core::TimeClass::Blitz,
            base_seconds: Some(180),
            increment_seconds: Some(0),
            white_rating: Some(1500),
            black_rating: Some(1550),
            white_hash: format!("{:064x}", 2 * i + 1),
            black_hash: format!("{:064x}", 2 * i + 2),
            white_selected: true,
            black_selected: true,
            result: "1/2-1/2".to_owned(),
            termination: Some("Normal".to_owned()),
            played_at: time::OffsetDateTime::UNIX_EPOCH,
        })
        .collect();
    insert_reference_games(&pool, build, &games).await.unwrap();
    assert_eq!(
        live_jobs(&pool, "analyse-reference-game").await,
        0,
        "the jobs were lost"
    );
    let running = start_workers(&env, opts(1)).await.expect("start_workers");
    let jobs: i64 = sqlx::query_scalar(
        "SELECT count(DISTINCT dedupe_key) FROM jobs WHERE kind = 'analyse-reference-game'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(jobs, 2, "one job per pending reference game");
    stop(running).await;
}

/// The benchmark knob: without the hook, the analysis is R1's alone (no enrichment rows).
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn without_the_profile_hook_analysis_writes_no_enrichment(
    pool_opts: PgPoolOptions,
    o: PgConnectOptions,
) {
    let Some(env) = env_for(&o) else {
        return;
    };
    let pool: PgPool = pool_opts.connect_with(o).await.unwrap();
    let (acct, id) = common::add_game(&pool, RUY, "nohook").await;
    let running = start_workers(
        &env,
        StartOptions {
            profile_hook: false,
            ..opts(1)
        },
    )
    .await
    .expect("start_workers");
    assert!(
        analysis::jobs::analyse::enqueue_analyse(&running.queue, id, acct)
            .await
            .unwrap()
    );
    wait_until("the analysis to finish", Duration::from_secs(120), || {
        let pool = pool.clone();
        async move {
            let s: String =
                sqlx::query_scalar("SELECT analysis_status::text FROM games WHERE id = $1")
                    .bind(id)
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            (s == "done").then_some(())
        }
    })
    .await;
    let (protocol, features): (Option<String>, i64) = sqlx::query_as(
        "SELECT g.enrichment_protocol, (SELECT count(*) FROM move_features WHERE game_id = g.id) \
         FROM games g WHERE g.id = $1",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!((protocol, features), (None, 0));
    stop(running).await;
}

//! Port of apps/worker/test/stack.test.ts (the Redis case is gone with Redis), plus the boot-time
//! Stockfish 19 check (T7 hand-off).

mod common;

use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use common::env_for;
use queue::runner::RunnerConfig;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use worker::config::WorkerEnv;
use worker::shutdown::{ShutdownOptions, shutdown};
use worker::start::{StartError, StartOptions, start_workers};

#[tokio::test]
async fn reaches_postgres_at_database_url() {
    let Ok(url) = std::env::var("DATABASE_URL") else {
        assert!(
            !testkit::in_docker(),
            "DATABASE_URL must be set when IN_DOCKER=1"
        );
        return;
    };
    let pool = PgPoolOptions::new()
        .acquire_timeout(Duration::from_secs(5))
        .connect(&url)
        .await
        .expect("postgres at DATABASE_URL");
    let one: i32 = sqlx::query_scalar("SELECT 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(one, 1);
    pool.close().await;
}

/// A throwaway executable script, written by a child `sh` (see engine's tests: ETXTBSY).
fn fake_engine(name: &str, banner: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ci-worker-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("stockfish");
    let script = format!("#!/bin/sh\necho '{banner}'\nwhile read line; do :; done\n");
    let status = Command::new("sh")
        .arg("-c")
        .arg("printf '%s' \"$1\" > \"$2\" && chmod +x \"$2\"")
        .arg("sh")
        .arg(script)
        .arg(&path)
        .status()
        .unwrap();
    assert!(status.success());
    path
}

#[cfg(unix)]
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn refuses_to_start_unless_the_engine_is_stockfish_19(
    _pool_opts: PgPoolOptions,
    opts: PgConnectOptions,
) {
    let env = WorkerEnv {
        pool_size: Some(1),
        ..WorkerEnv::new(
            common::test_db_url(&opts),
            fake_engine("sf18", "Stockfish 18 by the Stockfish developers"),
        )
    };
    let err = match start_workers(&env, StartOptions::default()).await {
        Ok(_) => panic!("started with Stockfish 18"),
        Err(e) => e,
    };
    match err.downcast_ref::<StartError>() {
        Some(StartError::WrongStockfish { found }) => assert!(found.contains("Stockfish 18")),
        other => panic!("expected WrongStockfish, got {other:?} ({err:#})"),
    }
    assert!(err.to_string().contains("Stockfish 19"), "{err}");
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn starts_with_stockfish_19_and_shuts_down_cleanly(
    _pool_opts: PgPoolOptions,
    opts: PgConnectOptions,
) {
    let Some(env) = env_for(&opts) else {
        return;
    };
    let running = start_workers(
        &env,
        StartOptions {
            pool_size: Some(1),
            runner: Some(RunnerConfig::fast_for_tests()),
            with_reconciler: true,
            ..StartOptions::default()
        },
    )
    .await
    .expect("start_workers");
    assert!(running.stockfish_version.starts_with("Stockfish 19"));
    assert_eq!(running.engines.pids().len(), 1);
    // 18 runners, every one registered through `Spawned::spawn` (`claims` lists them in order):
    // - R1: import-account, analyse-game, schedule-analysis, preview-game (4);
    // - R2 Task 35: build-profile, analyse-reference-game, difficulty-sweep,
    //   reference-difficulty-sweep (4);
    // - R3 Task 26: generate-report, weekly-review, explain-weakness-change, coach-chat,
    //   interactive-analyse, report-batch-schedule, weekly-batch-schedule, ai-batch-poll,
    //   ai-reconcile (9);
    // - reconcile (with_reconciler: 1).
    assert_eq!(
        running.runners.len(),
        4 + 4 + worker::ai_jobs::AI_RUNNER_KINDS.len() + 1
    );
    assert_eq!(running.runners.len(), 18);
    assert_eq!(running.claims.len(), running.runners.len());
    let claimed: Vec<&str> = running.claims.iter().map(|c| c.kind).collect();
    assert_eq!(claimed, running.runner_kinds());
    let recurring: Vec<String> = sqlx::query_scalar(
        "SELECT kind FROM jobs WHERE recur_every IS NOT NULL AND state <> 'done' ORDER BY kind",
    )
    .fetch_all(&running.pool)
    .await
    .unwrap();
    assert_eq!(
        recurring,
        [
            "ai-batch-poll",
            "ai-reconcile",
            "difficulty-sweep",
            "reconcile",
            "reference-difficulty-sweep",
            "report-batch-schedule",
            "weekly-batch-schedule"
        ]
    );
    let code = shutdown(
        running.into_shutdown_handles(),
        ShutdownOptions::with_grace(Duration::from_secs(5)),
    )
    .await;
    assert_eq!(code, 0);
}

/// A real SIGTERM reaches the channel from `signals()`, even when it arrives before anyone reads
/// it (the worker registers the handlers before its startup, as PID 1 in the container).
#[cfg(unix)]
#[tokio::test]
async fn a_sigterm_before_the_handles_exist_is_kept_for_the_shutdown() {
    let mut rx = worker::shutdown::signals();
    // The shell's builtin `kill` (the image has no procps).
    let status = Command::new("sh")
        .args([
            "-c",
            "kill -TERM \"$1\"",
            "sh",
            &std::process::id().to_string(),
        ])
        .status()
        .unwrap();
    assert!(status.success());
    tokio::time::sleep(Duration::from_millis(200)).await;
    let name = tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("the signal is forwarded")
        .unwrap();
    assert_eq!(name, "SIGTERM");
}

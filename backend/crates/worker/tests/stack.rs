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
        database_url: common::test_db_url(&opts),
        stockfish_path: fake_engine("sf18", "Stockfish 18 by the Stockfish developers"),
        lichess_token: None,
        lichess_lookback_days: None,
        pool_size: Some(1),
        syzygy_path: None,
        syzygy_stage_dir: None,
        data_dir: None,
        maia_url: None,
        plan_default: scheduler::select::Plan::Beta,
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
            profile_hook: true,
        },
    )
    .await
    .expect("start_workers");
    assert!(running.stockfish_version.starts_with("Stockfish 19"));
    assert_eq!(running.engines.pids().len(), 1);
    // Import, analyse, schedule-analysis, preview, build-profile, analyse-reference-game,
    // difficulty-sweep, reference-difficulty-sweep (R2 Task 35) and reconcile.
    assert_eq!(running.runners.len(), 9);
    let reconcile: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM jobs WHERE kind = 'reconcile' AND state <> 'done'",
    )
    .fetch_one(&running.pool)
    .await
    .unwrap();
    assert_eq!(reconcile, 1);
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

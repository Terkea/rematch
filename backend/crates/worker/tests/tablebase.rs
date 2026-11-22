//! R2 Task 23: `SYZYGY_PATH` reaches the worker's tablebase probe (ADR-R2-12).
//!
//! The real-table test skips only when `SYZYGY_PATH` is unset; under `IN_DOCKER=1` a set path
//! without `KQvK.rtbw` fails it. A missing tables directory must leave the probe "unavailable"
//! and never block analysis.

mod common;

use std::path::PathBuf;
use std::time::Duration;

use common::{RUY, add_game, env_for, game, wait_until};
use db::games::AnalysisStatus;
use enrich::tablebase::pool::TablebaseStatus;
use queue::runner::RunnerConfig;
use sqlx::PgPool;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use worker::shutdown::{ShutdownOptions, shutdown};
use worker::start::{StartOptions, start_workers};

// Lichess API: KQvK win, dtz 13 (see the enrich tablebase tests).
const KQK_WIN: &str = "8/8/8/8/8/4k3/8/KQ6 w - - 0 1";

fn opts() -> StartOptions {
    StartOptions {
        pool_size: Some(1),
        runner: Some(RunnerConfig::fast_for_tests()),
        with_reconciler: false,
    }
}

fn real_tables() -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var_os("SYZYGY_PATH").filter(|v| !v.is_empty())?);
    if !dir.join("KQvK.rtbw").is_file() {
        assert!(
            !testkit::in_docker(),
            "SYZYGY_PATH={} has no KQvK.rtbw (IN_DOCKER=1)",
            dir.display()
        );
        return None;
    }
    Some(dir)
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn syzygy_path_reaches_the_tablebase_probe(
    pool_opts: PgPoolOptions,
    opts_: PgConnectOptions,
) {
    let Some(mut env) = env_for(&opts_) else {
        return;
    };
    let Some(tables) = real_tables() else {
        eprintln!("skipped: SYZYGY_PATH unset");
        return;
    };
    env.syzygy_path = Some(tables);
    let pool: PgPool = pool_opts.connect_with(opts_).await.unwrap();
    let running = start_workers(&env, opts()).await.expect("start_workers");
    let tb = running.tablebase.as_ref().expect("a tablebase probe");
    assert_eq!(tb.status(), TablebaseStatus::Available { tables: 145 });
    assert_eq!(tb.probe.probe(KQK_WIN).await, Some(1));
    assert_eq!(tb.pool.probe_dtz(KQK_WIN).await, Some(13));
    // The pipeline's probe is the cached one: the answer is in position_tb.
    let wdl: Option<i16> = sqlx::query_scalar(
        "SELECT wdl FROM position_tb WHERE fen_key = '8/8/8/8/8/4k3/8/KQ6 w - -'",
    )
    .fetch_optional(&pool)
    .await
    .unwrap();
    assert_eq!(wdl, Some(1));
    let code = shutdown(
        running.into_shutdown_handles(),
        ShutdownOptions::with_grace(Duration::from_secs(5)),
    )
    .await;
    assert_eq!(code, 0);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn missing_tables_dir_is_unavailable_and_never_blocks_analysis(
    pool_opts: PgPoolOptions,
    opts_: PgConnectOptions,
) {
    let Some(mut env) = env_for(&opts_) else {
        return;
    };
    env.syzygy_path = Some(PathBuf::from("/nonexistent/syzygy"));
    let pool: PgPool = pool_opts.connect_with(opts_).await.unwrap();
    let (acct, id) = add_game(&pool, RUY, "notables").await;
    let running = start_workers(&env, opts()).await.expect("start_workers");
    let tb = running.tablebase.as_ref().expect("a tablebase probe");
    assert_eq!(tb.status(), TablebaseStatus::Unavailable);
    assert_eq!(tb.status().as_str(), "unavailable");
    assert_eq!(tb.probe.probe(KQK_WIN).await, None);
    assert!(
        analysis::jobs::analyse::enqueue_analyse(&running.queue, id, acct)
            .await
            .unwrap()
    );
    wait_until("the analysis to finish", Duration::from_secs(120), || {
        let pool = pool.clone();
        async move { (game(&pool, id).await.analysis_status == AnalysisStatus::Done).then_some(()) }
    })
    .await;
    let code = shutdown(
        running.into_shutdown_handles(),
        ShutdownOptions::with_grace(Duration::from_secs(5)),
    )
    .await;
    assert_eq!(code, 0);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn no_syzygy_path_means_no_probe(_pool_opts: PgPoolOptions, opts_: PgConnectOptions) {
    let Some(env) = env_for(&opts_) else {
        return;
    };
    assert_eq!(env.syzygy_path, None);
    let running = start_workers(&env, opts()).await.expect("start_workers");
    assert!(running.tablebase.is_none());
    let code = shutdown(
        running.into_shutdown_handles(),
        ShutdownOptions::with_grace(Duration::from_secs(5)),
    )
    .await;
    assert_eq!(code, 0);
}

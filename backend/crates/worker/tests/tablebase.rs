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
        profile_hook: true,
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

// ---------------------------------------------------------------------------------------------
// R2 Task 35: staging the tables on a local disk (ADR-R2-25). Every `ucinewgame` makes Stockfish
// re-open the whole table set; on a slow mount (Docker Desktop's Windows bind mount) that made
// each probe cost about 0.5 s idle and 9.5 s under load.

fn temp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("ci-stage-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn staging_copies_only_table_files_and_skips_current_ones() {
    let src = temp_dir("src");
    let dst = temp_dir("dst").join("syzygy");
    std::fs::write(src.join("KQvK.rtbw"), b"wdl-bytes").unwrap();
    std::fs::write(src.join("KQvK.rtbz"), b"dtz-bytes!").unwrap();
    std::fs::write(src.join("README.txt"), b"not a table").unwrap();
    let first = worker::syzygy_stage::stage_tables(&src, &dst).unwrap();
    assert_eq!((first.copied, first.current), (2, 0));
    assert_eq!(std::fs::read(dst.join("KQvK.rtbw")).unwrap(), b"wdl-bytes");
    assert!(!dst.join("README.txt").exists());
    // A second start copies nothing.
    let again = worker::syzygy_stage::stage_tables(&src, &dst).unwrap();
    assert_eq!((again.copied, again.current), (0, 2));
    // A truncated (or changed) copy is replaced, never trusted.
    std::fs::write(dst.join("KQvK.rtbz"), b"dtz").unwrap();
    let fixed = worker::syzygy_stage::stage_tables(&src, &dst).unwrap();
    assert_eq!((fixed.copied, fixed.current), (1, 1));
    assert_eq!(std::fs::read(dst.join("KQvK.rtbz")).unwrap(), b"dtz-bytes!");
    // No temporary file is left behind.
    let names: Vec<String> = std::fs::read_dir(&dst)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(names.len(), 2, "{names:?}");
}

#[test]
fn staging_a_missing_source_is_an_error() {
    let dst = temp_dir("dst-missing").join("syzygy");
    assert!(
        worker::syzygy_stage::stage_tables(std::path::Path::new("/nonexistent/syzygy"), &dst)
            .is_err()
    );
}

/// With `SYZYGY_STAGE_DIR`, the probe reads the staged copy and answers exactly as from the
/// source (same WDL, same `position_tb` row).
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn staged_tables_answer_like_the_source(pool_opts: PgPoolOptions, opts_: PgConnectOptions) {
    let Some(mut env) = env_for(&opts_) else {
        return;
    };
    let Some(tables) = real_tables() else {
        eprintln!("skipped: SYZYGY_PATH unset");
        return;
    };
    let stage = temp_dir("real").join("syzygy");
    env.syzygy_path = Some(tables);
    env.syzygy_stage_dir = Some(stage.clone());
    let pool: PgPool = pool_opts.connect_with(opts_).await.unwrap();
    let running = start_workers(&env, opts()).await.expect("start_workers");
    let tb = running.tablebase.as_ref().expect("a tablebase probe");
    assert_eq!(tb.status(), TablebaseStatus::Available { tables: 145 });
    assert_eq!(tb.path, stage, "the pool reads the staged copy");
    assert_eq!(tb.probe.probe(KQK_WIN).await, Some(1));
    assert_eq!(tb.pool.probe_dtz(KQK_WIN).await, Some(13));
    let staged = std::fs::read_dir(&stage).unwrap().count();
    assert_eq!(staged, 290, "every .rtbw and .rtbz file");
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
    let _ = std::fs::remove_dir_all(stage);
}

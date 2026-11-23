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
        ..StartOptions::default()
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
    // This test's share of the Postgres connections (its pool is a child of sqlx::test's).
    let _budget = common::reserve_connections(1, 0).await;
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
    // This test's share of the Postgres connections (its pool is a child of sqlx::test's).
    let _budget = common::reserve_connections(1, 0).await;
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
    // This test's share of the Postgres connections (its pool is a child of sqlx::test's).
    let _budget = common::reserve_connections(1, 0).await;
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

use lichess_db::syzygy::Manifest;
use worker::syzygy_stage::{stage_tables, stage_tables_with};

/// Table names in `dir` (the lock and the stage manifest are not tables).
fn table_names(dir: &std::path::Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|n| !n.starts_with('.'))
        .collect();
    names.sort();
    names
}

/// No staging leftovers (temporary copies) in `dir`.
fn partials(dir: &std::path::Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|n| n.ends_with(".partial"))
        .collect()
}

/// Deterministic pseudo-random bytes (xorshift), so a copy cannot be confused with zeros.
fn noise(len: usize, seed: u64) -> Vec<u8> {
    let mut x = seed | 1;
    (0..len)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            (x >> 24) as u8
        })
        .collect()
}

#[test]
fn staging_copies_only_table_files_and_skips_current_ones() {
    let src = temp_dir("src");
    let dst = temp_dir("dst").join("syzygy");
    std::fs::write(src.join("KQvK.rtbw"), b"wdl-bytes").unwrap();
    std::fs::write(src.join("KQvK.rtbz"), b"dtz-bytes!").unwrap();
    std::fs::write(src.join("README.txt"), b"not a table").unwrap();
    let none = Manifest::default();
    let first = stage_tables_with(&src, &dst, &none).unwrap();
    assert_eq!((first.copied, first.current), (2, 0));
    assert_eq!(std::fs::read(dst.join("KQvK.rtbw")).unwrap(), b"wdl-bytes");
    assert_eq!(table_names(&dst), ["KQvK.rtbw", "KQvK.rtbz"]);
    // A second start copies nothing.
    let again = stage_tables_with(&src, &dst, &none).unwrap();
    assert_eq!((again.copied, again.current), (0, 2));
    // A truncated copy is replaced.
    std::fs::write(dst.join("KQvK.rtbz"), b"dtz").unwrap();
    let fixed = stage_tables_with(&src, &dst, &none).unwrap();
    assert_eq!((fixed.copied, fixed.current), (1, 1));
    assert_eq!(std::fs::read(dst.join("KQvK.rtbz")).unwrap(), b"dtz-bytes!");
    // A table removed from the source is removed from the stage.
    std::fs::remove_file(src.join("KQvK.rtbz")).unwrap();
    let pruned = stage_tables_with(&src, &dst, &none).unwrap();
    assert_eq!((pruned.copied, pruned.current, pruned.removed), (0, 1, 1));
    assert_eq!(table_names(&dst), ["KQvK.rtbw"]);
    assert!(partials(&dst).is_empty());
}

/// Review I2: a staged copy of the right size but the wrong bytes (a crash between rename and
/// write-back, a disk error, a changed source) is never trusted.
#[test]
fn a_same_size_corrupt_copy_is_restaged() {
    let src = temp_dir("src-corrupt");
    let dst = temp_dir("dst-corrupt").join("syzygy");
    let body = noise(4096, 7);
    std::fs::write(src.join("KRvK.rtbw"), &body).unwrap();
    let none = Manifest::default();
    stage_tables_with(&src, &dst, &none).unwrap();
    // Overwritten in place with zeros of the same size.
    std::fs::write(dst.join("KRvK.rtbw"), vec![0u8; body.len()]).unwrap();
    let r = stage_tables_with(&src, &dst, &none).unwrap();
    assert_eq!((r.copied, r.current), (1, 0), "{r:?}");
    assert_eq!(std::fs::read(dst.join("KRvK.rtbw")).unwrap(), body);
    // The crash case: zeros under the table's name with no stage manifest at all (the rename
    // persisted, the data and the manifest did not). The file is hashed, not trusted.
    let f = std::fs::OpenOptions::new()
        .write(true)
        .open(dst.join("KRvK.rtbw"))
        .unwrap();
    let mtime = f.metadata().unwrap().modified().unwrap();
    drop(f);
    std::fs::write(dst.join("KRvK.rtbw"), vec![0u8; body.len()]).unwrap();
    std::fs::File::options()
        .write(true)
        .open(dst.join("KRvK.rtbw"))
        .unwrap()
        .set_modified(mtime)
        .unwrap();
    std::fs::remove_file(dst.join(".staged.json")).unwrap();
    let r = stage_tables_with(&src, &dst, &none).unwrap();
    assert_eq!((r.copied, r.current), (1, 0), "{r:?}");
    assert_eq!(std::fs::read(dst.join("KRvK.rtbw")).unwrap(), body);
}

/// A source table whose hash differs from the pinned manifest is refused: it is not staged under
/// its name, and the caller falls back to the source (whose engine then reads what it reads).
#[test]
fn a_source_table_that_fails_the_pinned_hash_is_refused() {
    let src = temp_dir("src-pinned");
    let dst = temp_dir("dst-pinned").join("syzygy");
    std::fs::write(src.join("KQvK.rtbw"), b"not the pinned bytes").unwrap();
    // The real pinned manifest names KQvK.rtbw with another SHA-256.
    let err = stage_tables(&src, &dst).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData, "{err}");
    assert!(table_names(&dst).is_empty());
    assert!(partials(&dst).is_empty());
}

/// Review I1 (the reviewer's probe P2): two processes staging the same volume at once. The
/// second must wait for the first, and the file under the table's name must equal the source
/// the moment the first returns.
#[test]
fn two_concurrent_stagers_never_expose_a_partial_table() {
    let src = temp_dir("src-race");
    let dst = temp_dir("dst-race").join("syzygy");
    let body = noise(192 * 1024 * 1024, 11);
    std::fs::write(src.join("KBNvK.rtbw"), &body).unwrap();
    let none = Manifest::default();
    for round in 0..3 {
        let _ = std::fs::remove_dir_all(&dst);
        let (s1, d1, m1) = (src.clone(), dst.clone(), none.clone());
        let (s2, d2, m2) = (src.clone(), dst.clone(), none.clone());
        let first = std::thread::spawn(move || {
            let r = stage_tables_with(&s1, &d1, &m1);
            // What a worker's engines would read right after its staging returned.
            let seen = std::fs::read(d1.join("KBNvK.rtbw")).ok();
            (r, seen)
        });
        std::thread::sleep(Duration::from_millis(30));
        let second = std::thread::spawn(move || stage_tables_with(&s2, &d2, &m2));
        let (r1, seen) = first.join().unwrap();
        let r2 = second.join().unwrap();
        let r1 = r1.expect("the first stager succeeds");
        let r2 = r2.expect("the second stager succeeds too");
        assert!(
            seen.as_deref() == Some(&body[..]),
            "round {round}: a partial table was exposed"
        );
        assert_eq!(
            r1.copied + r2.copied,
            1,
            "round {round}: copied once ({r1:?}, {r2:?})"
        );
        assert_eq!(std::fs::read(dst.join("KBNvK.rtbw")).unwrap(), body);
        assert!(partials(&dst).is_empty(), "round {round}");
    }
    let _ = std::fs::remove_dir_all(&src);
    let _ = std::fs::remove_dir_all(&dst);
}

/// A leftover temporary copy (a crashed or out-of-space stager) is removed by the next pass.
#[test]
fn leftover_partials_are_removed() {
    let src = temp_dir("src-left");
    let dst = temp_dir("dst-left").join("syzygy");
    std::fs::write(src.join("KPvK.rtbw"), b"pawn").unwrap();
    std::fs::create_dir_all(&dst).unwrap();
    std::fs::write(dst.join(".KPvK.rtbw.1234.dead.partial"), b"pa").unwrap();
    stage_tables_with(&src, &dst, &Manifest::default()).unwrap();
    assert!(partials(&dst).is_empty());
    assert_eq!(table_names(&dst), ["KPvK.rtbw"]);
}

#[test]
fn staging_a_missing_source_is_an_error() {
    let dst = temp_dir("dst-missing").join("syzygy");
    assert!(stage_tables(std::path::Path::new("/nonexistent/syzygy"), &dst).is_err());
}

/// With `SYZYGY_STAGE_DIR`, the probe reads the staged copy and answers exactly as from the
/// source (same WDL, same `position_tb` row).
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn staged_tables_answer_like_the_source(pool_opts: PgPoolOptions, opts_: PgConnectOptions) {
    let Some(mut env) = env_for(&opts_) else {
        return;
    };
    // This test's share of the Postgres connections (its pool is a child of sqlx::test's).
    let _budget = common::reserve_connections(1, 0).await;
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
    assert_eq!(table_names(&stage).len(), 290, "every .rtbw and .rtbz file");
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

/// Belt and braces: if a staged table passes the stage's checks but answers wrongly (here a
/// zero-filled KQvK.rtbw with its recorded size and mtime kept), the start-up canary probe catches
/// it before any answer is cached, and the worker reads the read-only source instead.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_staged_table_that_answers_wrongly_falls_back_to_the_source(
    pool_opts: PgPoolOptions,
    opts_: PgConnectOptions,
) {
    let Some(mut env) = env_for(&opts_) else {
        return;
    };
    // This test's share of the Postgres connections (its pool is a child of sqlx::test's).
    let _budget = common::reserve_connections(1, 0).await;
    let Some(tables) = real_tables() else {
        eprintln!("skipped: SYZYGY_PATH unset");
        return;
    };
    let stage = temp_dir("canary").join("syzygy");
    stage_tables(&tables, &stage).expect("staged");
    let target = stage.join("KQvK.rtbw");
    let len = std::fs::metadata(&target).unwrap().len();
    let mtime = std::fs::metadata(&target).unwrap().modified().unwrap();
    std::fs::write(&target, vec![0u8; usize::try_from(len).unwrap()]).unwrap();
    std::fs::File::options()
        .write(true)
        .open(&target)
        .unwrap()
        .set_modified(mtime)
        .unwrap();
    env.syzygy_path = Some(tables.clone());
    env.syzygy_stage_dir = Some(stage.clone());
    let pool: PgPool = pool_opts.connect_with(opts_).await.unwrap();
    let running = start_workers(&env, opts()).await.expect("start_workers");
    let tb = running.tablebase.as_ref().expect("a tablebase probe");
    assert_eq!(
        tb.path, tables,
        "the worker reads the source after a failed canary"
    );
    assert_eq!(tb.probe.probe(KQK_WIN).await, Some(1));
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM position_tb")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 1, "only the source's answer is cached");
    let code = shutdown(
        running.into_shutdown_handles(),
        ShutdownOptions::with_grace(Duration::from_secs(5)),
    )
    .await;
    assert_eq!(code, 0);
    let _ = std::fs::remove_dir_all(stage);
}

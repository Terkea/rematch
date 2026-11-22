//! R2 Task 23: local Syzygy probe pool (ADR-R2-12).
//!
//! The fake-engine tests need neither Stockfish nor the real tables. The real-table tests read
//! `SYZYGY_PATH`: they skip only when it is unset, and fail under `IN_DOCKER=1` when it is set but
//! `KPvK.rtbw`/`KQvK.rtbw` are missing. Expected values were checked once against the Lichess
//! tablebase API (task 23 report, Step 1).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use chess_core::EngineScore;
use engine::pool::{EnginePool, PoolOptions};
use engine::types::{AnalyseOptions, AnalysisLine};
use engine::uci::{StartOptions, UciEngine};
use enrich::tablebase::cached::CachedTbProbe;
use enrich::tablebase::pool::{SyzygyPool, TablebaseStatus};
use enrich::tablebase::wdl::wdl_from_lines;
use enrich::types::TablebaseProbe;
use futures::future::BoxFuture;
use profile_core::types::Wdl;
use sqlx::PgPool;

// Lichess API (2026-11-21): draw, loss, win (dtz 13).
const KPK_DRAW: &str = "8/4k3/8/4K3/4P3/8/8/8 w - - 0 1";
const KPK_LOSS: &str = "8/4k3/8/4K3/4P3/8/8/8 b - - 0 1";
const KQK_WIN: &str = "8/8/8/8/8/4k3/8/KQ6 w - - 0 1";
// Lichess API (2026-11-22): KRvK win with dtz 27 for White to move, loss with dtz -30 for
// Black to move.
const KRK_WIN: &str = "8/8/8/8/8/2k5/8/KR6 w - - 0 1";
const KRK_LOSS: &str = "8/8/8/8/8/2k5/8/KR6 b - - 0 1";
// Lichess API: draw.
const KRKR_DRAW: &str = "8/8/8/4k3/8/8/r7/4K2R w - - 0 1";
// Lichess API: win, dtz 57.
const KBNK_WIN: &str = "7k/8/8/8/8/8/8/KBN5 w - - 0 1";
const KBNK_DTZ: u32 = 57;
// Lichess API: with the en-passant square, exd6 draws; without it, White loses (dtz -2).
const EP_DRAW: &str = "K7/8/8/3pPk2/8/8/8/8 w - d6 0 1";
const NO_EP_LOSS: &str = "K7/8/8/3pPk2/8/8/8/8 w - - 0 1";
// KRvK with White castling rights: Syzygy has no castling, so it must not be probed.
const CASTLING: &str = "4k3/8/8/8/8/8/8/4K2R w K - 0 1";
// Six pieces.
const SIX: &str = "4k3/8/8/8/8/8/PPPP4/4K3 w - - 0 1";
// Black is checkmated / stalemated.
const MATED: &str = "k7/1Q6/1K6/8/8/8/8/8 b - - 0 1";
const STALEMATE: &str = "k7/2Q5/1K6/8/8/8/8/8 b - - 0 1";

fn line(score: EngineScore) -> AnalysisLine {
    AnalysisLine {
        multipv: 1,
        score,
        pv: vec!["a1a2".into()],
        depth: 1,
        nodes: 1,
    }
}

// --- wdl_from_lines --------------------------------------------------------------------------

#[test]
fn wdl_from_lines_maps_tablebase_scores() {
    assert_eq!(wdl_from_lines(&[line(EngineScore::Cp(19_980))]), Some(1));
    assert_eq!(wdl_from_lines(&[line(EngineScore::Cp(-19_990))]), Some(-1));
    assert_eq!(wdl_from_lines(&[line(EngineScore::Cp(35))]), Some(0));
    assert_eq!(wdl_from_lines(&[line(EngineScore::Mate(3))]), Some(1));
    assert_eq!(wdl_from_lines(&[line(EngineScore::Mate(0))]), Some(-1));
    assert_eq!(wdl_from_lines(&[line(EngineScore::Mate(-4))]), Some(-1));
    assert_eq!(wdl_from_lines(&[]), None);
}

#[test]
fn wdl_from_lines_boundaries() {
    assert_eq!(wdl_from_lines(&[line(EngineScore::Cp(19_000))]), Some(1));
    assert_eq!(wdl_from_lines(&[line(EngineScore::Cp(18_999))]), Some(0));
    assert_eq!(wdl_from_lines(&[line(EngineScore::Cp(-19_000))]), Some(-1));
    assert_eq!(wdl_from_lines(&[line(EngineScore::Cp(-18_999))]), Some(0));
}

// --- fake engine -----------------------------------------------------------------------------

/// A temp dir holding a fake Stockfish script that logs its stdin, plus a `tables/` dir.
/// The script is written by a separate `sh` process (no ETXTBSY on exec, see engine tests).
struct Fake {
    dir: PathBuf,
}

impl Fake {
    fn new(name: &str, tables: &[&str]) -> Self {
        let dir = std::env::temp_dir().join(format!("tb-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("tables")).unwrap();
        for t in tables {
            std::fs::write(dir.join("tables").join(t), b"").unwrap();
        }
        let log = dir.join("stdin.log");
        let body = format!(
            "#!/bin/sh\nwhile read -r l; do\n  echo \"$l\" >> '{}'\n  case \"$l\" in\n    uci) echo 'id name Fake'; echo uciok;;\n    isready) echo readyok;;\n    go*) echo 'info depth 1 score cp 20000 nodes 1 pv a1a2'; echo 'bestmove a1a2';;\n    quit) exit 0;;\n  esac\ndone\n",
            log.display()
        );
        let path = dir.join("engine");
        let status = std::process::Command::new("sh")
            .args([
                "-c",
                "printf '%s' \"$1\" > \"$2\" && chmod 755 \"$2\"",
                "sh",
            ])
            .arg(body)
            .arg(&path)
            .status()
            .unwrap();
        assert!(status.success());
        Self { dir }
    }

    fn engine(&self) -> PathBuf {
        self.dir.join("engine")
    }

    fn tables(&self) -> PathBuf {
        self.dir.join("tables")
    }

    fn log(&self) -> String {
        std::fs::read_to_string(self.dir.join("stdin.log")).unwrap_or_default()
    }

    fn gos(&self) -> usize {
        self.log().lines().filter(|l| l.starts_with("go")).count()
    }
}

impl Drop for Fake {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[cfg(unix)]
#[tokio::test]
async fn six_piece_fen_makes_no_engine_call() {
    let f = Fake::new("six", &["KQvK.rtbw", "KPPPvK.rtbw"]);
    let pool = SyzygyPool::start(&f.engine(), &f.tables(), 1, 1000)
        .await
        .unwrap();
    assert_eq!(pool.max_pieces(), 5);
    assert_eq!(pool.probe(SIX).await, None);
    assert_eq!(pool.probe_dtz(SIX).await, None);
    assert_eq!(pool.engine_calls(), 0);
    pool.close().await;
    assert_eq!(f.gos(), 0, "log: {}", f.log());
}

#[cfg(unix)]
#[tokio::test]
async fn castling_rights_are_never_probed() {
    let f = Fake::new("castle", &["KRvK.rtbw"]);
    let pool = SyzygyPool::start(&f.engine(), &f.tables(), 1, 1000)
        .await
        .unwrap();
    assert_eq!(pool.probe(CASTLING).await, None);
    assert_eq!(pool.engine_calls(), 0);
    pool.close().await;
    assert_eq!(f.gos(), 0);
}

#[cfg(unix)]
#[tokio::test]
async fn material_without_a_table_file_is_not_probed() {
    // Only KQvK is present: a KRvKR position would get a search eval, not a tablebase answer.
    let f = Fake::new("nofile", &["KQvK.rtbw"]);
    let pool = SyzygyPool::start(&f.engine(), &f.tables(), 1, 1000)
        .await
        .unwrap();
    assert_eq!(pool.probe(KRKR_DRAW).await, None);
    assert_eq!(pool.engine_calls(), 0);
    pool.close().await;
    assert_eq!(f.gos(), 0);
}

#[cfg(unix)]
#[tokio::test]
async fn invalid_fen_is_not_probed() {
    let f = Fake::new("invalid", &["KQvK.rtbw"]);
    let pool = SyzygyPool::start(&f.engine(), &f.tables(), 1, 1000)
        .await
        .unwrap();
    assert_eq!(pool.probe("8/8/8/8 w - - 0 1").await, None);
    assert_eq!(pool.probe("not a fen").await, None);
    assert_eq!(pool.engine_calls(), 0);
    pool.close().await;
}

#[cfg(unix)]
#[tokio::test]
async fn bare_kings_are_a_draw_without_an_engine_call() {
    let f = Fake::new("kvk", &["KQvK.rtbw"]);
    let pool = SyzygyPool::start(&f.engine(), &f.tables(), 1, 1000)
        .await
        .unwrap();
    assert_eq!(pool.probe("8/8/4k3/8/8/4K3/8/8 w - - 0 1").await, Some(0));
    assert_eq!(pool.engine_calls(), 0);
    pool.close().await;
}

#[cfg(unix)]
#[tokio::test]
async fn pool_sets_syzygy_path_and_sends_normalised_clocks() {
    let f = Fake::new("wire", &["KQvK.rtbw", "KQvK.rtbz"]);
    let pool = SyzygyPool::start(&f.engine(), &f.tables(), 1, 1000)
        .await
        .unwrap();
    assert_eq!(pool.tables_found(), 1);
    assert_eq!(pool.status(), TablebaseStatus::Available { tables: 1 });
    assert_eq!(pool.status().as_str(), "available");
    // The halfmove clock is reset so the answer depends only on the fen_key.
    assert_eq!(
        pool.probe("8/8/8/8/8/4k3/8/KQ6 w - - 77 120").await,
        Some(1)
    );
    // Table names may list either side first (KBNvKQ): the reversed order is found too.
    assert_eq!(pool.probe("8/8/8/8/8/4K3/8/kq6 b - - 0 1").await, Some(1));
    assert_eq!(pool.engine_calls(), 2);
    pool.close().await;
    let log = f.log();
    assert!(
        log.contains(&format!(
            "setoption name SyzygyPath value {}",
            f.tables().display()
        )),
        "log: {log}"
    );
    assert!(
        log.contains("position fen 8/8/8/8/8/4k3/8/KQ6 w - - 0 1"),
        "log: {log}"
    );
    assert!(log.contains("go nodes 1000"), "log: {log}");
}

#[tokio::test]
async fn missing_tablebase_files_returns_none() {
    let dir = std::env::temp_dir().join(format!("tb-{}-empty", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    // No engine is started when there are no tables: the binary path is never used.
    let pool = SyzygyPool::start(Path::new("/nonexistent/stockfish"), &dir, 1, 1000)
        .await
        .unwrap();
    assert_eq!(pool.tables_found(), 0);
    assert_eq!(pool.status(), TablebaseStatus::Unavailable);
    assert_eq!(pool.status().as_str(), "unavailable");
    assert_eq!(pool.probe(KQK_WIN).await, None);
    assert_eq!(pool.probe(KPK_DRAW).await, None);
    assert_eq!(pool.probe_dtz(KQK_WIN).await, None);
    assert_eq!(pool.engine_calls(), 0);
    pool.close().await;
    let _ = std::fs::remove_dir_all(&dir);

    // A path that does not exist behaves the same.
    let gone = SyzygyPool::start(
        Path::new("/nonexistent/stockfish"),
        Path::new("/nonexistent/syzygy"),
        1,
        1000,
    )
    .await
    .unwrap();
    assert_eq!(gone.status(), TablebaseStatus::Unavailable);
    assert_eq!(gone.probe(KQK_WIN).await, None);
}

#[cfg(unix)]
#[tokio::test]
async fn main_pool_never_sets_syzygy_path() {
    let f = Fake::new("main", &[]);
    let pool = EnginePool::create(PoolOptions {
        size: 1,
        path: f.engine(),
        hash_mb: None,
    })
    .await
    .unwrap();
    pool.analyse(
        KQK_WIN,
        AnalyseOptions {
            nodes: 10,
            multipv: 1,
            timeout: Duration::from_secs(10),
        },
    )
    .await
    .unwrap();
    pool.close().await;
    let log = f.log();
    assert!(log.contains("go nodes 10"), "log: {log}");
    assert!(!log.contains("SyzygyPath"), "log: {log}");
}

// --- cached probe ----------------------------------------------------------------------------

struct Counting {
    inner: Arc<dyn TablebaseProbe>,
    calls: AtomicUsize,
}

impl TablebaseProbe for Counting {
    fn max_pieces(&self) -> u32 {
        self.inner.max_pieces()
    }

    fn probe<'a>(&'a self, fen: &'a str) -> BoxFuture<'a, Option<Wdl>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.inner.probe(fen)
    }
}

/// Answers 1 for every position.
struct AlwaysWin;

impl TablebaseProbe for AlwaysWin {
    fn max_pieces(&self) -> u32 {
        5
    }

    fn probe<'a>(&'a self, _fen: &'a str) -> BoxFuture<'a, Option<Wdl>> {
        Box::pin(async { Some(1) })
    }
}

/// Answers nothing.
struct Never;

impl TablebaseProbe for Never {
    fn max_pieces(&self) -> u32 {
        5
    }

    fn probe<'a>(&'a self, _fen: &'a str) -> BoxFuture<'a, Option<Wdl>> {
        Box::pin(async { None })
    }
}

async fn tb_row(pool: &PgPool, key: &str) -> Option<(i16, String)> {
    sqlx::query_as("SELECT wdl, source FROM position_tb WHERE fen_key = $1")
        .bind(key)
        .fetch_optional(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn cached_probe_reads_and_writes_position_tb(pool: PgPool) {
    let counting = Arc::new(Counting {
        inner: Arc::new(AlwaysWin),
        calls: AtomicUsize::new(0),
    });
    let cached = CachedTbProbe::new(counting.clone(), pool.clone());
    assert_eq!(cached.max_pieces(), 5);
    assert_eq!(cached.probe(KQK_WIN).await, Some(1));
    // Same fen_key with other clocks: answered from the table.
    assert_eq!(
        cached.probe("8/8/8/8/8/4k3/8/KQ6 w - - 12 40").await,
        Some(1)
    );
    assert_eq!(counting.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        tb_row(&pool, "8/8/8/8/8/4k3/8/KQ6 w - -").await,
        Some((1, "syzygy5".to_owned()))
    );
    // A stored row is used even by a fresh cache over another probe.
    let fresh = CachedTbProbe::new(Arc::new(Never), pool.clone());
    assert_eq!(fresh.probe(KQK_WIN).await, Some(1));
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn cached_probe_does_not_store_none_or_probe_uncovered(pool: PgPool) {
    let counting = Arc::new(Counting {
        inner: Arc::new(Never),
        calls: AtomicUsize::new(0),
    });
    let cached = CachedTbProbe::new(counting.clone(), pool.clone());
    assert_eq!(cached.probe(KQK_WIN).await, None);
    assert_eq!(cached.probe(SIX).await, None);
    assert_eq!(cached.probe(CASTLING).await, None);
    // Only the covered position reached the inner probe.
    assert_eq!(counting.calls.load(Ordering::SeqCst), 1);
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM position_tb")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 0);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn cached_probe_keys_by_canonical_fen(pool: PgPool) {
    // An en-passant square with no legal capture is not part of the canonical key.
    let cached = CachedTbProbe::new(Arc::new(AlwaysWin), pool.clone());
    assert_eq!(
        cached.probe("8/8/8/8/4P3/8/8/K5k1 b - e3 0 1").await,
        Some(1)
    );
    assert_eq!(
        tb_row(&pool, "8/8/8/8/4P3/8/8/K5k1 b - -").await,
        Some((1, "syzygy5".to_owned()))
    );
    // A real en-passant square is kept.
    assert_eq!(cached.probe(EP_DRAW).await, Some(1));
    assert!(tb_row(&pool, "K7/8/8/3pPk2/8/8/8/8 w - d6").await.is_some());
}

// --- real Stockfish and real tables ----------------------------------------------------------

/// The tables from `SYZYGY_PATH`: `None` (skip) only when it is unset. When it is set but the
/// 3-4-5 files are missing, the test fails under `IN_DOCKER=1` and skips outside Docker.
fn syzygy_dir() -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var_os("SYZYGY_PATH").filter(|v| !v.is_empty())?);
    let present = ["KPvK.rtbw", "KQvK.rtbw"]
        .iter()
        .all(|f| dir.join(f).is_file());
    if !present {
        assert!(
            !testkit::in_docker(),
            "SYZYGY_PATH={} has no KPvK.rtbw/KQvK.rtbw (IN_DOCKER=1)",
            dir.display()
        );
        return None;
    }
    Some(dir)
}

fn real() -> Option<(PathBuf, PathBuf)> {
    let tables = syzygy_dir()?;
    let stockfish = testkit::stockfish()?;
    Some((stockfish, tables))
}

#[tokio::test]
async fn real_tables_give_lichess_results() {
    let Some((sf, tables)) = real() else {
        eprintln!("skipped: SYZYGY_PATH unset");
        return;
    };
    let pool = SyzygyPool::start(&sf, &tables, 1, 1000).await.unwrap();
    let t0 = std::time::Instant::now();
    assert_eq!(pool.tables_found(), 145);
    // Brief Step 1.
    assert_eq!(pool.probe(KPK_DRAW).await, Some(0), "KPvK w");
    assert_eq!(pool.probe(KPK_LOSS).await, Some(-1), "KPvK b");
    assert_eq!(pool.probe(KQK_WIN).await, Some(1), "KQvK");
    // Further known positions.
    assert_eq!(pool.probe(KRKR_DRAW).await, Some(0), "KRvKR");
    assert_eq!(pool.probe(KBNK_WIN).await, Some(1), "KBNvK");
    assert_eq!(pool.probe(EP_DRAW).await, Some(0), "ep exd6 draws");
    assert_eq!(pool.probe(NO_EP_LOSS).await, Some(-1), "no ep loses");
    assert_eq!(pool.probe(MATED).await, Some(-1), "checkmated");
    assert_eq!(pool.probe(STALEMATE).await, Some(0), "stalemate");
    eprintln!("9 probes took {:?}", t0.elapsed());
    let calls = pool.engine_calls();
    // Not covered: never a guess, never an engine call.
    assert_eq!(pool.probe(CASTLING).await, None, "castling");
    assert_eq!(pool.probe(SIX).await, None, "six pieces");
    assert_eq!(pool.engine_calls(), calls);
    pool.close().await;
}

/// The probe API is WDL only, but the DTZ tables are what makes the 50-move rule exact. With a
/// halfmove clock h, Stockfish's root probe counts a win only when dtz + h <= 99: KBNvK at dtz 57
/// (Lichess) wins with h = 35 and is a 50-move draw with h = 60. The pool resets the clock, so it
/// reports the win for both.
#[tokio::test]
async fn real_dtz_bounds_the_fifty_move_rule() {
    let Some((sf, tables)) = real() else {
        eprintln!("skipped: SYZYGY_PATH unset");
        return;
    };
    let mut e = UciEngine::start(StartOptions {
        path: sf.clone(),
        threads: None,
        hash_mb: None,
        syzygy_path: Some(tables.display().to_string()),
        handshake_timeout: None,
    })
    .await
    .unwrap();
    let opts = AnalyseOptions {
        nodes: 1000,
        multipv: 1,
        timeout: Duration::from_secs(10),
    };
    let fen = |h: u32| format!("7k/8/8/8/8/8/8/KBN5 w - - {h} 60");
    let inside = e.analyse(&fen(99 - KBNK_DTZ - 7), opts).await.unwrap();
    let beyond = e.analyse(&fen(99 - KBNK_DTZ + 18), opts).await.unwrap();
    // Raw scores, so the answers are shown to come from the tables and not from a 1000-node
    // search: a tablebase draw is exactly cp 0.
    let draw = e.analyse(KRKR_DRAW, opts).await.unwrap();
    e.quit().await;
    eprintln!(
        "inside {:?} / beyond {:?} / KRvKR {:?}",
        inside.lines[0].score, beyond.lines[0].score, draw.lines[0].score
    );
    assert_eq!(wdl_from_lines(&inside.lines), Some(1), "{inside:?}");
    assert_eq!(wdl_from_lines(&beyond.lines), Some(0), "{beyond:?}");
    assert_eq!(draw.lines[0].score, EngineScore::Cp(0), "{draw:?}");

    let pool = SyzygyPool::start(&sf, &tables, 1, 1000).await.unwrap();
    assert_eq!(pool.probe(&fen(99 - KBNK_DTZ + 18)).await, Some(1));
    pool.close().await;
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn real_cached_probe_answers_second_lookup_without_engine(pg: PgPool) {
    let Some((sf, tables)) = real() else {
        eprintln!("skipped: SYZYGY_PATH unset");
        return;
    };
    let syz = Arc::new(SyzygyPool::start(&sf, &tables, 1, 1000).await.unwrap());
    let cached = CachedTbProbe::new(syz.clone(), pg.clone());
    assert_eq!(cached.probe(KPK_LOSS).await, Some(-1));
    assert_eq!(syz.engine_calls(), 1);
    assert_eq!(cached.probe(KPK_LOSS).await, Some(-1));
    assert_eq!(syz.engine_calls(), 1, "second lookup used the engine");
    assert_eq!(
        tb_row(&pg, "8/4k3/8/4K3/4P3/8/8/8 b - -").await,
        Some((-1, "syzygy5".to_owned()))
    );
    syz.close().await;
}

/// DTZ from the real tables, read through Stockfish's root rule (a win counts only when
/// dtz + halfmove clock <= 99; see ADR-R2-12), against the Lichess API values.
#[tokio::test]
async fn real_tables_give_lichess_dtz() {
    let Some((sf, tables)) = real() else {
        eprintln!("skipped: SYZYGY_PATH unset");
        return;
    };
    let pool = SyzygyPool::start(&sf, &tables, 1, 1000).await.unwrap();
    assert_eq!(pool.status(), TablebaseStatus::Available { tables: 145 });
    let t0 = std::time::Instant::now();
    assert_eq!(pool.probe(KRK_WIN).await, Some(1), "KRvK w");
    assert_eq!(pool.probe(KRK_LOSS).await, Some(-1), "KRvK b");
    assert_eq!(pool.probe_dtz(KQK_WIN).await, Some(13), "KQvK");
    assert_eq!(pool.probe_dtz(KRK_WIN).await, Some(27), "KRvK w");
    assert_eq!(pool.probe_dtz(KBNK_WIN).await, Some(57), "KBNvK");
    assert_eq!(pool.probe_dtz(KRK_LOSS).await, Some(-30), "KRvK b");
    assert_eq!(pool.probe_dtz(KPK_LOSS).await, Some(-4), "KPvK b");
    assert_eq!(pool.probe_dtz(NO_EP_LOSS).await, Some(-2), "no ep loses");
    assert_eq!(pool.probe_dtz(KPK_DRAW).await, Some(0), "KPvK w");
    assert_eq!(pool.probe_dtz(CASTLING).await, None, "castling");
    eprintln!("dtz probes took {:?}", t0.elapsed());
    pool.close().await;
}

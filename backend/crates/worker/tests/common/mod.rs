//! Helpers shared by the worker's integration tests.
#![allow(dead_code)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use analysis::eval_cache::DbEvalCache;
use analysis::jobs::analyse::AnalyseGameDeps;
use db::games::{GameRow, get_game};
use engine::{AnalyseOptions, Analyser, AnalysisResult, EngineError};
use futures::future::BoxFuture;
use sqlx::PgPool;
use sqlx::postgres::PgConnectOptions;
use testkit::fake_engine::FakeAnalyser;
use testkit::{GameSeed, insert_account, insert_game};
use uuid::Uuid;
use worker::config::WorkerEnv;

/// 10 plies, 11 distinct positions, not terminal.
pub const RUY: &str = "1. e4 e5 2. Nf3 Nc6 3. Bb5 a6 4. Ba4 Nf6 5. O-O Be7 *";

/// The URL of the sqlx::test database, built from the master `DATABASE_URL`.
pub fn test_db_url(opts: &PgConnectOptions) -> String {
    let master = std::env::var("DATABASE_URL").expect("DATABASE_URL");
    let (base, _) = master
        .rsplit_once('/')
        .expect("database name in DATABASE_URL");
    format!(
        "{base}/{}",
        opts.get_database().expect("test database name")
    )
}

/// A worker environment on the test database with the real Stockfish, or `None` (skip) when
/// Stockfish is not configured outside Docker.
pub fn env_for(opts: &PgConnectOptions) -> Option<WorkerEnv> {
    let stockfish_path = testkit::stockfish()?;
    Some(WorkerEnv {
        database_url: test_db_url(opts),
        stockfish_path,
        lichess_token: None,
        lichess_lookback_days: None,
        pool_size: Some(1),
    })
}

pub async fn add_game(pool: &PgPool, pgn: &str, username: &str) -> (Uuid, Uuid) {
    let acct = insert_account(pool, "chesscom", username).await;
    let id = insert_game(
        pool,
        acct,
        GameSeed {
            pgn: pgn.to_owned(),
            white: username.to_owned(),
            ..GameSeed::default()
        },
    )
    .await;
    (acct, id)
}

pub async fn game(pool: &PgPool, id: Uuid) -> GameRow {
    get_game(pool, id).await.unwrap().expect("game row")
}

/// `(state, attempts)` of the game's analyse job (the newest one).
pub async fn analyse_job(pool: &PgPool, game_id: Uuid) -> (String, i32) {
    sqlx::query_as(
        "SELECT state::text, attempts FROM jobs WHERE dedupe_key = $1 ORDER BY id DESC LIMIT 1",
    )
    .bind(queue::analyse_key(game_id))
    .fetch_one(pool)
    .await
    .unwrap()
}

pub async fn event_types(pool: &PgPool, account: Uuid) -> Vec<String> {
    sqlx::query_scalar("SELECT type::text FROM user_events WHERE account_id = $1 ORDER BY id")
        .bind(account)
        .fetch_all(pool)
        .await
        .unwrap()
}

pub async fn wait_until<T, F, Fut>(what: &str, timeout: Duration, f: F) -> T
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Option<T>>,
{
    testkit::wait_for(what, timeout, Duration::from_millis(20), f).await
}

pub fn deps(pool: &PgPool, analyser: Arc<dyn Analyser>) -> AnalyseGameDeps {
    let mut d = AnalyseGameDeps::new(
        pool.clone(),
        analyser,
        Arc::new(DbEvalCache::new(pool.clone())),
    );
    d.concurrency = 1;
    d
}

/// Answers the first `ok` calls like [`FakeAnalyser`]; every later call never resolves.
pub struct HangAfter {
    pub ok: usize,
    pub calls: AtomicUsize,
    pub fens: std::sync::Mutex<Vec<String>>,
    inner: FakeAnalyser,
}

impl HangAfter {
    pub fn new(ok: usize) -> Arc<Self> {
        Arc::new(HangAfter {
            ok,
            calls: AtomicUsize::new(0),
            fens: std::sync::Mutex::new(Vec::new()),
            inner: FakeAnalyser::new(),
        })
    }

    pub fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl Analyser for HangAfter {
    fn analyse<'a>(
        &'a self,
        fen: &'a str,
        opts: AnalyseOptions,
    ) -> BoxFuture<'a, Result<AnalysisResult, EngineError>> {
        self.fens.lock().unwrap().push(fen.to_owned());
        if self.calls.fetch_add(1, Ordering::SeqCst) >= self.ok {
            Box::pin(futures::future::pending())
        } else {
            self.inner.analyse(fen, opts)
        }
    }
}

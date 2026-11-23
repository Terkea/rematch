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
use tokio::sync::{Semaphore, SemaphorePermit};
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

/// The Postgres connections the tests of one binary may hold at once: the compose `postgres`
/// service's `max_connections` (300, docker-compose.yml, ADR 0066) minus sqlx::test's master pool
/// (20, shared by the binary; each test's own `PgPool` argument is a child of it).
///
/// Every test runs on its own runtime and thread, so with the default test threads (one per core)
/// all of a binary's worker tests start at once. A worker started with AI may open
/// `max_connections(1, true)` = 58 connections, and its runners open about 45 of them in the first
/// second. Twelve such workers asked Postgres for far more than 300: it answered "too many
/// clients", sqlx retried that until the acquire timeout, and a start failed with "pool timed
/// out". Each worker test reserves its share first, so the tests queue here instead.
static PG_CONNECTIONS: Semaphore = Semaphore::const_new(300 - 20);

/// Reserves, until the permit drops, every connection a worker started with `pool_size` engines
/// (and the AI runners, the default) may open, plus `own` for the pools the test opens itself.
/// Take it before `start_workers` and keep it until the worker is stopped.
pub async fn reserve_connections(pool_size: usize, own: usize) -> SemaphorePermit<'static> {
    let n = worker::start::max_connections(pool_size, true) + own;
    PG_CONNECTIONS
        .acquire_many(u32::try_from(n).expect("a test's connection budget fits in u32"))
        .await
        .expect("the connection budget is never closed")
}

/// A worker environment on the test database with the real Stockfish, or `None` (skip) when
/// Stockfish is not configured outside Docker.
pub fn env_for(opts: &PgConnectOptions) -> Option<WorkerEnv> {
    let stockfish_path = testkit::stockfish()?;
    Some(WorkerEnv {
        pool_size: Some(1),
        ..WorkerEnv::new(test_db_url(opts), stockfish_path)
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

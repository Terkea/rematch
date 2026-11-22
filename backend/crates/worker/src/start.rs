//! Starting the worker (port of `apps/worker/src/start-workers.ts`).
//!
//! Order: check the engine is Stockfish 19, adopt a Drizzle-migrated database and migrate it
//! (on a separate pool without a statement timeout), connect the worker pool, start the engine
//! pool, then one runner per job kind, and schedule the recurring reconcile. Whatever was created
//! is closed again if a later step fails.
//!
//! Connection budget: `max_connections = pool_size + POOL_HEADROOM`. The analyse runner holds at
//! most one connection per in-flight game (`pool_size` of them), each in-flight import holds its
//! dedicated account-lock connection plus at most one pooled connection at a time
//! (`2 * IMPORT_CONCURRENCY = 4`), the reconciler one, and each runner's NOTIFY listener one (3).
//! That leaves no slack below `pool_size + 8`, but every statement is short and the import's
//! busy checks open short-lived connections, so the pool's acquire timeout (10 s) is the bound.
//! Import concurrency must stay well below `max_connections`.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use analysis::eval_cache::DbEvalCache;
use analysis::jobs::analyse::{AnalyseGameDeps, AnalyseGameHandler};
use analysis::jobs::import::{ImportAccountDeps, ImportAccountHandler};
use analysis::jobs::reconcile::{ReconcileHandler, ensure_reconcile_scheduled};
use anyhow::Context;
use db::pool::DbOptions;
use engine::pool::{EnginePool, PoolOptions};
use engine::version::stockfish_version;
use engine::{ANALYSIS_PROTOCOL, Analyser};
use enrich::tablebase::cached::CachedTbProbe;
use enrich::tablebase::pool::{SyzygyPool, TablebaseStatus};
use enrich::types::TablebaseProbe;
use futures::FutureExt;
use futures::future::BoxFuture;
use platforms::breaker::{BreakerOptions, CircuitBreaker};
use platforms::chesscom::ChesscomSource;
use platforms::client::{ClientOptions, HttpPlatformClient};
use platforms::lichess::LichessSource;
use platforms::{GameSource, Platform, PlatformClient};
use queue::queue::Queue;
use queue::runner::{Runner, RunnerConfig, RunnerHandle};
use sqlx::PgPool;
use tokio::task::AbortHandle;

use crate::config::WorkerEnv;
use crate::shutdown::{Closable, Drainable, ShutdownHandles};

/// Imports at once (each holds an account-lock connection).
pub const IMPORT_CONCURRENCY: usize = 2;
/// Connections beyond one per engine (see the module docs).
pub const POOL_HEADROOM: usize = 8;
/// The runner's hard deadline on an analysis: past the handler's own 10-minute deadline, so the
/// handler normally ends it and records the game itself.
pub const ANALYSE_HARD_TIMEOUT: Duration = Duration::from_secs(11 * 60);
/// The runner's hard deadline on a reconcile sweep.
pub const RECONCILE_HARD_TIMEOUT: Duration = Duration::from_secs(5 * 60);
/// How long the boot-time version probe may take.
pub const VERSION_TIMEOUT: Duration = Duration::from_secs(10);
/// How often the account-busy counter is checked and logged.
const BUSY_LOG_EVERY: Duration = Duration::from_secs(300);

#[derive(Clone, Debug)]
pub struct StartOptions {
    /// Overrides `WORKER_POOL_SIZE`.
    pub pool_size: Option<usize>,
    /// Runner timings and `worker_id` shared by every runner (concurrency and hard timeout are
    /// set per kind). Default: [`RunnerConfig::default`].
    pub runner: Option<RunnerConfig>,
    /// Run the reconcile runner and schedule the recurring sweep.
    pub with_reconciler: bool,
}

impl Default for StartOptions {
    fn default() -> Self {
        StartOptions {
            pool_size: None,
            runner: None,
            with_reconciler: true,
        }
    }
}

/// Why the worker refused to start.
#[derive(Debug)]
pub enum StartError {
    /// The engine at `STOCKFISH_PATH` is not Stockfish 19.x, which the analysis protocol
    /// (`sf19-n300k-v1`) is pinned to.
    WrongStockfish { found: String },
    /// The engine at `STOCKFISH_PATH` could not be probed.
    Probe(engine::EngineError),
}

impl std::fmt::Display for StartError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StartError::WrongStockfish { found } => write!(
                f,
                "STOCKFISH_PATH must be Stockfish 19 (analysis protocol {ANALYSIS_PROTOCOL}), \
                 found {found:?}"
            ),
            StartError::Probe(e) => write!(f, "cannot probe the engine at STOCKFISH_PATH: {e}"),
        }
    }
}

impl std::error::Error for StartError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            StartError::Probe(e) => Some(e),
            StartError::WrongStockfish { .. } => None,
        }
    }
}

/// `true` for a `Stockfish 19` or `Stockfish 19.x` banner.
pub fn is_stockfish_19(banner: &str) -> bool {
    let mut words = banner.split_whitespace();
    if words.next() != Some("Stockfish") {
        return false;
    }
    let Some(version) = words.next() else {
        return false;
    };
    version == "19"
        || version
            .strip_prefix("19.")
            .is_some_and(|minor| minor.chars().next().is_some_and(|c| c.is_ascii_digit()))
}

/// Probes the engine and returns its banner, or [`StartError`] unless it is Stockfish 19.
pub async fn check_stockfish(path: &std::path::Path) -> Result<String, StartError> {
    let banner = stockfish_version(path, VERSION_TIMEOUT)
        .await
        .map_err(StartError::Probe)?;
    if is_stockfish_19(&banner) {
        Ok(banner)
    } else {
        Err(StartError::WrongStockfish { found: banner })
    }
}

/// At most this many engines unless `WORKER_POOL_SIZE` asks for more: a backfill must not
/// saturate a desktop.
pub const MAX_DEFAULT_POOL_SIZE: usize = 8;

/// The default engine count for `cores` CPUs: two left for everything else, at most
/// [`MAX_DEFAULT_POOL_SIZE`], at least 1.
pub fn pool_size_for(cores: usize) -> usize {
    cores.saturating_sub(2).clamp(1, MAX_DEFAULT_POOL_SIZE)
}

/// [`pool_size_for`] this machine's available parallelism.
pub fn default_pool_size() -> usize {
    pool_size_for(std::thread::available_parallelism().map_or(1, |n| n.get()))
}

pub fn analyse_runner_config(base: &RunnerConfig, concurrency: usize) -> RunnerConfig {
    RunnerConfig {
        concurrency: concurrency.max(1),
        hard_timeout: Some(ANALYSE_HARD_TIMEOUT),
        ..base.clone()
    }
}

/// No hard timeout: the import handler owns its idle deadline (an import of a large account can
/// legitimately run for a long time while it makes progress).
pub fn import_runner_config(base: &RunnerConfig) -> RunnerConfig {
    RunnerConfig {
        concurrency: IMPORT_CONCURRENCY,
        hard_timeout: None,
        ..base.clone()
    }
}

pub fn reconcile_runner_config(base: &RunnerConfig) -> RunnerConfig {
    RunnerConfig {
        concurrency: 1,
        hard_timeout: Some(RECONCILE_HARD_TIMEOUT),
        ..base.clone()
    }
}

pub fn spawn_analyse_runner(
    queue: Queue,
    deps: AnalyseGameDeps,
    base: &RunnerConfig,
    concurrency: usize,
) -> RunnerHandle {
    Runner::spawn(
        queue,
        Arc::new(AnalyseGameHandler(Arc::new(deps))),
        analyse_runner_config(base, concurrency),
    )
}

pub fn spawn_import_runner(
    queue: Queue,
    deps: ImportAccountDeps,
    base: &RunnerConfig,
) -> RunnerHandle {
    Runner::spawn(
        queue,
        Arc::new(ImportAccountHandler::new(Arc::new(deps))),
        import_runner_config(base),
    )
}

pub fn spawn_reconcile_runner(queue: Queue, pool: PgPool, base: &RunnerConfig) -> RunnerHandle {
    Runner::spawn(
        queue.clone(),
        Arc::new(ReconcileHandler { pool, queue }),
        reconcile_runner_config(base),
    )
}

/// The production import dependencies: the HTTP clients with the fixed project User-Agent
/// (ADR 0043), one breaker per platform (5 failures, 60 s, `is_outage`), and both sources.
pub fn import_deps(env: &WorkerEnv, pool: &PgPool, queue: &Queue) -> ImportAccountDeps {
    let mut deps = ImportAccountDeps::new(pool.clone(), queue.clone());
    let lichess: Arc<dyn GameSource> = Arc::new(LichessSource {
        token: env.lichess_token.clone(),
    });
    let chesscom: Arc<dyn GameSource> = Arc::new(ChesscomSource::new());
    deps.sources = HashMap::from([(Platform::Lichess, lichess), (Platform::Chesscom, chesscom)]);
    let client = |name: Platform| -> Arc<dyn PlatformClient> {
        Arc::new(HttpPlatformClient::new(ClientOptions {
            name,
            ..ClientOptions::default()
        }))
    };
    deps.clients = HashMap::from([
        (Platform::Lichess, client(Platform::Lichess)),
        (Platform::Chesscom, client(Platform::Chesscom)),
    ]);
    let breaker = || Arc::new(CircuitBreaker::new(BreakerOptions::default()));
    deps.breakers = HashMap::from([
        (Platform::Lichess, breaker()),
        (Platform::Chesscom, breaker()),
    ]);
    if let Some(days) = env.lichess_lookback_days {
        deps.lichess_lookback = Duration::from_secs(u64::from(days) * 24 * 3600);
    }
    deps
}

/// Engines in the tablebase pool (ADR-R2-12).
pub const TABLEBASE_ENGINES: usize = 1;
/// Nodes per tablebase probe (ADR-R2-12).
pub const TABLEBASE_NODES: u64 = 1000;

/// The worker's tablebase probe (R2 Task 23): its own Stockfish set with `SyzygyPath`, never the
/// main engine pool, behind the `position_tb` cache. The enrichment pipeline (R2 Tasks 25, 31,
/// 35) takes `probe`.
pub struct WorkerTablebase {
    pub pool: Arc<SyzygyPool>,
    /// [`CachedTbProbe`] over `pool`.
    pub probe: Arc<dyn TablebaseProbe>,
}

impl WorkerTablebase {
    pub fn status(&self) -> TablebaseStatus {
        self.pool.status()
    }
}

/// The tablebase probe for `SYZYGY_PATH`: `None` when it is unset or when its engines cannot
/// start. A path without tables gives an unavailable probe. Never an error: analysis does not
/// depend on the tables.
pub async fn start_tablebase(env: &WorkerEnv, pool: &PgPool) -> Option<WorkerTablebase> {
    let Some(path) = env.syzygy_path.as_deref() else {
        tracing::info!("SYZYGY_PATH is not set; no tablebase probe");
        return None;
    };
    match SyzygyPool::start(
        &env.stockfish_path,
        path,
        TABLEBASE_ENGINES,
        TABLEBASE_NODES,
    )
    .await
    {
        Ok(tb) => {
            let tb = Arc::new(tb);
            tracing::info!(
                path = %path.display(),
                status = tb.status().as_str(),
                tables = tb.tables_found(),
                "tablebase probe"
            );
            Some(WorkerTablebase {
                probe: Arc::new(CachedTbProbe::new(tb.clone(), pool.clone())),
                pool: tb,
            })
        }
        Err(err) => {
            tracing::warn!(%err, path = %path.display(), "tablebase engines failed to start; tablebase unavailable");
            None
        }
    }
}

/// A started worker. Turn it into [`ShutdownHandles`] with
/// [`into_shutdown_handles`](Self::into_shutdown_handles).
pub struct RunningWorkers {
    pub pool: PgPool,
    pub queue: Queue,
    pub engines: Arc<EnginePool>,
    /// Import, analyse and (with the reconciler) reconcile.
    pub runners: Vec<RunnerHandle>,
    pub stockfish_version: String,
    /// `None` when `SYZYGY_PATH` is unset (see [`start_tablebase`]).
    pub tablebase: Option<WorkerTablebase>,
    /// Imports released because another handler held the account (`account_busy`).
    pub busy_releases: Arc<AtomicU64>,
    monitor: AbortHandle,
}

impl RunningWorkers {
    pub fn into_shutdown_handles(self) -> ShutdownHandles {
        ShutdownHandles {
            runners: self
                .runners
                .into_iter()
                .map(|r| Box::new(r) as Box<dyn Drainable>)
                .collect(),
            engines: Box::new(Engines {
                main: self.engines,
                tablebase: self.tablebase.map(|tb| tb.pool),
            }),
            db: Box::new(DbClose {
                pool: self.pool,
                monitor: self.monitor,
            }),
        }
    }
}

/// The main engine pool and the tablebase engines, closed together.
struct Engines {
    main: Arc<EnginePool>,
    tablebase: Option<Arc<SyzygyPool>>,
}

impl Closable for Engines {
    fn close(&self) -> BoxFuture<'_, anyhow::Result<()>> {
        async move {
            if let Some(tb) = &self.tablebase {
                tb.close().await;
            }
            EnginePool::close(&self.main).await;
            Ok(())
        }
        .boxed()
    }
}

/// Closes the pool, after stopping the stats monitor.
struct DbClose {
    pool: PgPool,
    monitor: AbortHandle,
}

impl Closable for DbClose {
    fn close(&self) -> BoxFuture<'_, anyhow::Result<()>> {
        self.monitor.abort();
        async move {
            self.pool.close().await;
            Ok(())
        }
        .boxed()
    }
}

/// Starts the worker (see the module docs).
pub async fn start_workers(env: &WorkerEnv, opts: StartOptions) -> anyhow::Result<RunningWorkers> {
    let pool_size = opts
        .pool_size
        .or(env.pool_size)
        .unwrap_or_else(default_pool_size)
        .max(1);
    let stockfish_version = check_stockfish(&env.stockfish_path).await?;

    // Migrations run without the worker's statement timeout (ADR 0033).
    let migrations = db::pool::connect_for_migrations(&env.database_url)
        .await
        .context("connecting to DATABASE_URL")?;
    let migrated = async {
        db::baseline::adopt_drizzle_baseline(&migrations)
            .await
            .context("adopting the Drizzle baseline")?;
        db::pool::migrate(&migrations)
            .await
            .context("running migrations")?;
        anyhow::Ok(())
    }
    .await;
    migrations.close().await;
    migrated?;

    let pool = db::pool::connect(
        &env.database_url,
        DbOptions {
            max_connections: u32::try_from(pool_size + POOL_HEADROOM).unwrap_or(u32::MAX),
            ..DbOptions::default()
        },
    )
    .await
    .context("connecting to DATABASE_URL")?;
    let engines = match EnginePool::create(PoolOptions {
        size: pool_size,
        path: env.stockfish_path.clone(),
        hash_mb: None,
    })
    .await
    {
        Ok(engines) => engines,
        Err(e) => {
            pool.close().await;
            return Err(anyhow::Error::new(e).context("starting the engine pool"));
        }
    };

    let queue = Queue::new(pool.clone());
    let base = opts.runner.clone().unwrap_or_default();
    let import = import_deps(env, &pool, &queue);
    let busy_releases = import.busy_releases.clone();
    let analyser: Arc<dyn Analyser> = engines.clone();
    let analyse = AnalyseGameDeps::new(
        pool.clone(),
        analyser,
        Arc::new(DbEvalCache::new(pool.clone())),
    );
    let mut runners = vec![
        spawn_import_runner(queue.clone(), import, &base),
        spawn_analyse_runner(queue.clone(), analyse, &base, pool_size),
    ];
    if opts.with_reconciler {
        runners.push(spawn_reconcile_runner(queue.clone(), pool.clone(), &base));
        if let Err(e) = ensure_reconcile_scheduled(&queue).await {
            for runner in &runners {
                runner.force().await;
            }
            EnginePool::close(&engines).await;
            pool.close().await;
            return Err(anyhow::Error::new(e).context("scheduling the reconciler"));
        }
    }
    let tablebase = start_tablebase(env, &pool).await;
    let monitor = tokio::spawn(log_busy_releases(busy_releases.clone())).abort_handle();
    tracing::info!(
        stockfish_version = %stockfish_version,
        pool_size,
        max_connections = pool_size + POOL_HEADROOM,
        runners = runners.len(),
        "worker ready"
    );
    Ok(RunningWorkers {
        pool,
        queue,
        engines,
        runners,
        stockfish_version,
        tablebase,
        busy_releases,
        monitor,
    })
}

/// Logs the `account_busy` counter whenever it grew: repeated releases can mean a wedged
/// handler still holds an account lock (it is freed when its session ends).
async fn log_busy_releases(counter: Arc<AtomicU64>) {
    let mut last = 0;
    let mut tick = tokio::time::interval(BUSY_LOG_EVERY);
    tick.tick().await;
    loop {
        tick.tick().await;
        let now = counter.load(Ordering::Relaxed);
        if now > last {
            tracing::warn!(
                busy_releases = now,
                since_last = now - last,
                "imports were released because another handler held the account"
            );
        }
        last = now;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_stockfish_19_banners() {
        assert!(is_stockfish_19(
            "Stockfish 19 by the Stockfish developers (see AUTHORS file)"
        ));
        assert!(is_stockfish_19(
            "Stockfish 19.1 by the Stockfish developers"
        ));
        assert!(is_stockfish_19("Stockfish 19"));
        assert!(!is_stockfish_19("Stockfish 18 by the Stockfish developers"));
        assert!(!is_stockfish_19("Stockfish 190"));
        assert!(!is_stockfish_19("Stockfish dev-20261101-abcdef"));
        assert!(!is_stockfish_19("Komodo 19"));
        assert!(!is_stockfish_19(""));
    }

    #[test]
    fn per_kind_runner_configs() {
        let base = RunnerConfig::default();
        let a = analyse_runner_config(&base, 3);
        assert_eq!(
            (a.concurrency, a.hard_timeout),
            (3, Some(ANALYSE_HARD_TIMEOUT))
        );
        let i = import_runner_config(&base);
        assert_eq!((i.concurrency, i.hard_timeout), (2, None));
        let r = reconcile_runner_config(&base);
        assert_eq!(
            (r.concurrency, r.hard_timeout),
            (1, Some(RECONCILE_HARD_TIMEOUT))
        );
        assert_eq!(a.worker_id, base.worker_id);
        assert_eq!(ANALYSE_HARD_TIMEOUT, Duration::from_secs(660));
        assert_eq!(RECONCILE_HARD_TIMEOUT, Duration::from_secs(300));
        assert!((1..=MAX_DEFAULT_POOL_SIZE).contains(&default_pool_size()));
        for (cores, size) in [
            (0, 1),
            (1, 1),
            (2, 1),
            (3, 1),
            (4, 2),
            (10, 8),
            (24, 8),
            (64, 8),
        ] {
            assert_eq!(pool_size_for(cores), size, "{cores} cores");
        }
    }
}

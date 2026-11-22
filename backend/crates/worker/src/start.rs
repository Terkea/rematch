//! Starting the worker (port of `apps/worker/src/start-workers.ts`, plus the R2 profile engine).
//!
//! Order: check the engine is Stockfish 19, adopt a Drizzle-migrated database and migrate it
//! (on a separate pool without a statement timeout), connect the worker pool, start the engine
//! pool, the optional tablebase probe (`SYZYGY_PATH`) and the Maia client (`MAIA_URL`, lazy),
//! warm the opening dataset, then one runner per job kind, schedule the recurring jobs, requeue
//! the games without the current enrichment once and resume lost reference-game jobs
//! (ADR-R2-25). With AI (R3, [`crate::ai_jobs`]) it also loads the price table before anything
//! else (a bad table fails fast), picks the AI client, spawns one runner per AI kind and schedules
//! the recurring AI jobs with the reconciler. Whatever was created is closed again if a later step
//! fails. Every runner is registered through one `Spawned::spawn`, so
//! [`RunningWorkers::claims`] lists them all and shutdown drains them all.
//!
//! Runners (kind: concurrency; every kind but `analyse-game` keeps R1's [`PriorityFifo`] claim):
//! - `import-account`: 2;
//! - `analyse-game`: `pool_size`, the fair round robin (ADR-R2-15), with the profile hook
//!   (ADR-R2-24);
//! - `schedule-analysis`: 2; `preview-game`: `max(1, pool_size / 2)`; `build-profile`: 2;
//! - `analyse-reference-game`: 1. It shares the engine pool with the analyses and runs at
//!   priority 200, below every user job;
//! - `difficulty-sweep` and `reference-difficulty-sweep`: 1 each;
//! - the 9 AI kinds ([`AI_RUNNER_KINDS`](crate::ai_jobs::AI_RUNNER_KINDS)), unless
//!   [`AiSetup::None`];
//! - `reconcile`: 1 (optional);
//! - `sync-accounts`: 1, with the reconciler (M5b Task S, ADR 0076): every `SYNC_INTERVAL_MINUTES`
//!   each linked account gets an incremental import, spread across the interval.
//!
//! Connection budget: `max_connections = pool_size + preview + POOL_HEADROOM` (plus
//! [`AI_POOL_HEADROOM`](crate::ai_jobs::AI_POOL_HEADROOM) with the AI runners, R3). The analyse
//! runner holds at most one connection per in-flight game, each preview one. The headroom covers
//! each in-flight import's account-lock connection plus one pooled connection (4), the reconciler
//! (1), the schedule-analysis transactions (2), the build-profile jobs (2 each: the transaction and
//! the repertoire's explorer read, 4), the reference game and its finaliser (2), the two sweeps (2),
//! the sync tick and its listener (2, M5b Task S) and each R1/R2 runner's NOTIFY listener (9). The AI headroom covers the AI runners' listeners
//! and jobs. Every statement is short and the import's busy checks open short-lived connections,
//! so the pool's acquire timeout (10 s) is the bound.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use analysis::eval_cache::DbEvalCache;
use analysis::jobs::analyse::{AnalyseGameDeps, AnalyseGameHandler};
use analysis::jobs::import::{ImportAccountDeps, ImportAccountHandler};
use analysis::jobs::reconcile::{ReconcileHandler, ensure_reconcile_scheduled};
use analysis::jobs::sync::{SyncAccountsHandler, ensure_sync_scheduled};
use anyhow::Context;
use db::pool::DbOptions;
use engine::pool::{EnginePool, PoolOptions};
use engine::version::stockfish_version;
use engine::{ANALYSIS_PROTOCOL, Analyser};
use enrich::difficulty::DifficultySource;
use enrich::difficulty::select::{DifficultyEstimator, select_difficulty_provider};
use enrich::enrich_game::EnrichDeps;
use enrich::store::DbEnrichmentStore;
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
use profile::build_profile::{BuildProfileDeps, BuildProfileHandler};
use profile::hook_impl::{ProfileHook, into_hook, warm_openings};
use profile::reference_jobs::{
    ReferenceDifficultySweepHandler, ReferenceGameDeps, ReferenceGameHandler,
    ensure_reference_difficulty_sweep_scheduled,
};
use profile::requeue::{
    DifficultySweepDeps, DifficultySweepHandler, ensure_difficulty_sweep_scheduled,
    mark_stale_metrics, requeue_colour_split, requeue_unenriched,
};
use queue::queue::Queue;
use queue::runner::{Handler, Runner, RunnerConfig, RunnerHandle};
use queue::strategy::{PriorityFifo, SelectStrategy};
use scheduler::fair::FairRoundRobin;
use scheduler::preview::{PreviewDeps, PreviewHandler};
use scheduler::schedule_job::{ScheduleDeps, ScheduleHandler};
use scheduler::select::Plan;
use sqlx::PgPool;
use tokio::task::AbortHandle;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::ai_jobs::{
    AI_POOL_HEADROOM, AiRunnerDeps, AiSetup, ProfileInputProvider, ai_client_from_env, ai_runners,
    pricing_from_env,
};
use crate::config::WorkerEnv;
use crate::shutdown::{Closable, Drainable, ShutdownHandles};
use ai::jobs::providers::CoachingInputProvider;

/// Imports at once (each holds an account-lock connection).
pub const IMPORT_CONCURRENCY: usize = 2;
/// Connections beyond one per engine and one per preview (see the module docs).
pub const POOL_HEADROOM: usize = 26;
/// Schedule-analysis jobs at once (each holds one connection for its transaction).
pub const SCHEDULE_CONCURRENCY: usize = 2;
/// The runner's hard deadline on a schedule-analysis job (an account of 10k games takes well
/// under a second).
pub const SCHEDULE_HARD_TIMEOUT: Duration = Duration::from_secs(120);
/// Preview passes at once for `pool_size` engines: `max(1, pool_size / 2)` (each evaluates two
/// positions at a time, ADR-R2-15).
pub fn preview_concurrency(pool_size: usize) -> usize {
    (pool_size / 2).max(1)
}
/// Profile builds at once (each holds two connections, Task 18's repertoire read).
pub const BUILD_PROFILE_CONCURRENCY: usize = 2;
/// The runner's hard deadline on a profile build.
pub const BUILD_PROFILE_HARD_TIMEOUT: Duration = Duration::from_secs(10 * 60);
/// Reference games at once. They share the engine pool with the user's analyses, so one at a
/// time (four engine requests each) keeps user latency first (ADR-R2-25).
pub const REFERENCE_CONCURRENCY: usize = 1;
/// The runner's hard deadline on a sweep run (`difficulty-sweep`, `reference-difficulty-sweep`).
pub const SWEEP_HARD_TIMEOUT: Duration = Duration::from_secs(10 * 60);
/// The runner's hard deadline on a preview pass.
pub const PREVIEW_HARD_TIMEOUT: Duration = Duration::from_secs(300);
/// The runner's hard deadline on an analysis: past the handler's own 10-minute deadline, so the
/// handler normally ends it and records the game itself.
pub const ANALYSE_HARD_TIMEOUT: Duration = Duration::from_secs(11 * 60);
/// The runner's hard deadline on a reconcile sweep.
pub const RECONCILE_HARD_TIMEOUT: Duration = Duration::from_secs(5 * 60);
/// How long the boot-time version probe may take.
pub const VERSION_TIMEOUT: Duration = Duration::from_secs(10);
/// How often the account-busy counter is checked and logged.
const BUSY_LOG_EVERY: Duration = Duration::from_secs(300);

#[derive(Clone)]
pub struct StartOptions {
    /// Overrides `WORKER_POOL_SIZE`.
    pub pool_size: Option<usize>,
    /// Runner timings and `worker_id` shared by every runner (concurrency and hard timeout are
    /// set per kind). Default: [`RunnerConfig::default`].
    pub runner: Option<RunnerConfig>,
    /// Run the reconcile runner and schedule the recurring sweep (and the recurring AI jobs).
    pub with_reconciler: bool,
    /// The AI client ([`AiSetup::FromEnv`] by default; [`AiSetup::None`] spawns no AI runner).
    pub ai: AiSetup,
    /// Overrides the coaching-input provider (default: R2's builder).
    pub inputs: Option<Arc<dyn CoachingInputProvider>>,
    /// Set the profile hook on the analyse runner (default). `false` runs R1's analysis alone:
    /// only for measuring analysis throughput (`cli analyse` with `CLI_ANALYSIS_ONLY=1`); the
    /// games it writes are unenriched, so the next normal start requeues them.
    pub profile_hook: bool,
}

impl std::fmt::Debug for StartOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StartOptions")
            .field("pool_size", &self.pool_size)
            .field("runner", &self.runner)
            .field("with_reconciler", &self.with_reconciler)
            .field("ai", &self.ai)
            .field("inputs", &self.inputs.as_ref().map(|_| "custom"))
            .field("profile_hook", &self.profile_hook)
            .finish()
    }
}

impl Default for StartOptions {
    fn default() -> Self {
        StartOptions {
            pool_size: None,
            runner: None,
            with_reconciler: true,
            ai: AiSetup::FromEnv,
            inputs: None,
            profile_hook: true,
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

/// The analyse runner claims with the fair round robin (ADR-R2-15): `analyse-game` is the only
/// kind whose handler records `analysis_started_at`, which the round robin counts as served.
pub fn analyse_runner_config(base: &RunnerConfig, concurrency: usize) -> RunnerConfig {
    RunnerConfig {
        concurrency: concurrency.max(1),
        hard_timeout: Some(ANALYSE_HARD_TIMEOUT),
        strategy: Arc::new(FairRoundRobin::default()),
        ..base.clone()
    }
}

/// No hard timeout: the import handler owns its idle deadline (an import of a large account can
/// legitimately run for a long time while it makes progress). Every runner but the analyse one
/// keeps R1's claim ([`PriorityFifo`]).
pub fn import_runner_config(base: &RunnerConfig) -> RunnerConfig {
    RunnerConfig {
        concurrency: IMPORT_CONCURRENCY,
        hard_timeout: None,
        strategy: Arc::new(PriorityFifo),
        ..base.clone()
    }
}

pub fn reconcile_runner_config(base: &RunnerConfig) -> RunnerConfig {
    RunnerConfig {
        concurrency: 1,
        hard_timeout: Some(RECONCILE_HARD_TIMEOUT),
        strategy: Arc::new(PriorityFifo),
        ..base.clone()
    }
}

pub fn schedule_runner_config(base: &RunnerConfig) -> RunnerConfig {
    RunnerConfig {
        concurrency: SCHEDULE_CONCURRENCY,
        hard_timeout: Some(SCHEDULE_HARD_TIMEOUT),
        strategy: Arc::new(PriorityFifo),
        ..base.clone()
    }
}

pub fn preview_runner_config(base: &RunnerConfig, pool_size: usize) -> RunnerConfig {
    RunnerConfig {
        concurrency: preview_concurrency(pool_size),
        hard_timeout: Some(PREVIEW_HARD_TIMEOUT),
        strategy: Arc::new(PriorityFifo),
        ..base.clone()
    }
}

pub fn build_profile_runner_config(base: &RunnerConfig) -> RunnerConfig {
    RunnerConfig {
        concurrency: BUILD_PROFILE_CONCURRENCY,
        hard_timeout: Some(BUILD_PROFILE_HARD_TIMEOUT),
        strategy: Arc::new(PriorityFifo),
        ..base.clone()
    }
}

/// The reference runner keeps [`PriorityFifo`]: the fair round robin only serves kinds that set
/// `games.analysis_started_at` (ADR-R2-15), and reference games are not user games.
pub fn reference_runner_config(base: &RunnerConfig) -> RunnerConfig {
    RunnerConfig {
        concurrency: REFERENCE_CONCURRENCY,
        hard_timeout: Some(ANALYSE_HARD_TIMEOUT),
        strategy: Arc::new(PriorityFifo),
        ..base.clone()
    }
}

pub fn sweep_runner_config(base: &RunnerConfig) -> RunnerConfig {
    RunnerConfig {
        concurrency: 1,
        hard_timeout: Some(SWEEP_HARD_TIMEOUT),
        strategy: Arc::new(PriorityFifo),
        ..base.clone()
    }
}

/// One runner as started: its kind, how many jobs it runs at once and how it claims them.
#[derive(Clone)]
pub struct RunnerClaim {
    pub kind: &'static str,
    pub concurrency: usize,
    pub strategy: Arc<dyn SelectStrategy>,
}

impl std::fmt::Debug for RunnerClaim {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunnerClaim")
            .field("kind", &self.kind)
            .field("concurrency", &self.concurrency)
            .field("strategy", &self.strategy.order_by())
            .finish()
    }
}

/// The runners started so far, with their claims.
#[derive(Default)]
struct Spawned {
    runners: Vec<RunnerHandle>,
    claims: Vec<RunnerClaim>,
}

impl Spawned {
    fn spawn(&mut self, queue: &Queue, handler: Arc<dyn Handler>, cfg: RunnerConfig) {
        self.claims.push(RunnerClaim {
            kind: handler.kind(),
            concurrency: cfg.concurrency,
            strategy: cfg.strategy.clone(),
        });
        self.runners
            .push(Runner::spawn(queue.clone(), handler, cfg));
    }
}

pub fn schedule_deps(pool: PgPool, queue: Queue, default_plan: Plan) -> ScheduleDeps {
    ScheduleDeps {
        pool,
        queue,
        now: time::OffsetDateTime::now_utc,
        default_plan,
    }
}

/// The enrichment's dependencies (R2 Tasks 23-25): the main engines, the Postgres caches, the
/// tablebase probe when `SYZYGY_PATH` gave one, and Maia only when `MAIA_URL` is set (without
/// it, difficulty is `pending_maia`, never estimated). Each hook and job replaces `cancel` with
/// its own run token.
pub fn enrich_deps(
    analyser: Arc<dyn Analyser>,
    pool: &PgPool,
    tablebase: Option<&WorkerTablebase>,
    difficulty: &Arc<DifficultyEstimator>,
) -> EnrichDeps {
    EnrichDeps {
        analyser,
        store: Arc::new(DbEnrichmentStore::new(pool.clone())),
        tb: tablebase.map(|tb| tb.probe.clone()),
        difficulty: difficulty
            .is_configured()
            .then(|| difficulty.clone() as Arc<dyn DifficultySource>),
        cancel: CancellationToken::new(),
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
    /// The directory the engines read: the staged copy when `SYZYGY_STAGE_DIR` is set and the
    /// copy succeeded, else `SYZYGY_PATH`.
    pub path: std::path::PathBuf,
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
    let Some(source) = env.syzygy_path.clone() else {
        tracing::info!("SYZYGY_PATH is not set; no tablebase probe");
        return None;
    };
    let path = match env.syzygy_stage_dir.clone() {
        None => source.clone(),
        Some(stage) => {
            let (src, dst) = (source.clone(), stage.clone());
            let staged =
                tokio::task::spawn_blocking(move || crate::syzygy_stage::stage_tables(&src, &dst))
                    .await;
            match staged {
                Ok(Ok(r)) => {
                    tracing::info!(
                        from = %source.display(),
                        to = %stage.display(),
                        copied = r.copied,
                        current = r.current,
                        bytes = r.bytes,
                        "syzygy tables staged"
                    );
                    stage
                }
                Ok(Err(e)) => {
                    tracing::warn!(error = %e, "staging the syzygy tables failed; reading SYZYGY_PATH");
                    source.clone()
                }
                Err(e) => {
                    tracing::warn!(error = %e, "staging the syzygy tables failed; reading SYZYGY_PATH");
                    source.clone()
                }
            }
        }
    };
    let staged = path != source;
    match start_syzygy(env, &path, pool).await {
        Some(tb) if staged && !canary_ok(&tb).await => {
            // Belt and braces: a staged table that passed the stage's checks but answers wrongly
            // is never used, and its answer was never cached (the canary bypasses the cache).
            tracing::error!(
                stage = %path.display(),
                "a staged syzygy table failed the canary probe; reading SYZYGY_PATH"
            );
            tb.pool.close().await;
            start_syzygy(env, &source, pool).await
        }
        other => other,
    }
}

/// Known tablebase facts, checked on the raw pool (never through the `position_tb` cache) when
/// the engines read a staged copy: KQvK and KRvK are wins for the side with the piece.
const CANARIES: [(&str, &str, i8); 2] = [
    ("KQvK", "8/8/8/8/8/4k3/8/KQ6 w - - 0 1", 1),
    ("KRvK", "8/8/8/8/8/4k3/8/KR6 w - - 0 1", 1),
];

async fn canary_ok(tb: &WorkerTablebase) -> bool {
    for (table, fen, want) in CANARIES {
        if !tb.path.join(format!("{table}.rtbw")).is_file() {
            continue;
        }
        let got = tb.pool.probe(fen).await;
        if got != Some(want) {
            tracing::error!(table, ?got, want, "syzygy canary probe failed");
            return false;
        }
    }
    true
}

async fn start_syzygy(
    env: &WorkerEnv,
    path: &std::path::Path,
    pool: &PgPool,
) -> Option<WorkerTablebase> {
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
                path: path.to_path_buf(),
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
    /// One per kind (see the module docs); `claims[i]` describes `runners[i]`.
    pub runners: Vec<RunnerHandle>,
    pub claims: Vec<RunnerClaim>,
    /// `MAIA_URL` is set (the client connects lazily; see [`select_difficulty_provider`]).
    pub maia_configured: bool,
    pub stockfish_version: String,
    /// `None` when `SYZYGY_PATH` is unset (see [`start_tablebase`]).
    pub tablebase: Option<WorkerTablebase>,
    /// Imports released because another handler held the account (`account_busy`).
    pub busy_releases: Arc<AtomicU64>,
    /// Whether the AI runners have a client (see [`crate::ai_jobs`]).
    pub ai_enabled: bool,
    monitor: AbortHandle,
    /// Keeps this worker's `ai_status` row fresh while it has AI runners ([`crate::ai_status`]).
    ai_heartbeat: Option<AbortHandle>,
}

impl RunningWorkers {
    /// The job kind of each runner, in spawn order.
    pub fn runner_kinds(&self) -> Vec<&'static str> {
        self.runners.iter().map(RunnerHandle::kind).collect()
    }

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
                ai_heartbeat: self.ai_heartbeat,
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

/// Closes the pool, after stopping the stats monitor and the AI status heartbeat.
struct DbClose {
    pool: PgPool,
    monitor: AbortHandle,
    ai_heartbeat: Option<AbortHandle>,
}

impl Closable for DbClose {
    fn close(&self) -> BoxFuture<'_, anyhow::Result<()>> {
        self.monitor.abort();
        if let Some(h) = &self.ai_heartbeat {
            h.abort();
        }
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
    let with_ai = !matches!(opts.ai, AiSetup::None);
    let pricing = if with_ai {
        Some(Arc::new(pricing_from_env(env)?))
    } else {
        None
    };
    let max_connections = pool_size
        + preview_concurrency(pool_size)
        + POOL_HEADROOM
        + if with_ai { AI_POOL_HEADROOM } else { 0 };

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
            max_connections: u32::try_from(max_connections).unwrap_or(u32::MAX),
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
    let tablebase = start_tablebase(env, &pool).await;
    let difficulty = Arc::new(select_difficulty_provider(env.maia_url.as_deref()));
    let maia_configured = difficulty.is_configured();
    warm_openings().await;

    let import = import_deps(env, &pool, &queue);
    let busy_releases = import.busy_releases.clone();
    let analyser: Arc<dyn Analyser> = engines.clone();
    let ai_choice = match &opts.ai {
        AiSetup::None => None,
        AiSetup::With(client) => Some(Ok(crate::ai_jobs::AiClientChoice {
            client: client.clone(),
            line: if client.is_some() {
                "ai: enabled (injected client)".to_owned()
            } else {
                "ai: disabled (no client)".to_owned()
            },
        })),
        AiSetup::FromEnv => Some(ai_client_from_env(env).await),
    };
    // The startup line, recorded in `ai_status` for the web (bug hunt I5); never a credential.
    let mut ai_line: Option<String> = None;
    let ai_client = match ai_choice {
        None => None,
        Some(Ok(choice)) => {
            tracing::info!("{}", choice.line);
            ai_line = Some(choice.line.clone());
            // Which path the monthly and weekly schedules take (ADR 0075); no credential here.
            if let Some(client) = &choice.client {
                tracing::info!(
                    "ai: scheduled regenerations via {}",
                    if client.batches_enabled() {
                        "batches"
                    } else {
                        "sync jobs (sidecar)"
                    }
                );
            }
            Some(choice.client)
        }
        Some(Err(e)) => {
            if let Some(tb) = &tablebase {
                tb.pool.close().await;
            }
            EnginePool::close(&engines).await;
            pool.close().await;
            return Err(e.context("starting the AI client"));
        }
    };
    let ai_enabled = matches!(ai_client, Some(Some(_)));
    let cache = Arc::new(DbEvalCache::new(pool.clone()));
    let preview = PreviewDeps {
        pool: pool.clone(),
        analyser: analyser.clone(),
        concurrency: 2,
    };
    let mut analyse = AnalyseGameDeps::new(pool.clone(), analyser.clone(), cache.clone());
    if opts.profile_hook {
        analyse.hook = Some(into_hook(ProfileHook {
            pool: pool.clone(),
            enrich: enrich_deps(analyser.clone(), &pool, tablebase.as_ref(), &difficulty),
            queue: queue.clone(),
        }));
    }
    let reference = Arc::new(ReferenceGameDeps {
        pool: pool.clone(),
        analyser: analyser.clone(),
        cache: cache.clone(),
        enrich: enrich_deps(analyser.clone(), &pool, tablebase.as_ref(), &difficulty),
    });
    let sweep = DifficultySweepDeps {
        pool: pool.clone(),
        queue: queue.clone(),
        analyser: analyser.clone(),
        cache,
        source: difficulty.clone(),
        options: enrich::reenrich::SweepOptions::default(),
    };

    let mut s = Spawned::default();
    s.spawn(
        &queue,
        Arc::new(ImportAccountHandler::new(Arc::new(import))),
        import_runner_config(&base),
    );
    s.spawn(
        &queue,
        Arc::new(AnalyseGameHandler(Arc::new(analyse))),
        analyse_runner_config(&base, pool_size),
    );
    s.spawn(
        &queue,
        Arc::new(ScheduleHandler(Arc::new(schedule_deps(
            pool.clone(),
            queue.clone(),
            env.plan_default,
        )))),
        schedule_runner_config(&base),
    );
    s.spawn(
        &queue,
        Arc::new(PreviewHandler(Arc::new(preview))),
        preview_runner_config(&base, pool_size),
    );
    let ai_entitlements: Arc<dyn ai::entitlements::Entitlements> =
        Arc::new(crate::ai_jobs::ai_entitlements(env));
    let build_profile: Arc<dyn queue::runner::Handler> =
        Arc::new(BuildProfileHandler(Arc::new(BuildProfileDeps {
            pool: pool.clone(),
            queue: queue.clone(),
            now: time::OffsetDateTime::now_utc,
        })));
    // With the AI runners, a completed build also requests the first report (M5b ruling G3).
    let build_profile: Arc<dyn queue::runner::Handler> = if with_ai {
        Arc::new(crate::coach_triggers::FirstReportOnBuild {
            inner: build_profile,
            pool: pool.clone(),
            queue: queue.clone(),
            entitlements: ai_entitlements.clone(),
        })
    } else {
        build_profile
    };
    s.spawn(&queue, build_profile, build_profile_runner_config(&base));
    s.spawn(
        &queue,
        Arc::new(ReferenceGameHandler(reference.clone())),
        reference_runner_config(&base),
    );
    s.spawn(
        &queue,
        Arc::new(DifficultySweepHandler(Arc::new(sweep))),
        sweep_runner_config(&base),
    );
    s.spawn(
        &queue,
        Arc::new(ReferenceDifficultySweepHandler(reference)),
        sweep_runner_config(&base),
    );
    if let (Some(client), Some(pricing)) = (ai_client, pricing) {
        let inputs = opts.inputs.clone().unwrap_or_else(|| {
            Arc::new(ProfileInputProvider { pool: pool.clone() }) as Arc<dyn CoachingInputProvider>
        });
        let deps = AiRunnerDeps {
            pool: pool.clone(),
            queue: queue.clone(),
            ai: client,
            analyser: analyser.clone(),
            inputs,
            budgets: env.ai_budgets,
            pricing,
            entitlements: ai_entitlements.clone(),
            weekly: Arc::new(crate::coach_triggers::ProfileWeeklyProvider { pool: pool.clone() }),
        };
        for (handler, cfg) in ai_runners(&deps, &base) {
            s.spawn(&queue, handler, cfg);
        }
    }
    if opts.with_reconciler {
        s.spawn(
            &queue,
            Arc::new(ReconcileHandler {
                pool: pool.clone(),
                queue: queue.clone(),
            }),
            reconcile_runner_config(&base),
        );
        s.spawn(
            &queue,
            Arc::new(SyncAccountsHandler {
                pool: pool.clone(),
                queue: queue.clone(),
                opts: env.sync_options(),
            }),
            reconcile_runner_config(&base),
        );
    }
    let scheduled = async {
        if opts.with_reconciler {
            ensure_reconcile_scheduled(&queue)
                .await
                .context("scheduling the reconciler")?;
            ensure_sync_scheduled(&queue, &env.sync_options())
                .await
                .context("scheduling the account sync")?;
            if with_ai {
                ai::jobs::kinds::ensure_ai_recurring(&queue)
                    .await
                    .context("scheduling the recurring AI jobs")?;
            }
        }
        ensure_difficulty_sweep_scheduled(&queue)
            .await
            .context("scheduling the difficulty sweep")?;
        ensure_reference_difficulty_sweep_scheduled(&queue)
            .await
            .context("scheduling the reference difficulty sweep")?;
        anyhow::Ok(())
    }
    .await;
    if let Err(e) = scheduled {
        for runner in &s.runners {
            runner.force().await;
        }
        if let Some(tb) = &tablebase {
            tb.pool.close().await;
        }
        EnginePool::close(&engines).await;
        pool.close().await;
        return Err(e);
    }
    // Games analysed before the hook existed, or whose hook failed (ADR-R2-24). Not fatal: the
    // next start tries again.
    match requeue_unenriched(&pool, &queue).await {
        Ok(n) => tracing::info!(requeued = n, "unenriched games requeued"),
        Err(e) => tracing::warn!(error = %e, "requeueing unenriched games failed"),
    }
    // Games whose metrics an older PLAYER_GAME_METRICS_VERSION wrote: the difficulty sweep
    // recomputes them from the stored evals (ADR-FIX-C-1). Not fatal: the next start tries again.
    match mark_stale_metrics(&pool).await {
        Ok(n) => tracing::info!(marked = n, "games with stale metrics marked"),
        Err(e) => tracing::warn!(error = %e, "marking games with stale metrics failed"),
    }
    // Profiles built before the colour split get their White/Black rows from one rebuild over
    // the stored metrics (no engine). Not fatal: the next start tries again.
    match requeue_colour_split(&pool, &queue).await {
        Ok(n) => tracing::info!(due = n, "profiles without colour rows queued"),
        Err(e) => tracing::warn!(error = %e, "queueing colour-split rebuilds failed"),
    }
    // Reference-game jobs lost by a crash of `lichess-db build` between its row insert and its
    // last job batch (T21 review I4; idempotent through the `refgame:<id>` dedupe). Not fatal.
    match lichess_db::reference_select::resume_all_reference_enqueues(&pool, &queue).await {
        Ok(resumed) => {
            for (build, added) in resumed.into_iter().filter(|(_, n)| *n > 0) {
                tracing::info!(%build, added, "reference-game jobs resumed");
            }
        }
        Err(e) => tracing::warn!(error = %e, "resuming reference-game jobs failed"),
    }
    // Whether the coach can write, for the web (bug hunt I5, ADR 0078). Not fatal: without the
    // row the web says the coach is not available, which is the safe side.
    let ai_heartbeat = match &ai_line {
        Some(line) => {
            let worker_id = Uuid::new_v4();
            if let Err(e) = crate::ai_status::record(&pool, worker_id, ai_enabled, line).await {
                tracing::warn!(error = %e, "recording the ai status failed");
            }
            Some(tokio::spawn(crate::ai_status::heartbeat(pool.clone(), worker_id)).abort_handle())
        }
        None => None,
    };
    let Spawned { runners, claims } = s;
    let monitor = tokio::spawn(log_busy_releases(busy_releases.clone())).abort_handle();
    tracing::info!(
        stockfish_version = %stockfish_version,
        pool_size,
        max_connections,
        runners = runners.len(),
        tablebase = tablebase.as_ref().map(|tb| tb.status().as_str()),
        maia_configured,
        default_plan = ?env.plan_default,
        ai_enabled,
        "worker ready"
    );
    Ok(RunningWorkers {
        pool,
        queue,
        engines,
        runners,
        claims,
        maia_configured,
        stockfish_version,
        tablebase,
        busy_releases,
        ai_enabled,
        monitor,
        ai_heartbeat,
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
    use queue::strategy::SelectStrategy;

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
        // Only the analyse runner claims fairly (ADR-R2-15); every other kind keeps R1's claim.
        let fifo = PriorityFifo.order_by();
        assert_eq!(a.strategy.order_by(), FairRoundRobin::default().order_by());
        assert_ne!(a.strategy.order_by(), fifo);
        let s = schedule_runner_config(&base);
        assert_eq!(
            (s.concurrency, s.hard_timeout),
            (SCHEDULE_CONCURRENCY, Some(SCHEDULE_HARD_TIMEOUT))
        );
        let p = preview_runner_config(&base, 4);
        assert_eq!(
            (p.concurrency, p.hard_timeout),
            (2, Some(PREVIEW_HARD_TIMEOUT))
        );
        for (pool, want) in [(1, 1), (2, 1), (3, 1), (8, 4)] {
            assert_eq!(preview_concurrency(pool), want, "{pool} engines");
        }
        let b = build_profile_runner_config(&base);
        assert_eq!(
            (b.concurrency, b.hard_timeout),
            (2, Some(BUILD_PROFILE_HARD_TIMEOUT))
        );
        let g = reference_runner_config(&base);
        assert_eq!(
            (g.concurrency, g.hard_timeout),
            (1, Some(ANALYSE_HARD_TIMEOUT))
        );
        let w = sweep_runner_config(&base);
        assert_eq!(
            (w.concurrency, w.hard_timeout),
            (1, Some(SWEEP_HARD_TIMEOUT))
        );
        for c in [&i, &r, &s, &p, &b, &g, &w] {
            assert_eq!(c.strategy.order_by(), fifo);
        }
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

//! The resumable import-account job (port of `apps/worker/src/jobs/import-account.ts`).
//!
//! An import streams an account's games newest first and resumes from
//! `linked_accounts.import_cursor`. A pass streams `(since: newestSeenAt − look-back,
//! until: oldestReached)` and checkpoints `oldestReached` (the oldest game committed so far, the
//! *frontier*) every [`CHECKPOINT_EVERY`] games or [`CHECKPOINT_INTERVAL`], whichever comes
//! first. When a pass finishes, `newestSeenAt` becomes the newest game stored and
//! `oldestReached` is cleared. A run first finishes any interrupted pass, then fetches games newer
//! than everything stored. Upserts make overlaps harmless, and every upserted game that is still
//! `pending` is (re-)enqueued for analysis; the live-only dedupe key keeps one job per game.
//!
//! Changes from the TS reference (ADRs 0032, 0035):
//! - a run whose account's newest run is `paused` reuses that row instead of inserting one;
//! - each `next()` races an idle [`Deadline`] (reset after every game) and the shutdown token;
//! - an open breaker or a shutdown pauses the run and releases the job without using an attempt;
//! - a user-level `not_found` (the chess.com archive list or the Lichess user stream) is
//!   unrecoverable: the run fails at once and `job.failed` is published. A 404 on one chess.com
//!   monthly archive stays retryable (`unknown`);
//! - one handler per account at a time (a session advisory lock): a handler that lost its lease
//!   and is still running makes the re-claimed job release (`account_busy`) instead of
//!   interleaving cursor and watermark writes with it.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use db::accounts::{AccountRow, ImportCursor, get_account};
use db::games::{AnalysisStatus, NewGame, upsert_game};
use db::import_runs::{ImportErrorCode, ImportRunStatus};
use events::store::{UserEventType, publish};
use futures::future::BoxFuture;
use futures::{FutureExt, StreamExt};
use platforms::breaker::{CircuitBreaker, with_circuit_breaker};
use platforms::{
    FetchOptions, GameSource, NormalizedGame, Platform, PlatformClient, PlatformError,
};
use queue::queue::{Queue, QueueError};
use queue::{
    EnqueueOptions, ImportAccountPayload, Job, JobError, KIND_ANALYSE, KIND_IMPORT, analyse_key,
    import_key,
};
use serde_json::{Value, json};
use sqlx::pool::PoolConnection;
use sqlx::{PgPool, Postgres};
use time::OffsetDateTime;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::deadline::{Deadline, IMPORT_IDLE_DEADLINE, JobDeadlineError};

/// Checkpoint the cursor and the run after this many games...
pub const CHECKPOINT_EVERY: usize = 50;
/// ...or after this long since the last checkpoint, whichever comes first.
pub const CHECKPOINT_INTERVAL: Duration = Duration::from_secs(15);
/// Lichess lists a game by its creation time, so a correspondence game that finishes after a
/// newer one was imported would sit below the watermark for ever. Incremental Lichess passes
/// therefore re-fetch this far below `newestSeenAt`.
pub const LICHESS_SINCE_LOOKBACK: Duration = Duration::from_secs(30 * 24 * 3600);
/// The shortest breaker re-delay.
pub const BREAKER_MIN_DELAY: Duration = Duration::from_secs(5);
/// The longest a half-open probe can take (the platform request timeout).
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(30);
/// How long an import waits before retrying when another handler holds the account.
pub const ACCOUNT_BUSY_DELAY: Duration = Duration::from_secs(30);
/// Advisory-lock namespace (`classid`) of the per-account import lock. The two-key form never
/// conflicts with the single-key locks used elsewhere (the events lock 7262011).
const IMPORT_LOCK_NAMESPACE: i32 = 7_262_021;
/// Max attempts of an import job.
pub const IMPORT_MAX_ATTEMPTS: i32 = 3;
/// `import_runs.last_error` keeps at most this many characters.
const MAX_ERROR_CHARS: usize = 2000;

pub struct ImportAccountDeps {
    pub pool: PgPool,
    pub queue: Queue,
    pub sources: HashMap<Platform, Arc<dyn GameSource>>,
    pub clients: HashMap<Platform, Arc<dyn PlatformClient>>,
    /// Per-platform breakers, shared across jobs; platform calls go through them.
    pub breakers: HashMap<Platform, Arc<CircuitBreaker>>,
    pub checkpoint_every: usize,
    pub checkpoint_interval: Duration,
    /// Abandon the run after this long without a streamed game.
    pub idle_deadline: Duration,
    pub lichess_lookback: Duration,
    pub breaker_min_delay: Duration,
    pub probe_timeout: Duration,
}

impl ImportAccountDeps {
    /// No sources, clients or breakers, and the production defaults for everything else.
    pub fn new(pool: PgPool, queue: Queue) -> Self {
        ImportAccountDeps {
            pool,
            queue,
            sources: HashMap::new(),
            clients: HashMap::new(),
            breakers: HashMap::new(),
            checkpoint_every: CHECKPOINT_EVERY,
            checkpoint_interval: CHECKPOINT_INTERVAL,
            idle_deadline: IMPORT_IDLE_DEADLINE,
            lichess_lookback: LICHESS_SINCE_LOOKBACK,
            breaker_min_delay: BREAKER_MIN_DELAY,
            probe_timeout: PROBE_TIMEOUT,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct ImportSummary {
    /// Games this call inserted.
    pub imported: u32,
    /// Streamed games in which the account's user did not play.
    pub skipped: u32,
}

/// Why an import run stopped early.
#[derive(thiserror::Error, Debug)]
pub enum ImportFailure {
    #[error(transparent)]
    Platform(#[from] PlatformError),
    #[error(transparent)]
    Deadline(#[from] JobDeadlineError),
    #[error("import interrupted by shutdown")]
    Shutdown,
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),
    #[error(transparent)]
    Queue(#[from] QueueError),
    #[error("{0}")]
    Config(String),
}

/// Maps a failure to the stable code stored on `import_runs.error_code`.
pub fn import_error_code(err: &ImportFailure) -> ImportErrorCode {
    match err {
        ImportFailure::Platform(PlatformError::NotFound { url }) if is_user_not_found(url) => {
            ImportErrorCode::NotFound
        }
        ImportFailure::Platform(PlatformError::RateLimited { .. }) => ImportErrorCode::RateLimited,
        ImportFailure::Platform(PlatformError::Timeout { .. }) | ImportFailure::Deadline(_) => {
            ImportErrorCode::Timeout
        }
        ImportFailure::Platform(PlatformError::CircuitOpen { .. }) => ImportErrorCode::CircuitOpen,
        ImportFailure::Shutdown => ImportErrorCode::Shutdown,
        ImportFailure::Platform(_)
        | ImportFailure::Db(_)
        | ImportFailure::Queue(_)
        | ImportFailure::Config(_) => ImportErrorCode::Unknown,
    }
}

/// The re-delay of an import blocked by an open breaker: at least `min`, at least
/// `probe_timeout` when `retry_in` is zero (a half-open probe is in flight), plus up to 20 %
/// jitter (`jitter` in `[0, 1)`) so blocked imports do not stampede the probe.
pub fn breaker_redelay(
    retry_in: Duration,
    min: Duration,
    probe_timeout: Duration,
    jitter: f64,
) -> Duration {
    let base = if retry_in.is_zero() {
        min.max(probe_timeout)
    } else {
        retry_in.max(min)
    };
    let extra = base.as_nanos() as f64 * 0.2 * jitter.clamp(0.0, 1.0);
    base.saturating_add(Duration::from_nanos(extra as u64))
}

/// Enqueues an import of the account. `false` when a live import job already holds
/// `import:<account_id>` (one live import per account).
pub async fn enqueue_import(
    queue: &Queue,
    account_id: Uuid,
    max: Option<u32>,
) -> Result<bool, QueueError> {
    let payload = serde_json::to_value(ImportAccountPayload { account_id, max })
        .unwrap_or_else(|_| json!({ "accountId": account_id }));
    let id = queue
        .enqueue(
            KIND_IMPORT,
            &payload,
            EnqueueOptions {
                dedupe_key: Some(import_key(account_id)),
                user_key: Some(account_id),
                max_attempts: IMPORT_MAX_ATTEMPTS,
                ..EnqueueOptions::default()
            },
        )
        .await?;
    Ok(id.is_some())
}

/// The direct analyse enqueue (T22 switches this to T19's `enqueue_analyse`).
async fn enqueue_analysis(
    queue: &Queue,
    game_id: Uuid,
    account_id: Uuid,
) -> Result<(), QueueError> {
    queue
        .enqueue(
            KIND_ANALYSE,
            &json!({ "gameId": game_id }),
            EnqueueOptions {
                dedupe_key: Some(analyse_key(game_id)),
                user_key: Some(account_id),
                max_attempts: 3,
                ..EnqueueOptions::default()
            },
        )
        .await?;
    Ok(())
}

/// The queue handler for [`KIND_IMPORT`]. It has the shape of T14's `Handler`
/// (`kind`, `handle(job, ctx)`); the `impl Handler` delegates here once the runner lands.
pub struct ImportAccountHandler {
    pub deps: Arc<ImportAccountDeps>,
}

impl ImportAccountHandler {
    pub fn new(deps: Arc<ImportAccountDeps>) -> Self {
        ImportAccountHandler { deps }
    }

    pub fn kind(&self) -> &'static str {
        KIND_IMPORT
    }

    pub fn handle(
        &self,
        job: Job,
        cancel: CancellationToken,
    ) -> BoxFuture<'_, Result<Value, JobError>> {
        async move {
            let payload: ImportAccountPayload = serde_json::from_value(job.payload.clone())
                .map_err(|e| JobError::Unrecoverable {
                    message: format!("invalid import-account payload: {e}"),
                    code: None,
                })?;
            let summary = handle_import_account(&self.deps, &job, payload, cancel).await?;
            Ok(json!(summary))
        }
        .boxed()
    }
}

/// Imports the account's games, resuming from its cursor (see the module docs).
pub async fn handle_import_account(
    deps: &ImportAccountDeps,
    job: &Job,
    payload: ImportAccountPayload,
    cancel: CancellationToken,
) -> Result<ImportSummary, JobError> {
    let account = match get_account(&deps.pool, payload.account_id).await {
        Ok(Some(account)) => account,
        Ok(None) => {
            return Err(JobError::Unrecoverable {
                message: format!("Linked account not found: {}", payload.account_id),
                code: None,
            });
        }
        Err(e) => return Err(retryable(&ImportFailure::Db(e))),
    };
    // One handler per account at a time: a lease-lost handler still running must not
    // interleave its cursor writes (or its end-of-pass watermark) with the re-claimed job's.
    // The lock lives on a dedicated connection that is closed when dropped, so it is released
    // however this future ends (completion, error, or the runner dropping it).
    let _account_lock = match lock_account(&deps.pool, account.id).await {
        Ok(Some(conn)) => conn,
        Ok(None) => {
            return Err(JobError::Release {
                delay: ACCOUNT_BUSY_DELAY,
                reason: "account_busy".to_owned(),
            });
        }
        Err(e) => return Err(retryable(&ImportFailure::Db(e))),
    };
    let (run_id, base_imported) = open_run(&deps.pool, account.id)
        .await
        .map_err(|e| retryable(&ImportFailure::Db(e)))?;

    let mut run = Run {
        deps,
        cursor: account.import_cursor.clone().unwrap_or_default(),
        frontier: account
            .import_cursor
            .as_ref()
            .and_then(|c| c.oldest_reached),
        account,
        run_id,
        base_imported,
        max: payload.max,
        imported: 0,
        skipped: 0,
        since_checkpoint: 0,
        last_checkpoint: Instant::now(),
    };
    let deadline = Deadline::new(KIND_IMPORT, deps.idle_deadline);
    match run.run_passes(&deadline, &cancel).await {
        Ok(()) => {
            drop(deadline);
            run.finish()
                .await
                .map_err(|e| retryable(&ImportFailure::Db(e)))?;
            Ok(ImportSummary {
                imported: run.imported,
                skipped: run.skipped,
            })
        }
        Err(failure) => {
            drop(deadline);
            Err(run.fail(failure, job).await)
        }
    }
}

fn retryable(f: &ImportFailure) -> JobError {
    JobError::Retryable {
        message: truncate_chars(&f.to_string(), MAX_ERROR_CHARS),
        code: None,
    }
}

fn truncate_chars(s: &str, max: usize) -> String {
    match s.char_indices().nth(max) {
        Some((at, _)) => s[..at].to_owned(),
        None => s.to_owned(),
    }
}

/// True for a 404 that means the *user* does not exist: the chess.com archive list or the
/// Lichess user game stream. A 404 on one chess.com monthly archive is not (it stays retryable).
fn is_user_not_found(url: &str) -> bool {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    path.ends_with("/games/archives") || path.contains("/api/games/user/")
}

/// Folds the account id into the lock's `objid`. A collision only delays an import.
fn account_lock_key(account_id: Uuid) -> i32 {
    let v = account_id.as_u128();
    let folded = (v as u32) ^ ((v >> 32) as u32) ^ ((v >> 64) as u32) ^ ((v >> 96) as u32);
    i32::from_ne_bytes(folded.to_ne_bytes())
}

/// Takes the account's session advisory lock on a dedicated connection, or `None` when another
/// handler holds it. The connection is closed (not pooled) when dropped, which releases the lock.
async fn lock_account(
    pool: &PgPool,
    account_id: Uuid,
) -> Result<Option<PoolConnection<Postgres>>, sqlx::Error> {
    let mut conn = pool.acquire().await?;
    conn.close_on_drop();
    let locked: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1, $2)")
        .bind(IMPORT_LOCK_NAMESPACE)
        .bind(account_lock_key(account_id))
        .fetch_one(&mut *conn)
        .await?;
    Ok(locked.then_some(conn))
}

fn platform_of(s: &str) -> Option<Platform> {
    match s {
        "lichess" => Some(Platform::Lichess),
        "chesscom" => Some(Platform::Chesscom),
        _ => None,
    }
}

/// Reuses the account's newest run when it is `paused` (back to `running`, heartbeat now, error
/// cleared), otherwise inserts a new running run. Returns its id and its imported count so far.
async fn open_run(pool: &PgPool, account_id: Uuid) -> Result<(Uuid, i32), sqlx::Error> {
    let newest: Option<(Uuid, String)> = sqlx::query_as(
        "SELECT id, status::text FROM import_runs WHERE account_id = $1 \
         ORDER BY started_at DESC, id DESC LIMIT 1",
    )
    .bind(account_id)
    .fetch_optional(pool)
    .await?;
    if let Some((id, status)) = newest
        && status == ImportRunStatus::Paused.as_str()
    {
        let revived: Option<i32> = sqlx::query_scalar(
            "UPDATE import_runs SET status = 'running', heartbeat_at = now(), last_error = NULL, \
             error_code = NULL, finished_at = NULL \
             WHERE id = $1 AND status = 'paused' RETURNING imported",
        )
        .bind(id)
        .fetch_optional(pool)
        .await?;
        if let Some(imported) = revived {
            return Ok((id, imported));
        }
    }
    let id = sqlx::query_scalar(
        "INSERT INTO import_runs (account_id, status) VALUES ($1, 'running') RETURNING id",
    )
    .bind(account_id)
    .fetch_one(pool)
    .await?;
    Ok((id, 0))
}

/// One import run's state.
struct Run<'d> {
    deps: &'d ImportAccountDeps,
    account: AccountRow,
    run_id: Uuid,
    /// The run row's count before this call (non-zero for a reused paused run).
    base_imported: i32,
    max: Option<u32>,
    cursor: ImportCursor,
    /// The oldest game committed in the current pass.
    frontier: Option<OffsetDateTime>,
    imported: u32,
    skipped: u32,
    since_checkpoint: usize,
    last_checkpoint: Instant,
}

impl Run<'_> {
    fn platform(&self) -> Result<Platform, ImportFailure> {
        platform_of(&self.account.platform).ok_or_else(|| {
            ImportFailure::Config(format!("unknown platform {}", self.account.platform))
        })
    }

    fn run_imported(&self) -> i32 {
        self.base_imported
            .saturating_add(i32::try_from(self.imported).unwrap_or(i32::MAX))
    }

    /// The lower bound for a pass that starts at the watermark.
    fn since_of(&self, platform: Platform, newest_seen_at: OffsetDateTime) -> OffsetDateTime {
        let lookback = match platform {
            Platform::Lichess => self.deps.lichess_lookback,
            Platform::Chesscom => Duration::ZERO,
        };
        time::Duration::try_from(lookback)
            .ok()
            .and_then(|l| newest_seen_at.checked_sub(l))
            .unwrap_or(newest_seen_at)
    }

    async fn run_passes(
        &mut self,
        deadline: &Deadline,
        cancel: &CancellationToken,
    ) -> Result<(), ImportFailure> {
        let platform = self.platform()?;
        if let Some(oldest) = self.cursor.oldest_reached {
            let since = self
                .cursor
                .newest_seen_at
                .map(|n| self.since_of(platform, n));
            self.pass(platform, since, Some(oldest), deadline, cancel)
                .await?;
        }
        let since = self
            .cursor
            .newest_seen_at
            .map(|n| self.since_of(platform, n));
        self.pass(platform, since, None, deadline, cancel).await
    }

    async fn pass(
        &mut self,
        platform: Platform,
        since: Option<OffsetDateTime>,
        until: Option<OffsetDateTime>,
        deadline: &Deadline,
        cancel: &CancellationToken,
    ) -> Result<(), ImportFailure> {
        let deps = self.deps;
        let source = deps.sources.get(&platform).cloned().ok_or_else(|| {
            ImportFailure::Config(format!("no game source for {}", platform.as_str()))
        })?;
        let mut client =
            deps.clients.get(&platform).cloned().ok_or_else(|| {
                ImportFailure::Config(format!("no client for {}", platform.as_str()))
            })?;
        if let Some(breaker) = deps.breakers.get(&platform) {
            client = with_circuit_breaker(client, breaker.clone());
        }
        let username = self.account.username.clone();
        let opts = FetchOptions {
            since,
            until,
            max: self.max,
        };
        let expired = deadline.token();
        {
            let mut stream = source.fetch(client, &username, opts);
            loop {
                let next = tokio::select! {
                    biased;
                    () = cancel.cancelled() => return Err(ImportFailure::Shutdown),
                    () = expired.cancelled() => {
                        return Err(ImportFailure::Deadline(deadline.expired().unwrap_or(
                            JobDeadlineError {
                                kind: KIND_IMPORT,
                                ms: u64::try_from(deps.idle_deadline.as_millis()).unwrap_or(u64::MAX),
                            },
                        )));
                    }
                    next = stream.next() => next,
                };
                let Some(item) = next else { break };
                deadline.reset();
                self.store(item?).await?;
                deadline.reset();
            }
        }
        // The pass reached its lower bound: everything up to the newest stored game is imported.
        let newest: Option<OffsetDateTime> =
            sqlx::query_scalar("SELECT max(played_at) FROM games WHERE account_id = $1")
                .bind(self.account.id)
                .fetch_one(&deps.pool)
                .await?;
        let newest = match (newest, self.cursor.newest_seen_at) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (a, b) => a.or(b),
        };
        self.frontier = None;
        self.cursor = ImportCursor {
            oldest_reached: None,
            newest_seen_at: newest,
        };
        self.checkpoint().await
    }

    async fn store(&mut self, g: NormalizedGame) -> Result<(), ImportFailure> {
        let username = self.account.username.to_lowercase();
        let player_color = if g.white.name.to_lowercase() == username {
            Some("white")
        } else if g.black.name.to_lowercase() == username {
            Some("black")
        } else {
            None
        };
        match player_color {
            None => self.skipped += 1,
            Some(color) => {
                let res =
                    upsert_game(&self.deps.pool, &new_game(self.account.id, color, &g)).await?;
                if res.inserted {
                    self.imported += 1;
                }
                // Also re-enqueue existing pending games: a previous run may have died between
                // the insert and the enqueue. The live dedupe key keeps one job per game.
                if res.analysis_status == AnalysisStatus::Pending {
                    enqueue_analysis(&self.deps.queue, res.id, self.account.id).await?;
                }
            }
        }
        if self.frontier.is_none_or(|f| g.played_at < f) {
            self.frontier = Some(g.played_at);
        }
        self.since_checkpoint += 1;
        if self.since_checkpoint >= self.deps.checkpoint_every
            || self.last_checkpoint.elapsed() >= self.deps.checkpoint_interval
        {
            self.checkpoint().await?;
        }
        Ok(())
    }

    async fn save_cursor(&self) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE linked_accounts SET import_cursor = $2 WHERE id = $1")
            .bind(self.account.id)
            .bind(sqlx::types::Json(&self.cursor))
            .execute(&self.deps.pool)
            .await?;
        Ok(())
    }

    async fn checkpoint(&mut self) -> Result<(), ImportFailure> {
        self.cursor.oldest_reached = self.frontier;
        self.save_cursor().await?;
        sqlx::query("UPDATE import_runs SET imported = $2, heartbeat_at = now() WHERE id = $1")
            .bind(self.run_id)
            .bind(self.run_imported())
            .execute(&self.deps.pool)
            .await?;
        self.since_checkpoint = 0;
        self.last_checkpoint = Instant::now();
        self.publish(
            UserEventType::ImportProgress,
            json!({ "runId": self.run_id, "imported": self.run_imported() }),
        )
        .await;
        Ok(())
    }

    async fn finish(&self) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE import_runs SET status = 'done', imported = $2, finished_at = now(), \
             heartbeat_at = now() WHERE id = $1",
        )
        .bind(self.run_id)
        .bind(self.run_imported())
        .execute(&self.deps.pool)
        .await?;
        sqlx::query("UPDATE linked_accounts SET last_synced_at = now() WHERE id = $1")
            .bind(self.account.id)
            .execute(&self.deps.pool)
            .await?;
        self.publish(
            UserEventType::ImportProgress,
            json!({ "runId": self.run_id, "imported": self.run_imported(), "done": true }),
        )
        .await;
        Ok(())
    }

    /// Records the failure on the run (keeping the progress made) and maps it to the job result.
    async fn fail(&mut self, failure: ImportFailure, job: &Job) -> JobError {
        let message = truncate_chars(&failure.to_string(), MAX_ERROR_CHARS);
        let code = import_error_code(&failure);
        // An open breaker or a shutdown is not this run failing: it pauses and resumes later.
        let status = match code {
            ImportErrorCode::CircuitOpen | ImportErrorCode::Shutdown => ImportRunStatus::Paused,
            _ => ImportRunStatus::Failed,
        };
        // Keep the progress made: the next run resumes below the last committed game.
        if let Some(frontier) = self.frontier {
            self.cursor.oldest_reached = Some(frontier);
            if let Err(e) = self.save_cursor().await {
                tracing::warn!(account_id = %self.account.id, error = %e, "saving the import cursor failed");
            }
        }
        let recorded = sqlx::query(
            "UPDATE import_runs SET status = $2::import_run_status, imported = $3, \
             last_error = $4, error_code = $5, finished_at = now() WHERE id = $1",
        )
        .bind(self.run_id)
        .bind(status.as_str())
        .bind(self.run_imported())
        .bind(&message)
        .bind(code.as_str())
        .execute(&self.deps.pool)
        .await;
        if let Err(e) = recorded {
            // Keep the original error; the reconciler handles a run left running.
            tracing::warn!(run_id = %self.run_id, error = %e, "recording the import failure failed");
        }
        let job_failed = json!({ "kind": KIND_IMPORT, "runId": self.run_id, "reason": message });
        match failure {
            ImportFailure::Platform(PlatformError::CircuitOpen { retry_in }) => JobError::Release {
                delay: breaker_redelay(
                    retry_in,
                    self.deps.breaker_min_delay,
                    self.deps.probe_timeout,
                    rand::random::<f64>(),
                ),
                reason: code.as_str().to_owned(),
            },
            ImportFailure::Shutdown => JobError::Release {
                delay: Duration::ZERO,
                reason: code.as_str().to_owned(),
            },
            ImportFailure::Platform(PlatformError::NotFound { url }) if is_user_not_found(&url) => {
                self.publish(UserEventType::JobFailed, job_failed).await;
                JobError::Unrecoverable {
                    message,
                    code: Some(code.as_str().to_owned()),
                }
            }
            _ => {
                if job.is_final_attempt() {
                    self.publish(UserEventType::JobFailed, job_failed).await;
                }
                JobError::Retryable {
                    message,
                    code: Some(code.as_str().to_owned()),
                }
            }
        }
    }

    /// Publishing is best effort: a failed publish never fails the import.
    async fn publish(&self, ty: UserEventType, payload: Value) {
        if let Err(e) = publish(&self.deps.pool, self.account.id, ty, payload).await {
            tracing::warn!(account_id = %self.account.id, error = %e, "publish failed");
        }
    }
}

fn new_game(account_id: Uuid, player_color: &str, g: &NormalizedGame) -> NewGame {
    NewGame {
        account_id,
        platform: g.platform.as_str().to_owned(),
        platform_game_id: g.platform_game_id.clone(),
        pgn: g.pgn.clone(),
        played_at: g.played_at,
        time_class: g.time_class.as_str().to_owned(),
        base_seconds: g.base_seconds,
        increment_seconds: g.increment_seconds,
        rated: g.rated,
        player_color: player_color.to_owned(),
        white_name: g.white.name.clone(),
        black_name: g.black.name.clone(),
        white_rating: g.white.rating,
        black_rating: g.black.rating,
        result: g.result.clone(),
        termination: g.termination.clone(),
        eco: g.eco.clone(),
        opening_name: g.opening_name.clone(),
        lichess_evals: g
            .lichess_evals
            .as_ref()
            .and_then(|e| serde_json::to_value(e).ok()),
    }
}

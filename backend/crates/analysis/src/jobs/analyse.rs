//! The analyse-game job (port of `apps/worker/src/jobs/analyse-game.ts`, ADRs 0035 and 0036).
//!
//! Every attempt claims the game by incrementing `analysis_attempts` and keeps the value it got,
//! together with the game's `analysis_generation`. Every later write (heartbeat, skipped, done,
//! pending, failed) applies only while the row still carries both values (the *fence*), so a
//! stale duplicate run can never overwrite a newer attempt's result, and a run from before a
//! re-analysis request can never complete the new generation.
//!
//! Changes from the TS reference (ADR 0035):
//! - "done" is generation-aware: a game is finished only while
//!   `analysed_generation = analysis_generation`, and [`request_reanalysis`] bumps the
//!   generation and resets `analysis_attempts`;
//! - a closed engine pool or a cancelled job token (shutdown, lost lease) leaves the game
//!   `pending` and releases the job without using an attempt, on any attempt (TS dead-lettered
//!   it on the final attempt);
//! - a deadline overrun is a retryable `timeout`;
//! - every attempt that claimed the game records a `job_metrics` row (ADR 0036).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chess_core::game_accuracy::game_accuracy;
use chess_core::move_record::{MoveRecordError, build_move_records};
use chess_core::pgn::{PgnError, parse_pgn};
use chess_core::score::win_percent;
use chess_core::{ClockInfo, Color, EngineScore, MoveRecord, ParsedGame};
use db::games::{AnalysisStatus, GameRow, get_game};
use engine::{ANALYSIS_PROTOCOL, Analyser, EngineError};
use events::store::{UserEventType, publish};
use futures::FutureExt;
use futures::future::BoxFuture;
use queue::queue::{Queue, QueueError};
use queue::runner::{Handler, JobContext};
use queue::{AnalyseGamePayload, EnqueueOptions, Job, JobError, KIND_ANALYSE, analyse_key};
use serde::ser::SerializeMap;
use serde_json::{Value, json};
use sqlx::PgPool;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::deadline::{ANALYSE_DEADLINE, JobDeadlineError};
use crate::eval_cache::EvalCache;
use crate::evaluate::{DeadlineInfo, EvalStats, EvaluateDeps, EvaluateError, evaluate_game};
use crate::metrics::{GameMetrics, log_metrics, record_metrics};

/// Minimum gap between heartbeat writes.
pub const DEFAULT_HEARTBEAT_EVERY: Duration = Duration::from_secs(10);
/// Parallel engine requests per game.
pub const DEFAULT_CONCURRENCY: usize = 4;
/// Max attempts of an analyse job.
pub const ANALYSE_MAX_ATTEMPTS: i32 = 3;
/// `games.analysis_error` keeps at most this many characters.
pub const MAX_ERROR_CHARS: usize = 2000;
/// Move rows per INSERT.
pub const INSERT_CHUNK: usize = 500;
/// The deadline kind reported by an overrun.
const KIND: &str = "analyse-game";

pub struct AnalyseGameDeps {
    pub pool: PgPool,
    pub analyser: Arc<dyn Analyser>,
    pub cache: Arc<dyn EvalCache>,
    /// Minimum gap between heartbeat writes (default 10 s).
    pub heartbeat_every: Duration,
    /// Hard cap on one attempt (default 10 minutes); an overrun is a retryable `timeout`.
    pub deadline: Duration,
    /// Parallel engine requests for `evaluate_game` (default 4).
    pub concurrency: usize,
    /// Publish `game.analysed` and `job.failed` (default true).
    pub publish_events: bool,
}

impl AnalyseGameDeps {
    /// The production defaults: heartbeat 10 s, deadline 10 min, concurrency 4, events on.
    pub fn new(pool: PgPool, analyser: Arc<dyn Analyser>, cache: Arc<dyn EvalCache>) -> Self {
        AnalyseGameDeps {
            pool,
            analyser,
            cache,
            heartbeat_every: DEFAULT_HEARTBEAT_EVERY,
            deadline: ANALYSE_DEADLINE,
            concurrency: DEFAULT_CONCURRENCY,
            publish_events: true,
        }
    }
}

/// What one attempt did. Serialises as `{"status":"done"|"skipped"|"superseded"}`.
#[derive(Clone, Debug, PartialEq)]
pub enum AnalyseOutcome {
    /// Analysed (or already done at this protocol and generation). `accuracy` is the player's
    /// colour's game accuracy.
    Done { accuracy: Option<f64> },
    /// Unsupported variant or fewer than 2 plies.
    Skipped,
    /// A newer attempt owns the game; this run's writes were discarded.
    Superseded,
}

impl AnalyseOutcome {
    pub fn status(&self) -> &'static str {
        match self {
            AnalyseOutcome::Done { .. } => "done",
            AnalyseOutcome::Skipped => "skipped",
            AnalyseOutcome::Superseded => "superseded",
        }
    }
}

impl serde::Serialize for AnalyseOutcome {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut m = s.serialize_map(Some(1))?;
        m.serialize_entry("status", self.status())?;
        m.end()
    }
}

/// Enqueues an analysis of the game. `false` when a live job already holds `analyse:<game_id>`.
pub async fn enqueue_analyse(
    queue: &Queue,
    game_id: Uuid,
    account_id: Uuid,
) -> Result<bool, QueueError> {
    let payload = serde_json::to_value(AnalyseGamePayload { game_id })
        .unwrap_or_else(|_| json!({ "gameId": game_id }));
    let id = queue
        .enqueue(
            KIND_ANALYSE,
            &payload,
            EnqueueOptions {
                dedupe_key: Some(analyse_key(game_id)),
                user_key: Some(account_id),
                max_attempts: ANALYSE_MAX_ATTEMPTS,
                ..EnqueueOptions::default()
            },
        )
        .await?;
    Ok(id.is_some())
}

/// Starts a new analysis generation of the game: bumps `analysis_generation`, resets
/// `analysis_attempts`, sets it pending and enqueues it (a live job for the game is reused).
/// `false`, with nothing changed, when the game is missing or mid-analysis.
pub async fn request_reanalysis(
    pool: &PgPool,
    queue: &Queue,
    game_id: Uuid,
) -> Result<bool, QueueError> {
    let account: Option<Uuid> = sqlx::query_scalar(
        "UPDATE games SET analysis_generation = analysis_generation + 1, analysis_attempts = 0, \
         analysis_status = 'pending', analysis_error = NULL, pending_since = now() \
         WHERE id = $1 AND analysis_status <> 'analysing' RETURNING account_id",
    )
    .bind(game_id)
    .fetch_optional(pool)
    .await?;
    let Some(account_id) = account else {
        return Ok(false);
    };
    if !enqueue_analyse(queue, game_id, account_id).await? {
        // A live job holds `analyse:<game_id>`. A queued one claims the new generation; a running
        // one re-reads the game when it finishes and releases itself to run again (the
        // reconciler is the final backstop).
        tracing::info!(
            game_id = %game_id,
            "re-analysis deduped onto the game's live analyse job"
        );
    }
    Ok(true)
}

/// The queue handler for [`KIND_ANALYSE`].
pub struct AnalyseGameHandler(pub Arc<AnalyseGameDeps>);

impl Handler for AnalyseGameHandler {
    fn kind(&self) -> &'static str {
        KIND_ANALYSE
    }

    fn handle(&self, job: Job, ctx: JobContext) -> BoxFuture<'_, Result<Value, JobError>> {
        async move {
            // Retried like any failure (`Unrecoverable` is kept for a missing game row).
            let payload: AnalyseGamePayload =
                serde_json::from_value(job.payload.clone()).map_err(|e| JobError::Retryable {
                    message: format!("invalid analyse-game payload: {e}"),
                    code: Some("invalid_payload".to_owned()),
                })?;
            let outcome = handle_analyse_game(&self.0, &job, payload, ctx.cancel).await?;
            Ok(serde_json::to_value(&outcome).unwrap_or_else(|_| json!({})))
        }
        .boxed()
    }

    /// Marks the game failed when the job is dead-lettered, unless it already reached a final
    /// status (the handler's own final-attempt write, which also published `job.failed`) or a
    /// re-analysis reset it (`analysis_attempts = 0`: no attempt of the new generation ran yet).
    fn on_dead<'a>(&'a self, job: &'a Job, reason: &'a str) -> BoxFuture<'a, ()> {
        async move {
            let deps = &self.0;
            let Ok(payload) = serde_json::from_value::<AnalyseGamePayload>(job.payload.clone())
            else {
                tracing::warn!(job_id = job.id, "dead analyse job with an invalid payload");
                return;
            };
            let reason = truncate_chars(reason, MAX_ERROR_CHARS);
            let account: Result<Option<Uuid>, sqlx::Error> = sqlx::query_scalar(
                "UPDATE games SET analysis_status = 'failed', analysis_error = $2 \
                 WHERE id = $1 AND analysis_status NOT IN ('done', 'failed', 'skipped') \
                 AND analysis_attempts > 0 RETURNING account_id",
            )
            .bind(payload.game_id)
            .bind(&reason)
            .fetch_optional(&deps.pool)
            .await;
            match account {
                Ok(Some(account_id)) => {
                    publish_failed(deps, account_id, payload.game_id, &reason).await;
                }
                Ok(None) => {}
                Err(e) => {
                    tracing::warn!(game_id = %payload.game_id, error = %e, "on_dead write failed");
                }
            }
        }
        .boxed()
    }
}

/// Why an attempt stopped before `done` or `skipped`.
#[derive(thiserror::Error, Debug)]
enum Failure {
    #[error("superseded by a newer attempt")]
    Superseded,
    #[error(transparent)]
    Pgn(#[from] PgnError),
    #[error(transparent)]
    Evaluate(#[from] EvaluateError),
    #[error(transparent)]
    Records(#[from] MoveRecordError),
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),
}

/// The game while this attempt owns it.
#[derive(Clone, Copy)]
struct Fence {
    game_id: Uuid,
    attempt: i32,
    generation: i32,
}

const FENCE: &str = "id = $1 AND analysis_attempts = $2 AND analysis_generation = $3";

impl Fence {
    /// Runs `UPDATE games SET <set> WHERE <fence> RETURNING id`, with `$4` bound to `error` when
    /// given (`set` must use `$4` exactly then); `true` when it hit the row.
    async fn update(
        &self,
        pool: &PgPool,
        set: &str,
        error: Option<&str>,
    ) -> Result<bool, sqlx::Error> {
        let sql = format!("UPDATE games SET {set} WHERE {FENCE} RETURNING id");
        let mut q = sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
            .bind(self.game_id)
            .bind(self.attempt)
            .bind(self.generation);
        if let Some(error) = error {
            q = q.bind(error.to_owned());
        }
        let hit: Option<Uuid> = q.fetch_optional(pool).await?;
        Ok(hit.is_some())
    }
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

fn truncate_chars(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

fn to_i32(n: usize) -> Option<i32> {
    i32::try_from(n).ok()
}

/// Analyses one game (see the module docs).
pub async fn handle_analyse_game(
    deps: &AnalyseGameDeps,
    job: &Job,
    payload: AnalyseGamePayload,
    cancel: CancellationToken,
) -> Result<AnalyseOutcome, JobError> {
    let started = Instant::now();
    let game_id = payload.game_id;
    let retryable_db = |e: sqlx::Error| JobError::Retryable {
        message: format!("database error: {e}"),
        code: None,
    };
    let missing = || JobError::Unrecoverable {
        message: format!("Game not found: {game_id}"),
        code: Some("not_found".to_owned()),
    };
    // Retrying cannot make a deleted game appear.
    let game = get_game(&deps.pool, game_id)
        .await
        .map_err(retryable_db)?
        .ok_or_else(missing)?;
    // A duplicate delivery of finished work: nothing to do.
    if game.analysis_status == AnalysisStatus::Done
        && game.analysis_protocol.as_deref() == Some(ANALYSIS_PROTOCOL)
        && game.analysed_generation == Some(game.analysis_generation)
    {
        if reanalysis_requested(&deps.pool, game_id, game.analysis_generation).await {
            return Err(reanalysis_release());
        }
        return Ok(AnalyseOutcome::Done {
            accuracy: player_accuracy(&game),
        });
    }

    // Every attempt (retry, lost-lease pickup, reconciler re-enqueue) starts again from here.
    let claimed: Option<(i32, i32)> = sqlx::query_as(
        "UPDATE games SET analysis_status = 'analysing', analysis_error = NULL, \
         analysis_started_at = now(), heartbeat_at = now(), \
         analysis_attempts = analysis_attempts + 1 \
         WHERE id = $1 RETURNING analysis_attempts, analysis_generation",
    )
    .bind(game_id)
    .fetch_optional(&deps.pool)
    .await
    .map_err(retryable_db)?;
    let (attempt, generation) = claimed.ok_or_else(missing)?;
    let fence = Fence {
        game_id,
        attempt,
        generation,
    };

    let wait = job.started_at - job.run_at.max(job.created_at);
    let mut metrics = GameMetrics {
        job_id: job.id,
        game_id,
        account_id: game.account_id,
        queue_wait_ms: Some((wait.as_seconds_f64() * 1000.0).max(0.0)),
        ..GameMetrics::default()
    };

    // The run's token is a child of the job's: it fires on shutdown or a lost lease, on the
    // deadline, and when a heartbeat finds the game superseded (`analyse` maps that one to
    // `Superseded`; `timed_out` marks the deadline). On the deadline the same future is awaited
    // to its end, so `evaluate_game` returns its partial statistics.
    let token = cancel.child_token();
    let deadline_info = DeadlineInfo {
        kind: KIND,
        ms: u64::try_from(deps.deadline.as_millis()).unwrap_or(u64::MAX),
    };
    let mut timed_out = None;
    let result = {
        let run = analyse(deps, &game, fence, &token, deadline_info, &mut metrics);
        tokio::pin!(run);
        tokio::select! {
            r = &mut run => r,
            _ = tokio::time::sleep(deps.deadline) => {
                timed_out = Some(JobDeadlineError { kind: KIND, ms: deadline_info.ms });
                token.cancel();
                run.await
            }
        }
    };

    let outcome = match result {
        Ok(outcome) => Ok(outcome),
        Err(Failure::Superseded) => Ok(AnalyseOutcome::Superseded),
        Err(failure) => Err(on_failure(deps, job, &game, fence, timed_out, &cancel, failure).await),
    };

    metrics.outcome = match &outcome {
        Ok(o) => o.status(),
        Err(JobError::Release { .. }) => "released",
        Err(_) if job.is_final_attempt() => "failed",
        Err(_) => "retry",
    };
    if let Ok(AnalyseOutcome::Done { accuracy }) = &outcome
        && deps.publish_events
    {
        let payload = json!({ "gameId": game_id, "accuracy": accuracy });
        if let Err(e) = publish(
            &deps.pool,
            game.account_id,
            UserEventType::GameAnalysed,
            payload,
        )
        .await
        {
            tracing::warn!(game_id = %game_id, error = %e, "publish failed");
        }
    }
    metrics.total_ms = ms(started.elapsed());
    if let Err(e) = record_metrics(&deps.pool, &metrics).await {
        tracing::warn!(game_id = %game_id, error = %e, "recording job metrics failed");
    }
    log_metrics(&metrics);
    // A re-analysis requested while this run finished was deduped onto this job: run it again
    // (a zero-delay release keeps `run_at`), so it claims the new generation. That applies after
    // every final status this run wrote (done, skipped, or failed on the final attempt); a
    // non-final failure is retried anyway, and its next claim takes the new generation.
    let handoff = match &outcome {
        Ok(AnalyseOutcome::Done { .. } | AnalyseOutcome::Skipped) => true,
        Err(JobError::Retryable { .. }) => job.is_final_attempt(),
        _ => false,
    };
    if handoff && reanalysis_requested(&deps.pool, game_id, generation).await {
        return Err(reanalysis_release());
    }
    outcome
}

/// `true` when the game is pending at a newer generation than `processed`.
async fn reanalysis_requested(pool: &PgPool, game_id: Uuid, processed: i32) -> bool {
    let newer: Result<Option<bool>, sqlx::Error> = sqlx::query_scalar(
        "SELECT analysis_status = 'pending' AND analysis_generation > $2 FROM games WHERE id = $1",
    )
    .bind(game_id)
    .bind(processed)
    .fetch_optional(pool)
    .await;
    match newer {
        Ok(found) => found.unwrap_or(false),
        Err(e) => {
            // The reconciler re-enqueues a pending game whose job is gone.
            tracing::warn!(game_id = %game_id, error = %e, "re-analysis check failed");
            false
        }
    }
}

fn reanalysis_release() -> JobError {
    JobError::Release {
        delay: Duration::ZERO,
        reason: "reanalysis".to_owned(),
    }
}

/// Records a failed attempt on the game (fenced) and maps it to the job's verdict.
async fn on_failure(
    deps: &AnalyseGameDeps,
    job: &Job,
    game: &GameRow,
    fence: Fence,
    timed_out: Option<JobDeadlineError>,
    cancel: &CancellationToken,
    failure: Failure,
) -> JobError {
    let pool_closed = matches!(
        failure,
        Failure::Evaluate(EvaluateError::Engine(EngineError::PoolClosed))
    );
    // A shutdown (or a lost lease) is not the game's fault: leave it pending, use no attempt.
    if pool_closed || cancel.is_cancelled() {
        let message = if pool_closed {
            failure.to_string()
        } else {
            "analysis interrupted by shutdown".to_owned()
        };
        let message = truncate_chars(&message, MAX_ERROR_CHARS);
        if let Err(e) = fence
            .update(
                &deps.pool,
                "analysis_status = 'pending', analysis_error = $4, pending_since = now()",
                Some(&message),
            )
            .await
        {
            tracing::warn!(game_id = %fence.game_id, error = %e, "pending write failed");
        }
        return JobError::Release {
            delay: Duration::ZERO,
            reason: "shutdown".to_owned(),
        };
    }

    let message = match &timed_out {
        Some(e) => e.to_string(),
        None => failure.to_string(),
    };
    let message = truncate_chars(&message, MAX_ERROR_CHARS);
    let code = timed_out.map(|_| "timeout".to_owned());
    if job.is_final_attempt() {
        match fence
            .update(
                &deps.pool,
                "analysis_status = 'failed', analysis_error = $4",
                Some(&message),
            )
            .await
        {
            Ok(true) => publish_failed(deps, game.account_id, game.id, &message).await,
            Ok(false) => {}
            Err(e) => tracing::warn!(game_id = %game.id, error = %e, "failed write failed"),
        }
    } else if let Err(e) = fence
        .update(
            &deps.pool,
            "analysis_status = 'pending', analysis_error = $4, pending_since = now()",
            Some(&message),
        )
        .await
    {
        // Keep the original error; the retry sets the status again.
        tracing::warn!(game_id = %game.id, error = %e, "pending write failed");
    }
    JobError::Retryable { message, code }
}

async fn publish_failed(deps: &AnalyseGameDeps, account_id: Uuid, game_id: Uuid, reason: &str) {
    if !deps.publish_events {
        return;
    }
    let payload = json!({ "kind": KIND_ANALYSE, "gameId": game_id, "reason": reason });
    if let Err(e) = publish(&deps.pool, account_id, UserEventType::JobFailed, payload).await {
        tracing::warn!(game_id = %game_id, error = %e, "publish failed");
    }
}

fn player_accuracy(game: &GameRow) -> Option<f64> {
    let a = if game.player_color == "white" {
        game.white_accuracy
    } else {
        game.black_accuracy
    };
    a.map(f64::from)
}

fn apply_stats(m: &mut GameMetrics, s: &EvalStats) {
    m.positions = to_i32(s.positions);
    m.engine_positions = to_i32(s.engine_positions);
    m.cache_hits = to_i32(s.cache_hits);
    m.lichess_used = to_i32(s.lichess_used);
    m.cache_ms = Some(s.cache_ms);
    m.engine_wall_ms = Some(s.engine_wall_ms);
    m.engine_cpu_ms = s.engine_cpu_ms;
    m.engine_stage_ms = Some(s.engine_stage_elapsed_ms);
    m.evaluate_ms = Some(s.total_elapsed_ms);
}

/// Supplied Lichess evals (`lichess_evals[i]` is the eval after ply `i + 1`), if well formed.
fn lichess_evals(game: &GameRow) -> Option<Vec<Option<EngineScore>>> {
    let v = game.lichess_evals.as_ref()?;
    match serde_json::from_value(v.clone()) {
        Ok(evals) => Some(evals),
        Err(e) => {
            tracing::warn!(game_id = %game.id, error = %e, "ignoring malformed lichess_evals");
            None
        }
    }
}

async fn analyse(
    deps: &AnalyseGameDeps,
    game: &GameRow,
    fence: Fence,
    token: &CancellationToken,
    deadline: DeadlineInfo,
    metrics: &mut GameMetrics,
) -> Result<AnalyseOutcome, Failure> {
    let pool = &deps.pool;
    let skipped = || async {
        if fence
            .update(
                pool,
                "analysis_status = 'skipped', analysis_error = NULL",
                None,
            )
            .await?
        {
            Ok(AnalyseOutcome::Skipped)
        } else {
            Err(Failure::Superseded)
        }
    };

    let t = Instant::now();
    let parsed = parse_pgn(&game.pgn);
    metrics.parse_ms = Some(ms(t.elapsed()));
    let parsed: ParsedGame = match parsed {
        Ok(p) => p,
        Err(e) if e.is_unsupported() => return skipped().await,
        // Any other parse error is the game's (poison): retried, then failed.
        Err(e) => return Err(e.into()),
    };
    if parsed.plies.len() < 2 {
        metrics.positions = to_i32(parsed.plies.len() + 1);
        return skipped().await;
    }

    // Heartbeats: fenced, at most every `heartbeat_every`, from inside the evaluation. A miss
    // means a newer attempt owns the game: cancel the run, so its engine slots are freed.
    let last_beat = Arc::new(Mutex::new(Instant::now()));
    let every = deps.heartbeat_every;
    let lost = Arc::new(AtomicBool::new(false));
    let heartbeat = {
        let pool = pool.clone();
        let (token, lost) = (token.clone(), lost.clone());
        move |_done: usize, _total: usize| -> BoxFuture<'static, Result<(), String>> {
            let due = {
                let mut last = last_beat.lock().unwrap_or_else(|e| e.into_inner());
                let due = last.elapsed() >= every;
                if due {
                    *last = Instant::now();
                }
                due
            };
            if !due {
                return futures::future::ready(Ok(())).boxed();
            }
            let (pool, token, lost) = (pool.clone(), token.clone(), lost.clone());
            async move {
                let hit = fence
                    .update(&pool, "heartbeat_at = now()", None)
                    .await
                    .map_err(|e| e.to_string())?;
                if !hit {
                    lost.store(true, Ordering::SeqCst);
                    token.cancel();
                }
                Ok(())
            }
            .boxed()
        }
    };

    let lichess = lichess_evals(game);
    let eval_deps = EvaluateDeps {
        analyser: deps.analyser.as_ref(),
        cache: deps.cache.as_ref(),
        concurrency: deps.concurrency,
        on_progress: Some(&heartbeat),
        cancel: Some(token.clone()),
        deadline: Some(deadline),
    };
    let evaluated = evaluate_game(&eval_deps, &parsed, lichess.as_deref()).await;
    let evals = match evaluated {
        Ok(ev) => {
            apply_stats(metrics, &ev.stats);
            ev.evals
        }
        Err(failure) => {
            apply_stats(metrics, &failure.stats);
            if lost.load(Ordering::SeqCst) {
                return Err(Failure::Superseded);
            }
            return Err(failure.error.into());
        }
    };
    // A completed evaluation is persisted even if the deadline fired just after it (the persist
    // is fenced, so a newer attempt still wins).

    let clock = match (game.base_seconds, game.increment_seconds) {
        (Some(base), Some(inc)) => Some(ClockInfo {
            base_seconds: f64::from(base),
            increment_seconds: f64::from(inc),
        }),
        _ => None,
    };
    let records = build_move_records(&parsed, &evals, clock)?;
    let wins: Vec<f64> = evals.iter().map(|e| win_percent(e.score)).collect();
    let white = game_accuracy(&wins, Color::White);
    let black = game_accuracy(&wins, Color::Black);

    let t = Instant::now();
    let persisted = persist(pool, fence, &records, white, black).await;
    metrics.persist_ms = Some(ms(t.elapsed()));
    persisted?;
    Ok(AnalyseOutcome::Done {
        accuracy: Some(if game.player_color == "white" {
            white
        } else {
            black
        }),
    })
}

/// Claims `done` under the fence (or rolls back with `Superseded`), then replaces the moves.
async fn persist(
    pool: &PgPool,
    fence: Fence,
    records: &[MoveRecord],
    white: f64,
    black: f64,
) -> Result<(), Failure> {
    let mut tx = pool.begin().await?;
    let owned: Option<Uuid> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "UPDATE games SET analysis_status = 'done', analysis_protocol = $4, \
         analysis_error = NULL, white_accuracy = $5, black_accuracy = $6, \
         analysed_generation = analysis_generation WHERE {FENCE} RETURNING id"
    )))
    .bind(fence.game_id)
    .bind(fence.attempt)
    .bind(fence.generation)
    .bind(ANALYSIS_PROTOCOL)
    .bind(white)
    .bind(black)
    .fetch_optional(&mut *tx)
    .await?;
    if owned.is_none() {
        // Dropping the transaction rolls it back.
        return Err(Failure::Superseded);
    }
    sqlx::query("DELETE FROM moves WHERE game_id = $1")
        .bind(fence.game_id)
        .execute(&mut *tx)
        .await?;
    for chunk in records.chunks(INSERT_CHUNK) {
        insert_moves(&mut tx, fence.game_id, chunk).await?;
    }
    tx.commit().await?;
    Ok(())
}

const INSERT_MOVES_SQL: &str = "INSERT INTO moves (game_id, ply, color, san, uci, fen_before, \
    best_uci, win_before, win_after, delta_w, accuracy, judgement, phase, clock_before, \
    time_spent, decided) \
    SELECT * FROM UNNEST($1::uuid[], $2::int4[], $3::text[]::color[], $4::text[], $5::text[], \
    $6::text[], $7::text[], $8::float8[], $9::float8[], $10::float8[], $11::float8[], \
    $12::text[]::judgement[], $13::text[]::phase[], $14::float8[], $15::float8[], $16::bool[])";

async fn insert_moves(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    game_id: Uuid,
    rows: &[MoveRecord],
) -> Result<(), sqlx::Error> {
    let col = |f: fn(&MoveRecord) -> f64| rows.iter().map(f).collect::<Vec<f64>>();
    sqlx::query(INSERT_MOVES_SQL)
        .bind(vec![game_id; rows.len()])
        .bind(
            rows.iter()
                .map(|r| i32::try_from(r.ply).unwrap_or(i32::MAX))
                .collect::<Vec<i32>>(),
        )
        .bind(rows.iter().map(|r| r.color.as_str()).collect::<Vec<_>>())
        .bind(rows.iter().map(|r| r.san.as_str()).collect::<Vec<_>>())
        .bind(rows.iter().map(|r| r.uci.as_str()).collect::<Vec<_>>())
        .bind(
            rows.iter()
                .map(|r| r.fen_before.as_str())
                .collect::<Vec<_>>(),
        )
        .bind(
            rows.iter()
                .map(|r| r.best_uci.as_deref())
                .collect::<Vec<_>>(),
        )
        .bind(col(|r| r.win_before))
        .bind(col(|r| r.win_after))
        .bind(col(|r| r.delta_w))
        .bind(col(|r| r.accuracy))
        .bind(
            rows.iter()
                .map(|r| r.judgement.map(|j| j.as_str()))
                .collect::<Vec<_>>(),
        )
        .bind(rows.iter().map(|r| r.phase.as_str()).collect::<Vec<_>>())
        .bind(rows.iter().map(|r| r.clock_before).collect::<Vec<_>>())
        .bind(rows.iter().map(|r| r.time_spent).collect::<Vec<_>>())
        .bind(rows.iter().map(|r| r.decided).collect::<Vec<_>>())
        .execute(&mut **tx)
        .await?;
    Ok(())
}

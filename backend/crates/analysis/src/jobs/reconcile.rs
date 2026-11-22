//! The reconciler (port of `apps/worker/src/jobs/reconcile.ts`, spec §13a): an idempotent repair
//! sweep. Postgres holds the state of games and import runs; the sweep rebuilds what the queue
//! should contain.
//!
//! 1. Games `pending` for longer than `pending_grace` with no live analyse job are re-enqueued,
//!    in keyset pages with one `live_states` query per page.
//! 2. `analysing` games whose heartbeat is older than `stale_after` go back to `pending` (a
//!    fenced reset) and are re-enqueued, unless their job is still `running` under a lease
//!    (`hung_active`: left alone, its deadline or the reaper ends it).
//! 3. In both sweeps a game at [`MAX_ANALYSIS_ATTEMPTS`] is marked failed instead (poison games)
//!    and `job.failed` is published.
//! 4. `running` import runs with a stale heartbeat: their account's import is enqueued again,
//!    then the runs are closed as failed.
//! 5. Accounts whose newest run is `paused` and that have no live import job get one (the job
//!    reuses the paused run, T21). Changed from TS, where BullMQ kept the delayed job.
//! 6. Retention: user events (7 days), finished jobs ([`Retention::default`]) and `job_metrics`
//!    (30 days).
//!
//! Interruption: there is no sweep-wide transaction, so a sweep cut off part-way (shutdown, a
//! lost connection, a crash) can stop between two writes. The writes are ordered so that
//! whatever such a stop leaves behind is found and repaired by the next sweep:
//! - a stale import run is closed only after its import is enqueued (step 4);
//! - a stale game reset to `pending` before its enqueue is caught by step 1 once
//!   `pending_grace` has passed;
//! - the one loss is a `job.failed` event when the stop falls between a game's failed update and
//!   its publish. Clients see the failed status on their next refetch.

use std::collections::HashMap;
use std::time::Duration;

use futures::FutureExt;
use futures::future::BoxFuture;
use queue::queue::{Queue, QueueError};
use queue::retention::Retention;
use queue::runner::{Handler, JobContext};
use queue::{
    EnqueueOptions, Job, JobError, JobState, KIND_ANALYSE, KIND_IMPORT, KIND_RECONCILE,
    RECONCILE_EVERY, RECONCILE_KEY, analyse_key, import_key,
};
use serde::Serialize;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use crate::jobs::analyse::{analyse_enqueue_options, analyse_payload, enqueue_analyse};
use crate::jobs::import::enqueue_import;

/// Attempt cap across every path (queue retries, stalled pickups, reconciler re-enqueues): twice
/// the analyse job's 3 attempts, so a game is only given up on after two full rounds.
pub const MAX_ANALYSIS_ATTEMPTS: i32 = 6;
pub const EXHAUSTED_REASON: &str = "exceeded max attempts (stalled or crashed worker)";

/// The error stored on an import run closed for a stale heartbeat.
pub const STALE_RUN_ERROR: &str = "stale heartbeat: resumed by the reconciler";

/// `job_metrics` rows older than this are deleted (ADR 0036).
pub const JOB_METRICS_RETENTION: Duration = Duration::from_secs(30 * 24 * 3600);
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReconcileOptions {
    /// Heartbeat age after which `analysing` games and `running` imports count as stale (10 min).
    pub stale_after: Duration,
    /// Games pending for less than this are left alone: their enqueue may be in flight (2 min).
    pub pending_grace: Duration,
    /// Keyset page size of the game sweeps (1000).
    pub page_size: i64,
}

impl Default for ReconcileOptions {
    fn default() -> Self {
        Self {
            stale_after: Duration::from_secs(600),
            pending_grace: Duration::from_secs(120),
            page_size: 1000,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReconcileResult {
    pub requeued_pending: u64,
    pub reset_stale: u64,
    pub resumed_imports: u64,
    pub revived_paused: u64,
    /// Stale heartbeat but the job is still `running` under a lease: left alone.
    pub hung_active: u64,
    /// Games marked failed because they used up [`MAX_ANALYSIS_ATTEMPTS`].
    pub exhausted: u64,
    pub pruned_events: u64,
    pub cleaned_jobs: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum ReconcileError {
    #[error("reconcile database error: {0}")]
    Db(#[from] sqlx::Error),
    #[error(transparent)]
    Queue(#[from] QueueError),
    #[error("reconcile events error: {0}")]
    Events(#[from] events::store::Error),
}

/// One game row of a sweep page.
#[derive(sqlx::FromRow)]
struct GameRow {
    id: Uuid,
    account_id: Uuid,
    analysis_attempts: i32,
    /// Old enough for this sweep (past the grace or the stale cutoff).
    due: bool,
}

/// One sweep page: a *window* of the next `$3` games of the status after the keyset cursor `$2`
/// (NULL for the first), each flagged `due` when its age column is older than `$1` seconds.
///
/// The window has no other filter, so it is always an ordered range scan of
/// `games_pending_id_idx (analysis_status, id) WHERE analysis_status IN ('pending',
/// 'analysing')` that stops after `$3` entries. Filtering on the age in the scan instead lets
/// the planner, which guesses that a third of rows pass, prefer a bitmap scan plus a top-N sort
/// over the whole remaining range. The status is a literal, so even the generic plan proves the
/// index predicate, and the cursor bound is sargable (`coalesce` to the nil uuid). The cursor
/// advances to the window's last id, due or not.
macro_rules! page_sql {
    ($status:literal, $age:literal) => {
        concat!(
            "SELECT id, account_id, analysis_attempts, ",
            $age,
            " < now() - make_interval(secs => $1) AS due \
             FROM games WHERE analysis_status = '",
            $status,
            "' AND id > coalesce($2::uuid, '00000000-0000-0000-0000-000000000000'::uuid) \
             ORDER BY id LIMIT $3"
        )
    };
}

const PENDING_PAGE_SQL: &str =
    page_sql!("pending", "coalesce(pending_since, 'epoch'::timestamptz)");
const STALE_PAGE_SQL: &str = page_sql!(
    "analysing",
    "coalesce(heartbeat_at, analysis_started_at, 'epoch'::timestamptz)"
);

const GIVE_UP_SQL: &str = "\
UPDATE games SET analysis_status = 'failed', analysis_error = $3
WHERE id = ANY($1) AND analysis_status = $2::analysis_status AND analysis_attempts >= 6
RETURNING id, account_id";

/// The fenced reset of a stale analysis: only if nothing claimed the game since it was read.
const RESET_STALE_SQL: &str = "\
UPDATE games SET analysis_status = 'pending', pending_since = now()
WHERE id = $1 AND analysis_status = 'analysing' AND analysis_attempts = $2";

/// `$1` the staleness in seconds (served by `import_runs_status_heartbeat_idx`).
const STALE_RUNS_SQL: &str = "\
SELECT id, account_id FROM import_runs
WHERE status = 'running' AND heartbeat_at < now() - make_interval(secs => $1)";

/// Closes the runs read by [`STALE_RUNS_SQL`], fenced: a run that heartbeated or finished since
/// it was read is left alone.
const CLOSE_STALE_RUNS_SQL: &str = "\
UPDATE import_runs SET status = 'failed', finished_at = now(), last_error = $3,
  error_code = 'timeout'
WHERE id = ANY($1) AND status = 'running' AND heartbeat_at < now() - make_interval(secs => $2)";

/// Accounts whose newest run (the one the import job would reuse) is paused.
const PAUSED_ACCOUNTS_SQL: &str = "\
SELECT DISTINCT p.account_id FROM import_runs p
WHERE p.status = 'paused' AND NOT EXISTS (
  SELECT 1 FROM import_runs n WHERE n.account_id = p.account_id
    AND (n.started_at, n.id) > (p.started_at, p.id))";

/// The literal `6` matches the predicate of `games_attempt_cap_idx`, so the count uses it.
const AT_CAP_SQL: &str = "SELECT count(*) FROM games WHERE analysis_attempts >= 6";

const STATUS_COUNTS_SQL: &str = "\
SELECT analysis_status::text, count(*) FROM games
WHERE analysis_status IN ('pending', 'analysing', 'failed') GROUP BY 1";

/// Runs one sweep (see the module docs) and returns what it did.
pub async fn reconcile(
    pool: &PgPool,
    queue: &Queue,
    opts: &ReconcileOptions,
) -> Result<ReconcileResult, ReconcileError> {
    let mut result = ReconcileResult::default();
    let page_size = opts.page_size.max(1);

    // 1. Lost pending games. Runs first, so games reset in step 2 are counted only there.
    let grace = opts.pending_grace.as_secs_f64();
    let mut after: Option<Uuid> = None;
    loop {
        let page = fetch_page(pool, PENDING_PAGE_SQL, grace, after, page_size).await?;
        let Some(last) = page.last else { break };
        after = Some(last);
        let live = live_states(queue, &page.due).await?;
        let mut exhausted = Vec::new();
        let mut lost = Vec::new();
        for g in &page.due {
            if live.contains_key(&analyse_key(g.id)) {
                continue;
            }
            if g.analysis_attempts >= MAX_ANALYSIS_ATTEMPTS {
                exhausted.push(g.id);
            } else {
                lost.push((g.id, g.account_id));
            }
        }
        result.requeued_pending += enqueue_analysis_page(queue, &lost).await?;
        result.exhausted += give_up(pool, &exhausted, "pending").await?;
        if (page.len as i64) < page_size {
            break;
        }
    }

    // 2. Stale analyses. A null heartbeat falls back to the start time (or counts as stale).
    let stale_after = opts.stale_after.as_secs_f64();
    let mut after: Option<Uuid> = None;
    loop {
        let page = fetch_page(pool, STALE_PAGE_SQL, stale_after, after, page_size).await?;
        let Some(last) = page.last else { break };
        after = Some(last);
        let live = live_states(queue, &page.due).await?;
        let mut exhausted = Vec::new();
        for g in &page.due {
            if live.get(&analyse_key(g.id)) == Some(&JobState::Running) {
                // A worker holds the lease, so re-enqueueing would be a no-op and resetting the
                // row would be a lie. Its deadline ends it; the next sweep sees the outcome.
                result.hung_active += 1;
                tracing::warn!(game_id = %g.id, "stale heartbeat but the job is still running");
                continue;
            }
            if g.analysis_attempts >= MAX_ANALYSIS_ATTEMPTS {
                exhausted.push(g.id);
                continue;
            }
            let reset = sqlx::query(RESET_STALE_SQL)
                .bind(g.id)
                .bind(g.analysis_attempts)
                .execute(pool)
                .await?;
            if reset.rows_affected() == 0 {
                continue; // it moved on meanwhile
            }
            result.reset_stale += 1;
            enqueue_analyse(queue, g.id, g.account_id).await?;
        }
        result.exhausted += give_up(pool, &exhausted, "analysing").await?;
        if (page.len as i64) < page_size {
            break;
        }
    }

    // 4. Stale import runs: enqueue each account's import first (deduped, so repeating it is
    //    harmless), and only then close the runs. A sweep cut off in between leaves the runs
    //    `running` and stale, so the next sweep finds them again; closing first could lose the
    //    resume for good.
    let stale_runs: Vec<(Uuid, Uuid)> = sqlx::query_as(STALE_RUNS_SQL)
        .bind(stale_after)
        .fetch_all(pool)
        .await?;
    if !stale_runs.is_empty() {
        let mut accounts: Vec<Uuid> = stale_runs.iter().map(|r| r.1).collect();
        accounts.sort_unstable();
        accounts.dedup();
        for account in accounts {
            enqueue_import(queue, account, None).await?;
        }
        let ids: Vec<Uuid> = stale_runs.iter().map(|r| r.0).collect();
        let closed = sqlx::query(CLOSE_STALE_RUNS_SQL)
            .bind(&ids)
            .bind(stale_after)
            .bind(STALE_RUN_ERROR)
            .execute(pool)
            .await?;
        result.resumed_imports = closed.rows_affected();
    }

    // 5. Paused runs with no live import job (an import that lost its released job).
    let paused: Vec<Uuid> = sqlx::query_scalar(PAUSED_ACCOUNTS_SQL)
        .fetch_all(pool)
        .await?;
    for account in paused {
        if queue
            .live_states(KIND_IMPORT, &[import_key(account)])
            .await?
            .is_empty()
            && enqueue_import(queue, account, None).await?
        {
            result.revived_paused += 1;
        }
    }

    // 6. Retention.
    result.pruned_events =
        events::store::prune_events(pool, events::store::EVENT_RETENTION).await?;
    result.cleaned_jobs = queue.cleanup(&Retention::default()).await?;
    let pruned_metrics = prune_job_metrics(pool).await?;

    // 7. Counts for the log.
    let counts: Vec<(String, i64)> = sqlx::query_as(STATUS_COUNTS_SQL).fetch_all(pool).await?;
    let counts: HashMap<String, i64> = counts.into_iter().collect();
    let at_attempt_cap: i64 = sqlx::query_scalar(AT_CAP_SQL).fetch_one(pool).await?;
    tracing::info!(
        requeued_pending = result.requeued_pending,
        reset_stale = result.reset_stale,
        resumed_imports = result.resumed_imports,
        revived_paused = result.revived_paused,
        hung_active = result.hung_active,
        exhausted = result.exhausted,
        pruned_events = result.pruned_events,
        cleaned_jobs = result.cleaned_jobs,
        pruned_metrics,
        pending = counts.get("pending").copied().unwrap_or(0),
        analysing = counts.get("analysing").copied().unwrap_or(0),
        failed = counts.get("failed").copied().unwrap_or(0),
        at_attempt_cap,
        "reconcile sweep"
    );
    Ok(result)
}

/// One window of a sweep (see [`page_sql`]).
struct Page {
    /// The window's due games.
    due: Vec<GameRow>,
    /// The window's highest id: the next cursor (`None` when the window is empty).
    last: Option<Uuid>,
    /// Games in the window; fewer than the page size means the sweep is done.
    len: usize,
}

async fn fetch_page(
    pool: &PgPool,
    sql: &'static str,
    age_secs: f64,
    after: Option<Uuid>,
    limit: i64,
) -> Result<Page, sqlx::Error> {
    let rows: Vec<GameRow> = sqlx::query_as(sql)
        .bind(age_secs)
        .bind(after)
        .bind(limit)
        .fetch_all(pool)
        .await?;
    // Rust's `Uuid` order is Postgres's (bytewise), so the max is the window's last id.
    let last = rows.iter().map(|g| g.id).max();
    let len = rows.len();
    let due = rows.into_iter().filter(|g| g.due).collect();
    Ok(Page { due, last, len })
}

async fn live_states(
    queue: &Queue,
    page: &[GameRow],
) -> Result<HashMap<String, JobState>, QueueError> {
    let keys: Vec<String> = page.iter().map(|g| analyse_key(g.id)).collect();
    queue.live_states(KIND_ANALYSE, &keys).await
}

/// Enqueues analyse jobs for `games` (`(game_id, account_id)`) in one transaction with one
/// NOTIFY ([`Queue::enqueue_many`]), and returns how many were new (a live job already holds the
/// others' keys). Each `Queue::enqueue` is its own commit, and its NOTIFY serialises commits, so
/// requeueing a lost queue row by row is bound by commit latency (2,500 games took 10-25 s under
/// load); one commit per page keeps it to well under a second. The rows equal what
/// [`enqueue_analyse`] writes.
pub async fn enqueue_analysis_page(
    queue: &Queue,
    games: &[(Uuid, Uuid)],
) -> Result<u64, QueueError> {
    let jobs: Vec<(Value, EnqueueOptions)> = games
        .iter()
        .map(|&(game_id, account_id)| {
            (
                analyse_payload(game_id),
                analyse_enqueue_options(game_id, account_id),
            )
        })
        .collect();
    queue.enqueue_many(KIND_ANALYSE, &jobs).await
}

/// Marks `ids` failed if they are still `from`, and publishes `job.failed` for each one that
/// was. Returns how many were marked. A publish error is logged, not raised: the state change
/// has committed and the client refetches on its next load.
async fn give_up(pool: &PgPool, ids: &[Uuid], from: &str) -> Result<u64, sqlx::Error> {
    if ids.is_empty() {
        return Ok(0);
    }
    let failed: Vec<(Uuid, Uuid)> = sqlx::query_as(GIVE_UP_SQL)
        .bind(ids)
        .bind(from)
        .bind(EXHAUSTED_REASON)
        .fetch_all(pool)
        .await?;
    for (game_id, account_id) in &failed {
        let payload = json!({
            "kind": KIND_ANALYSE,
            "gameId": game_id,
            "reason": EXHAUSTED_REASON,
        });
        if let Err(e) = events::store::publish(
            pool,
            *account_id,
            events::store::UserEventType::JobFailed,
            payload,
        )
        .await
        {
            tracing::warn!(game_id = %game_id, error = %e, "publishing job.failed failed");
        }
    }
    Ok(failed.len() as u64)
}

/// Deletes `job_metrics` rows older than [`JOB_METRICS_RETENTION`] (served by
/// `job_metrics_created_idx`).
async fn prune_job_metrics(pool: &PgPool) -> Result<u64, sqlx::Error> {
    let done =
        sqlx::query("DELETE FROM job_metrics WHERE created_at < now() - make_interval(secs => $1)")
            .bind(JOB_METRICS_RETENTION.as_secs_f64())
            .execute(pool)
            .await?;
    Ok(done.rows_affected())
}

/// Schedules the recurring sweep (every [`RECONCILE_EVERY`]). Idempotent: the live dedupe key
/// keeps one reconcile job.
pub async fn ensure_reconcile_scheduled(queue: &Queue) -> Result<(), QueueError> {
    queue
        .enqueue(
            KIND_RECONCILE,
            &json!({}),
            EnqueueOptions {
                dedupe_key: Some(RECONCILE_KEY.to_owned()),
                priority: 0,
                max_attempts: 1,
                recur_every: Some(RECONCILE_EVERY),
                ..EnqueueOptions::default()
            },
        )
        .await?;
    Ok(())
}

/// The queue handler for [`KIND_RECONCILE`]: one sweep with the default options.
pub struct ReconcileHandler {
    pub pool: PgPool,
    pub queue: Queue,
}

impl Handler for ReconcileHandler {
    fn kind(&self) -> &'static str {
        KIND_RECONCILE
    }

    fn handle(&self, _job: Job, ctx: JobContext) -> BoxFuture<'_, Result<Value, JobError>> {
        async move {
            let opts = ReconcileOptions::default();
            let sweep = reconcile(&self.pool, &self.queue, &opts);
            tokio::select! {
                res = sweep => match res {
                    Ok(result) => Ok(json!(result)),
                    Err(e) => Err(JobError::Retryable {
                        message: e.to_string(),
                        code: None,
                    }),
                },
                // Every step is idempotent, so a sweep cut off part-way is safe to drop.
                () = ctx.cancel.cancelled() => Err(JobError::Release {
                    delay: Duration::ZERO,
                    reason: "shutdown".to_owned(),
                }),
            }
        }
        .boxed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attempt_cap_count_matches_the_partial_index_predicate() {
        assert!(AT_CAP_SQL.ends_with(&format!(">= {MAX_ANALYSIS_ATTEMPTS}")));
    }

    #[test]
    fn give_up_is_fenced_on_the_attempt_cap() {
        // A reanalysis resets attempts to 0 between the page read and this update; it must survive.
        assert!(GIVE_UP_SQL.contains(&format!("analysis_attempts >= {MAX_ANALYSIS_ATTEMPTS}")));
    }

    #[test]
    fn page_queries_use_their_age_columns() {
        assert!(PENDING_PAGE_SQL.contains("coalesce(pending_since, 'epoch'::timestamptz) < now()"));
        assert!(
            STALE_PAGE_SQL.contains(
                "coalesce(heartbeat_at, analysis_started_at, 'epoch'::timestamptz) < now()"
            )
        );
    }

    /// Both sweep windows read `games_pending_id_idx` in id order from the keyset cursor and stop after
    /// `LIMIT` rows: no Seq Scan and no Sort (top-N) over every pending row, in both the custom and
    /// the generic (cached) plan, for the first page (NULL cursor) and a later one.
    #[sqlx::test(migrator = "db::MIGRATOR")]
    async fn sweep_pages_are_index_ordered(pool: PgPool) {
        sqlx::query(
            "WITH a AS (INSERT INTO linked_accounts (platform, username, display_name) \
               VALUES ('lichess', 'plan', 'plan') RETURNING id) \
             INSERT INTO games (account_id, platform, platform_game_id, pgn, played_at, \
               time_class, rated, player_color, white_name, black_name, result, \
               analysis_status, pending_since) \
             SELECT a.id, 'lichess', 'p-' || i, '1. e4 *', now(), 'blitz', true, 'white', 'w', \
               'b', '*', CASE WHEN i % 10 < 3 THEN 'pending' WHEN i % 10 = 3 THEN 'analysing' \
               ELSE 'done' END::analysis_status, now() - interval '1 hour' \
             FROM a, generate_series(1, 100000) i",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("ANALYZE games").execute(&pool).await.unwrap();
        let mut conn = pool.acquire().await.unwrap();
        for (name, sql) in [("pend", PENDING_PAGE_SQL), ("stale", STALE_PAGE_SQL)] {
            sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
                "PREPARE {name}(float8, uuid, int8) AS {sql}"
            )))
            .execute(&mut *conn)
            .await
            .unwrap();
            for mode in ["force_custom_plan", "force_generic_plan"] {
                sqlx::query("SELECT set_config('plan_cache_mode', $1, false)")
                    .bind(mode)
                    .execute(&mut *conn)
                    .await
                    .unwrap();
                for after in ["NULL", "'7fffffff-ffff-ffff-ffff-ffffffffffff'"] {
                    let plan: Vec<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
                        "EXPLAIN EXECUTE {name}(60, {after}, 1000)"
                    )))
                    .fetch_all(&mut *conn)
                    .await
                    .unwrap();
                    let plan = plan.join("\n");
                    let at = format!("{name} {mode} after={after}:\n{plan}");
                    assert!(plan.contains("Scan using games_pending_id_idx"), "{at}");
                    assert!(!plan.contains("Seq Scan"), "{at}");
                    assert!(!plan.contains("Sort"), "{at}");
                }
            }
            sqlx::raw_sql(sqlx::AssertSqlSafe(format!("DEALLOCATE {name}")))
                .execute(&mut *conn)
                .await
                .unwrap();
        }
        sqlx::raw_sql("RESET plan_cache_mode")
            .execute(&mut *conn)
            .await
            .unwrap();
    }
}

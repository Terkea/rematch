//! The build-profile job (R2 Task 33, TS M2 Task 25): assembles an account × time class profile
//! from its analysed games.
//!
//! Steps ([`handle_build_profile`]):
//! 1. Record `started_at` from the deps' clock.
//! 2. Fingerprint the stored `game_metrics` rows, then load the current-version games
//!    ([`load_analysed_games`]; stale versions are no evidence), their series points (points of
//!    games left out are dropped too) and the band reference of the player's band (R-4 with
//!    ruling Q2, per-metric fallback of Task 17). Then take the build's advisory lock
//!    (`build-profile:<account>:<tc>`) in the write transaction and check the fingerprint again:
//!    if the games changed meanwhile, load again (optimistic, at most [`MAX_LOAD_ATTEMPTS`]).
//!    Builds of one account × time class therefore write one at a time, each from a view that
//!    was current when it took the lock, and a build never holds two connections.
//! 3. [`estimate_weakness`](profile_core::stats::priority::estimate_weakness) per binomial key
//!    ([`crate::estimates`]), upserted with the timeline's state and `P_imp`.
//! 4. [`evaluate_series`](profile_core::timeline::series::evaluate_series) per series; history
//!    rows whose `(kind, to, at_game_id)` are not stored are appended ([`crate::timeline_job`]).
//!    The stored keys are read under the lock, so concurrent builds never append twice.
//! 5. KPIs for `last50` and every week with games, appended when `value` or `n` changed, and
//! 6. area scores as `area:<area>` rows ([`crate::snapshots`]). Commit.
//! 7. After the commit, publish one `profile.updated` per changed benchmarked area and one
//!    `weakness.changed` per new transition row. Events are account-scoped (`user_events.
//!    account_id`); both carry the player's own statistics (see the R3 `user_id` note in the
//!    task report). Then [`rebuild_repertoire`] (its own lock and transaction).
//! 8. Follow-up (T31 hand-off): a job first gives up its dedupe key, then checks whether the
//!    stored games changed since its fingerprint or any game has `analysed_at > started_at`, and
//!    if so enqueues the next build. A game that commits after the key is released enqueues its
//!    own build (the hook's enqueue no longer meets a live job); one that committed before is
//!    seen by the check. So a game finishing mid-build is never left waiting. The fingerprint
//!    covers what timestamps cannot: `analysed_at` is the analysis transaction's start, which
//!    can precede `started_at` although the game committed after the load.
//! 9. Enqueue `schedule-analysis` for the account, and record the `job_metrics` row.
//!
//! The job is idempotent: the same games, reference and clock give the same estimates, and a
//! rerun appends no snapshot or history row and publishes no event.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;

use analysis::jobs::import::enqueue_schedule;
use chess_core::TimeClass;
use db::profile_rows::{append_kpi_snapshot, append_state_history, upsert_weakness_estimate};
use db::stage_metrics::{StageMetrics, record_stage_metrics};
use engine::ANALYSIS_PROTOCOL;
use events::store::{UserEventType, publish};
use futures::future::BoxFuture;
use profile_core::kpi::catalogue::KpiGame;
use profile_core::timeline::series::SeriesPoint;
use profile_core::types::{
    Band, BandReference, GameContext, PlayerGameMetrics, SeriesKey, SessionInfo, WeaknessState,
};
use profile_core::weakness::sessions::{
    SESSION_GAP_MINUTES, SessionGame, build_sessions, est_duration_seconds,
};
use queue::queue::Queue;
use queue::runner::{Handler, JobContext};
use queue::{BuildProfilePayload, Job, JobError, KIND_BUILD_PROFILE};
use refdata::band_reference::load_band_reference;
use refdata::builds::RefdataError;
use refdata::repertoire::{player_band, rebuild_repertoire};
use serde::Serialize;
use serde_json::{Value, json};
use sqlx::{PgConnection, PgPool};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::analyse_features::parse_time_class;
use crate::estimates::{compute_estimates, estimate_row, games_in_window};
use crate::hook_impl::enqueue_build_profile;
use crate::load::{LoadError, load_analysed_games, load_series_points};
use crate::snapshots::{
    changed_areas, compute_snapshots, latest_snapshots, profile_updated_payload, rows_to_append,
    snapshot_row,
};
use crate::timeline_job::{
    CauseFacts, evaluate_all, history_row, new_rows, stored_row_keys, weakness_changed_payload,
};

/// `job_metrics.kind` of the job.
pub const BUILD_PROFILE_KIND: &str = KIND_BUILD_PROFILE;
/// The stages of its `job_metrics` row.
pub const BUILD_PROFILE_STAGES: [&str; 6] = [
    "load_ms",
    "estimates_ms",
    "timeline_ms",
    "kpi_ms",
    "repertoire_ms",
    "persist_ms",
];
/// Loads whose view is no longer current under the lock before the build writes anyway (the
/// follow-up check then sees the change and builds again).
pub const MAX_LOAD_ATTEMPTS: usize = 3;

pub struct BuildProfileDeps {
    pub pool: PgPool,
    pub queue: Queue,
    /// The build's clock (`started_at`, ages of series points). `OffsetDateTime::now_utc` in
    /// production.
    pub now: fn() -> OffsetDateTime,
}

/// What a build wrote.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct BuildProfileSummary {
    /// `weakness_estimates` rows upserted (one per binomial key).
    pub estimates: usize,
    /// `kpi_snapshot` rows appended.
    pub snapshots: usize,
    /// `weakness_state_history` rows appended (transitions and onsets).
    pub transitions: usize,
}

fn retryable(what: &str, e: impl std::fmt::Display) -> JobError {
    JobError::Retryable {
        message: format!("build-profile {what}: {e}"),
        code: Some("db".to_owned()),
    }
}

fn unrecoverable(message: String, code: &str) -> JobError {
    JobError::Unrecoverable {
        message,
        code: Some(code.to_owned()),
    }
}

fn load_error(e: LoadError) -> JobError {
    match e {
        LoadError::Db(e) => retryable("load", e),
        LoadError::Decode(m) => unrecoverable(format!("build-profile: {m}"), "decode"),
    }
}

fn refdata_error(e: RefdataError) -> JobError {
    match e {
        RefdataError::NotFound(id) => unrecoverable(format!("no such account {id}"), "not_found"),
        e => retryable("reference data", e),
    }
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

fn add_stage(stages: &mut BTreeMap<String, f64>, name: &str, t: Instant) {
    *stages.entry(name.to_owned()).or_default() += ms(t);
}

/// A digest of the stored `game_metrics` rows of the account × time class: it changes whenever
/// a row is added, replaced (`created_at` is reset) or deleted.
pub async fn games_fingerprint(
    conn: &mut PgConnection,
    account_id: Uuid,
    tc: TimeClass,
) -> Result<(i64, String), sqlx::Error> {
    sqlx::query_as(
        "SELECT count(*), coalesce(md5(string_agg(game_id::text || '@' || created_at::text \
           || '@' || metrics_version::text, ',' ORDER BY game_id)), '') \
         FROM game_metrics WHERE account_id = $1 AND time_class = $2::time_class",
    )
    .bind(account_id)
    .bind(tc.as_str())
    .fetch_one(conn)
    .await
}

async fn pool_fingerprint(
    pool: &PgPool,
    account_id: Uuid,
    tc: TimeClass,
) -> Result<(i64, String), JobError> {
    let mut conn = pool
        .acquire()
        .await
        .map_err(|e| retryable("fingerprint", e))?;
    games_fingerprint(&mut conn, account_id, tc)
        .await
        .map_err(|e| retryable("fingerprint", e))
}

/// Takes the build's lock for the rest of `conn`'s transaction.
async fn lock_build(conn: &mut PgConnection, account_id: Uuid, tc: TimeClass) -> sqlx::Result<()> {
    sqlx::query(
        "SELECT pg_advisory_xact_lock(hashtext('build-profile:' || $1::text || ':' || $2))",
    )
    .bind(account_id)
    .bind(tc.as_str())
    .execute(conn)
    .await?;
    Ok(())
}

/// Gives up the running job's dedupe key (step 8). `false` when the job no longer holds its
/// lease or the key (nothing changed). The queue's live-only unique index then lets the next
/// enqueue for the key through while this job is still `running`.
async fn release_dedupe_key(pool: &PgPool, job: &Job) -> sqlx::Result<bool> {
    let done = sqlx::query(
        "UPDATE jobs SET dedupe_key = NULL WHERE id = $1 AND lease_token = $2 \
         AND state = 'running' AND dedupe_key IS NOT NULL",
    )
    .bind(job.id)
    .bind(job.lease_token)
    .execute(pool)
    .await?;
    Ok(done.rows_affected() > 0)
}

fn sessions(games: &[(GameContext, PlayerGameMetrics)]) -> HashMap<Uuid, SessionInfo> {
    let input: Vec<SessionGame> = games
        .iter()
        .map(|(ctx, m)| SessionGame {
            game_id: ctx.game_id,
            played_at: ctx.played_at,
            est_duration_seconds: est_duration_seconds(ctx.base_seconds, ctx.increment_seconds)
                .unwrap_or(0.0),
            score: ctx.score,
            expected_score: m.expected_score,
        })
        .collect();
    build_sessions(&input, SESSION_GAP_MINUTES)
        .into_iter()
        .map(|s| (s.game_id, s))
        .collect()
}

/// A build attempt's view of the account: what it loaded and the fingerprint taken first.
struct Loaded {
    fingerprint: (i64, String),
    user_id: Option<Uuid>,
    games: Vec<(GameContext, PlayerGameMetrics)>,
    series: BTreeMap<SeriesKey, Vec<SeriesPoint>>,
    band: Band,
    reference: Option<BandReference>,
    line_keys: HashMap<Uuid, String>,
}

/// Step 2's loads: lock-free, one connection at a time.
async fn load(pool: &PgPool, account_id: Uuid, tc: TimeClass) -> Result<Loaded, JobError> {
    let fingerprint = pool_fingerprint(pool, account_id, tc).await?;
    let user_id: Option<Uuid> =
        sqlx::query_scalar::<_, Option<Uuid>>("SELECT user_id FROM linked_accounts WHERE id = $1")
            .bind(account_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| retryable("account", e))?
            .ok_or_else(|| unrecoverable(format!("no such account {account_id}"), "not_found"))?;
    let games = load_analysed_games(pool, account_id, tc)
        .await
        .map_err(load_error)?;
    let loaded: HashSet<Uuid> = games.iter().map(|(c, _)| c.game_id).collect();
    let mut series = load_series_points(pool, account_id, tc)
        .await
        .map_err(load_error)?;
    // Points of a game whose metrics are stale (or missing) are no evidence either.
    for points in series.values_mut() {
        points.retain(|p| loaded.contains(&p.game_id));
    }
    series.retain(|_, pts| !pts.is_empty());
    let (band, _source) = player_band(pool, account_id, tc)
        .await
        .map_err(refdata_error)?;
    let reference = load_band_reference(pool, band, tc)
        .await
        .map_err(refdata_error)?
        .map(|r| r.reference());
    let line_keys: HashMap<Uuid, String> = sqlx::query_as::<_, (Uuid, String)>(
        "SELECT game_id, line_key FROM game_openings WHERE account_id = $1 \
         AND time_class = $2::time_class AND line_key IS NOT NULL",
    )
    .bind(account_id)
    .bind(tc.as_str())
    .fetch_all(pool)
    .await
    .map_err(|e| retryable("openings", e))?
    .into_iter()
    .collect();
    Ok(Loaded {
        fingerprint,
        user_id,
        games,
        series,
        band,
        reference,
        line_keys,
    })
}

/// Runs one build (module docs). The runner's [`BuildProfileHandler`] passes its job, so the
/// job's dedupe key is given up before the follow-up check.
pub async fn handle_build_profile(
    deps: &BuildProfileDeps,
    payload: BuildProfilePayload,
) -> Result<BuildProfileSummary, JobError> {
    run_build(deps, &payload, None).await
}

async fn run_build(
    deps: &BuildProfileDeps,
    payload: &BuildProfilePayload,
    job: Option<&Job>,
) -> Result<BuildProfileSummary, JobError> {
    let total = Instant::now();
    let account_id = payload.account_id;
    let tc = parse_time_class(&payload.time_class).ok_or_else(|| {
        unrecoverable(
            format!("unknown time class {:?}", payload.time_class),
            "bad_payload",
        )
    })?;
    let pool = &deps.pool;
    let started_at = (deps.now)();
    let mut stages: BTreeMap<String, f64> = BUILD_PROFILE_STAGES
        .iter()
        .map(|s| ((*s).to_owned(), 0.0))
        .collect();

    // 2. Load, lock, and check that the view is still current.
    let mut attempt = 0;
    let (l, mut tx) = loop {
        attempt += 1;
        let t = Instant::now();
        let l = load(pool, account_id, tc).await?;
        let mut tx = pool.begin().await.map_err(|e| retryable("begin", e))?;
        lock_build(&mut tx, account_id, tc)
            .await
            .map_err(|e| retryable("lock", e))?;
        let current = games_fingerprint(&mut tx, account_id, tc)
            .await
            .map_err(|e| retryable("fingerprint", e))?;
        add_stage(&mut stages, "load_ms", t);
        if current == l.fingerprint || attempt >= MAX_LOAD_ATTEMPTS {
            break (l, tx);
        }
        tx.rollback().await.map_err(|e| retryable("rollback", e))?;
    };
    let t = Instant::now();
    let stored_rows = stored_row_keys(&mut tx, account_id, tc)
        .await
        .map_err(|e| retryable("history", e))?;
    let latest = latest_snapshots(&mut tx, account_id, tc)
        .await
        .map_err(|e| retryable("snapshots", e))?;
    add_stage(&mut stages, "load_ms", t);

    // 3. Estimates.
    let t = Instant::now();
    let in_window = games_in_window(l.games.iter().map(|(c, _)| c.played_at), started_at);
    let estimates = compute_estimates(
        &l.series,
        in_window,
        l.band,
        l.reference.as_ref(),
        started_at,
    );
    add_stage(&mut stages, "estimates_ms", t);

    // 4. Timeline.
    let t = Instant::now();
    let outcomes = evaluate_all(&l.series, l.reference.as_ref());
    let contexts: HashMap<Uuid, &GameContext> =
        l.games.iter().map(|(c, _)| (c.game_id, c)).collect();
    let facts = CauseFacts {
        games: &contexts,
        line_keys: &l.line_keys,
    };
    let fresh = new_rows(&l.series, &outcomes, &stored_rows, &facts);
    add_stage(&mut stages, "timeline_ms", t);

    // 5-6. KPIs and area scores.
    let t = Instant::now();
    let session_map = sessions(&l.games);
    let kpi_games: Vec<KpiGame<'_>> = l
        .games
        .iter()
        .map(|(ctx, metrics)| KpiGame {
            ctx,
            metrics,
            session: session_map.get(&ctx.game_id),
        })
        .collect();
    let snapshot_values = compute_snapshots(&kpi_games, l.reference.as_ref());
    let appended = rows_to_append(&snapshot_values, &latest);
    add_stage(&mut stages, "kpi_ms", t);

    // Persist, then commit (which releases the lock).
    let t = Instant::now();
    let protocols: HashMap<Uuid, &str> = l
        .series
        .values()
        .flatten()
        .map(|p| (p.game_id, p.protocol.as_str()))
        .collect();
    for e in &estimates {
        let (state, p_imp) = outcomes
            .get(&SeriesKey::Weakness(e.key))
            .map_or((WeaknessState::NotAnIssue, None), |o| (o.state, o.p_imp));
        upsert_weakness_estimate(&mut tx, &estimate_row(account_id, tc, e, state, p_imp))
            .await
            .map_err(|e| retryable("estimates", e))?;
    }
    for r in &fresh {
        let protocol = protocols
            .get(&r.row.at_game_id)
            .copied()
            .unwrap_or(ANALYSIS_PROTOCOL);
        append_state_history(&mut tx, &history_row(account_id, tc, r, protocol))
            .await
            .map_err(|e| retryable("history", e))?;
    }
    for v in &appended {
        append_kpi_snapshot(
            &mut tx,
            &snapshot_row(l.user_id, account_id, tc, v, ANALYSIS_PROTOCOL),
        )
        .await
        .map_err(|e| retryable("snapshots", e))?;
    }
    tx.commit().await.map_err(|e| retryable("commit", e))?;
    add_stage(&mut stages, "persist_ms", t);

    let summary = BuildProfileSummary {
        estimates: estimates.len(),
        snapshots: appended.len(),
        transitions: fresh.len(),
    };

    // 7. Events, right after the commit. They notify; the state is in Postgres, so a failed
    // publish is logged, not retried (a retry would find nothing new to announce).
    for area in changed_areas(&appended) {
        let p = profile_updated_payload(tc, area, &snapshot_values);
        if let Err(e) = publish(pool, account_id, UserEventType::ProfileUpdated, p).await {
            tracing::warn!(%account_id, area = area.as_str(), error = %e, "publish profile.updated failed");
        }
    }
    for r in &fresh {
        if let Some(p) = weakness_changed_payload(tc, r)
            && let Err(e) = publish(pool, account_id, UserEventType::WeaknessChanged, p).await
        {
            tracing::warn!(%account_id, key = %r.key, error = %e, "publish weakness.changed failed");
        }
    }
    // The repertoire has its own lock and transaction. A failure retries the job, which finds
    // the profile rows written and rebuilds the repertoire.
    let t = Instant::now();
    rebuild_repertoire(pool, account_id, tc)
        .await
        .map_err(refdata_error)?;
    add_stage(&mut stages, "repertoire_ms", t);

    // 8. Follow-up build.
    if let Some(job) = job {
        release_dedupe_key(pool, job)
            .await
            .map_err(|e| retryable("release dedupe key", e))?;
    }
    let now_print = pool_fingerprint(pool, account_id, tc).await?;
    let analysed_later: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM games WHERE account_id = $1 \
         AND time_class = $2::time_class AND analysed_at > $3)",
    )
    .bind(account_id)
    .bind(tc.as_str())
    .bind(started_at)
    .fetch_one(pool)
    .await
    .map_err(|e| retryable("follow-up check", e))?;
    if now_print != l.fingerprint || analysed_later {
        enqueue_build_profile(&deps.queue, account_id, tc)
            .await
            .map_err(|e| retryable("enqueue follow-up", e))?;
    }

    // 9. Schedule more analysis (precision may stop backfill), and the metrics row.
    enqueue_schedule(&deps.queue, account_id)
        .await
        .map_err(|e| retryable("enqueue schedule-analysis", e))?;
    let m = StageMetrics {
        job_id: job.map(|j| j.id),
        kind: BUILD_PROFILE_KIND.to_owned(),
        game_id: None,
        account_id: Some(account_id),
        total_ms: ms(total),
        stages,
    };
    if let Err(e) = record_stage_metrics(pool, &m).await {
        tracing::warn!(%account_id, error = %e, "record build-profile job_metrics failed");
    }
    Ok(summary)
}

/// The queue handler for [`KIND_BUILD_PROFILE`].
pub struct BuildProfileHandler(pub Arc<BuildProfileDeps>);

impl Handler for BuildProfileHandler {
    fn kind(&self) -> &'static str {
        KIND_BUILD_PROFILE
    }

    fn handle(&self, job: Job, ctx: JobContext) -> BoxFuture<'_, Result<Value, JobError>> {
        Box::pin(async move {
            let payload: BuildProfilePayload = serde_json::from_value(job.payload.clone())
                .map_err(|e| {
                    unrecoverable(format!("bad build-profile payload: {e}"), "bad_payload")
                })?;
            tokio::select! {
                res = run_build(&self.0, &payload, Some(&job)) => {
                    let s = res?;
                    Ok(json!({
                        "estimates": s.estimates,
                        "snapshots": s.snapshots,
                        "transitions": s.transitions,
                    }))
                }
                // Dropping the build before its commit rolls the transaction back.
                () = ctx.cancel.cancelled() => Err(JobError::Release {
                    delay: std::time::Duration::ZERO,
                    reason: "cancelled".to_owned(),
                }),
            }
        })
    }
}

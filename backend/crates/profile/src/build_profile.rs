//! The build-profile job (R2 Task 33, TS M2 Task 25): assembles an account × time class profile
//! from its analysed games.
//!
//! Steps ([`handle_build_profile`]):
//! 1. Record `started_at` from the deps' clock.
//! 2. Read the band reference of the player's band (R-4 with ruling Q2, per-metric fallback of
//!    Task 17). It is reference data, not account state, so it is read before the lock. Then
//!    open the write transaction and take the build's advisory lock
//!    (`build-profile:<account>:<tc>`). On the transaction's own connection, fingerprint the
//!    stored `game_metrics` rows and load:
//!    - the current-version games ([`load_analysed_games_in`]; stale versions are no evidence);
//!    - their series points (points of games left out are dropped too);
//!    - the band again (re-read under the lock; if it moved, the reference is read again);
//!    - the line keys, the stored history, the stored estimate states and the latest
//!      snapshots.
//!
//!    Builds of one account × time class therefore run one at a time, each from one consistent
//!    view, on one connection (T33 review M1; the earlier optimistic re-load is gone).
//! 3. to 6. The CPU-heavy part runs in `spawn_blocking` (review M2):
//!    - [`estimate_weakness`](profile_core::stats::priority::estimate_weakness) per binomial
//!      key ([`crate::estimates`]), upserted with the timeline's state and `P_imp`;
//!    - [`evaluate_series`](profile_core::timeline::series::evaluate_series) per series. The
//!      stored history is made equal to the replay: stale rows deleted, missing rows inserted
//!      ([`crate::timeline_job`], review I1 ruling a);
//!    - KPIs for `last50` and every week with games, and area scores as `area:<area>` rows,
//!      appended when a compared field changed ([`crate::snapshots`]).
//!
//!    Then write and commit.
//! 7. After the commit, publish:
//!    - one `profile.updated` per changed benchmarked area;
//!    - one `weakness.changed` per series whose current state differs from the stored one.
//!
//!    Events are account-scoped (`user_events.account_id`); both carry the player's own
//!    statistics (see the R3 `user_id` note in the task report). Then [`rebuild_repertoire`],
//!    which has its own lock and transaction and holds two connections (Task 18's code).
//! 8. Follow-up (T31 hand-off). A job first gives up its dedupe key. It then checks whether the
//!    stored games changed since its fingerprint, or any game has `analysed_at > started_at`;
//!    if so, it enqueues the next build.
//!    - A game that commits after the key is released enqueues its own build: the hook's
//!      enqueue no longer meets a live job.
//!    - A game that committed before the release is seen by the check.
//!
//!    So a game finishing mid-build is never left waiting. The release must come first: before
//!    it, the job's own enqueue would dedupe against the running job. The fingerprint covers
//!    what timestamps cannot: `analysed_at` is the analysis transaction's start, which can
//!    precede `started_at` although the game committed after the load.
//! 9. Enqueue `schedule-analysis` for the account, and record the `job_metrics` row.
//!
//! The job is idempotent. The same games, reference and clock give the same estimates, and a
//! rerun writes no snapshot or history row and publishes no event.

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
use profile_core::kpi::snapshots::KpiValue;
use profile_core::stats::priority::WeaknessEstimate;
use profile_core::timeline::series::{SeriesOutcome, SeriesPoint};
use profile_core::types::{
    BandReference, GameContext, PlayerGameMetrics, SeriesKey, SessionInfo, WeaknessState,
};
use profile_core::weakness::sessions::{
    SESSION_GAP_MINUTES, SessionGame, build_sessions, est_duration_seconds,
};
use queue::queue::Queue;
use queue::runner::{Handler, JobContext};
use queue::{BuildProfilePayload, Job, JobError, KIND_BUILD_PROFILE};
use refdata::band_reference::load_band_reference;
use refdata::builds::RefdataError;
use refdata::repertoire::{player_band, player_band_in, rebuild_repertoire};
use serde::Serialize;
use serde_json::{Value, json};
use sqlx::{PgConnection, PgPool};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::analyse_features::parse_time_class;
use crate::estimates::{compute_estimates, estimate_row, games_in_window};
use crate::hook_impl::enqueue_build_profile;
use crate::load::{LoadError, load_analysed_games_in, load_series_points_in};
use crate::snapshots::{
    Latest, changed_areas, compute_snapshots, latest_snapshots, profile_updated_payload,
    rows_to_append, snapshot_row,
};
use crate::timeline_job::{
    CauseFacts, NewRow, StateChange, StoredHistory, delete_history_rows, evaluate_all, history_row,
    new_rows, stale_row_ids, state_changes, stored_estimate_states, stored_history,
    weakness_changed_payload,
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
    /// `weakness_state_history` rows inserted (transitions and onsets). Stale rows deleted to
    /// match the replay are not counted.
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

/// Sessions of the loaded games. Daily games have no sessions (no game duration is known, and a
/// duration of 0 would claim one).
fn sessions(
    tc: TimeClass,
    games: &[(GameContext, PlayerGameMetrics)],
) -> HashMap<Uuid, SessionInfo> {
    if tc == TimeClass::Daily {
        return HashMap::new();
    }
    let input: Vec<SessionGame> = games
        .iter()
        .filter_map(|(ctx, m)| {
            Some(SessionGame {
                game_id: ctx.game_id,
                played_at: ctx.played_at,
                est_duration_seconds: est_duration_seconds(
                    ctx.base_seconds,
                    ctx.increment_seconds,
                )?,
                score: ctx.score,
                expected_score: m.expected_score,
            })
        })
        .collect();
    build_sessions(&input, SESSION_GAP_MINUTES)
        .into_iter()
        .map(|s| (s.game_id, s))
        .collect()
}

/// What the build read under its lock.
struct Loaded {
    fingerprint: (i64, String),
    user_id: Option<Uuid>,
    games: Vec<(GameContext, PlayerGameMetrics)>,
    series: BTreeMap<SeriesKey, Vec<SeriesPoint>>,
    band: profile_core::types::Band,
    reference: Option<BandReference>,
    line_keys: HashMap<Uuid, String>,
    stored: StoredHistory,
    stored_states: HashMap<String, WeaknessState>,
    latest: Latest,
}

/// Step 2 on the locked transaction. `prefetched` is the band and reference read before the
/// lock; the reference is read again only if the band moved meanwhile (the only moment a second
/// connection is used, briefly).
async fn load_locked(
    conn: &mut PgConnection,
    pool: &PgPool,
    account_id: Uuid,
    tc: TimeClass,
    prefetched: (profile_core::types::Band, Option<BandReference>),
) -> Result<Loaded, JobError> {
    let fingerprint = games_fingerprint(conn, account_id, tc)
        .await
        .map_err(|e| retryable("fingerprint", e))?;
    let user_id: Option<Uuid> =
        sqlx::query_scalar::<_, Option<Uuid>>("SELECT user_id FROM linked_accounts WHERE id = $1")
            .bind(account_id)
            .fetch_optional(&mut *conn)
            .await
            .map_err(|e| retryable("account", e))?
            .ok_or_else(|| unrecoverable(format!("no such account {account_id}"), "not_found"))?;
    let games = load_analysed_games_in(conn, account_id, tc)
        .await
        .map_err(load_error)?;
    let loaded: HashSet<Uuid> = games.iter().map(|(c, _)| c.game_id).collect();
    let mut series = load_series_points_in(conn, account_id, tc)
        .await
        .map_err(load_error)?;
    // Points of a game whose metrics are stale (or missing) are no evidence either.
    for points in series.values_mut() {
        points.retain(|p| loaded.contains(&p.game_id));
    }
    series.retain(|_, pts| !pts.is_empty());
    let (band, _source) = player_band_in(conn, account_id, tc)
        .await
        .map_err(refdata_error)?;
    let reference = if band == prefetched.0 {
        prefetched.1
    } else {
        load_band_reference(pool, band, tc)
            .await
            .map_err(refdata_error)?
            .map(|r| r.reference())
    };
    let line_keys: HashMap<Uuid, String> = sqlx::query_as::<_, (Uuid, String)>(
        "SELECT game_id, line_key FROM game_openings WHERE account_id = $1 \
         AND time_class = $2::time_class AND line_key IS NOT NULL",
    )
    .bind(account_id)
    .bind(tc.as_str())
    .fetch_all(&mut *conn)
    .await
    .map_err(|e| retryable("openings", e))?
    .into_iter()
    .collect();
    let stored = stored_history(conn, account_id, tc)
        .await
        .map_err(|e| retryable("history", e))?;
    let stored_states = stored_estimate_states(conn, account_id, tc)
        .await
        .map_err(|e| retryable("estimates", e))?;
    let latest = latest_snapshots(conn, account_id, tc)
        .await
        .map_err(|e| retryable("snapshots", e))?;
    Ok(Loaded {
        fingerprint,
        user_id,
        games,
        series,
        band,
        reference,
        line_keys,
        stored,
        stored_states,
        latest,
    })
}

/// The pure result of steps 3 to 6.
struct Computed {
    estimates: Vec<WeaknessEstimate>,
    outcomes: BTreeMap<SeriesKey, SeriesOutcome>,
    fresh: Vec<NewRow>,
    stale_ids: Vec<i64>,
    changes: Vec<StateChange>,
    snapshot_values: Vec<KpiValue>,
    appended: Vec<KpiValue>,
    protocols: HashMap<Uuid, String>,
    estimates_ms: f64,
    timeline_ms: f64,
    kpi_ms: f64,
}

fn compute(tc: TimeClass, l: &Loaded, started_at: OffsetDateTime) -> Computed {
    let t = Instant::now();
    let in_window = games_in_window(l.games.iter().map(|(c, _)| c.played_at), started_at);
    let estimates = compute_estimates(
        &l.series,
        in_window,
        l.band,
        l.reference.as_ref(),
        started_at,
    );
    let estimates_ms = ms(t);

    let t = Instant::now();
    let outcomes = evaluate_all(&l.series, l.reference.as_ref());
    let contexts: HashMap<Uuid, &GameContext> =
        l.games.iter().map(|(c, _)| (c.game_id, c)).collect();
    let facts = CauseFacts {
        games: &contexts,
        line_keys: &l.line_keys,
    };
    let fresh = new_rows(&l.series, &outcomes, &l.stored, &facts);
    let stale_ids = stale_row_ids(&outcomes, &l.stored);
    let changes = state_changes(&outcomes, &l.stored_states, &l.stored);
    let timeline_ms = ms(t);

    let t = Instant::now();
    let session_map = sessions(tc, &l.games);
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
    let appended: Vec<KpiValue> = rows_to_append(&snapshot_values, &l.latest)
        .into_iter()
        .cloned()
        .collect();
    let kpi_ms = ms(t);

    let protocols = l
        .series
        .values()
        .flatten()
        .map(|p| (p.game_id, p.protocol.clone()))
        .collect();
    Computed {
        estimates,
        outcomes,
        fresh,
        stale_ids,
        changes,
        snapshot_values,
        appended,
        protocols,
        estimates_ms,
        timeline_ms,
        kpi_ms,
    }
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

    // 2. Reference data first, then the lock and the account's state on one connection.
    let t = Instant::now();
    let (band0, _) = player_band(pool, account_id, tc)
        .await
        .map_err(refdata_error)?;
    let reference0 = load_band_reference(pool, band0, tc)
        .await
        .map_err(refdata_error)?
        .map(|r| r.reference());
    let mut tx = pool.begin().await.map_err(|e| retryable("begin", e))?;
    lock_build(&mut tx, account_id, tc)
        .await
        .map_err(|e| retryable("lock", e))?;
    let l = load_locked(&mut tx, pool, account_id, tc, (band0, reference0)).await?;
    stages.insert("load_ms".into(), ms(t));

    // 3-6. The CPU-heavy part, off the async executor.
    let (l, c) = tokio::task::spawn_blocking(move || {
        let c = compute(tc, &l, started_at);
        (l, c)
    })
    .await
    .map_err(|e| {
        // A panic is deterministic for the same inputs: retrying it only delays the dead
        // letter. Any other join error (the runtime shutting down) is retried.
        if e.is_panic() {
            unrecoverable(format!("build-profile compute panicked: {e}"), "panic")
        } else {
            retryable("compute", e)
        }
    })?;
    stages.insert("estimates_ms".into(), c.estimates_ms);
    stages.insert("timeline_ms".into(), c.timeline_ms);
    stages.insert("kpi_ms".into(), c.kpi_ms);

    // Persist, then commit (which releases the lock).
    let t = Instant::now();
    for e in &c.estimates {
        let (state, p_imp) = c
            .outcomes
            .get(&SeriesKey::Weakness(e.key))
            .map_or((WeaknessState::NotAnIssue, None), |o| (o.state, o.p_imp));
        upsert_weakness_estimate(&mut tx, &estimate_row(account_id, tc, e, state, p_imp))
            .await
            .map_err(|e| retryable("estimates", e))?;
    }
    let deleted = delete_history_rows(&mut tx, account_id, tc, &c.stale_ids)
        .await
        .map_err(|e| retryable("history", e))?;
    for r in &c.fresh {
        let protocol = c
            .protocols
            .get(&r.row.at_game_id)
            .map_or(ANALYSIS_PROTOCOL, String::as_str);
        append_state_history(&mut tx, &history_row(account_id, tc, r, protocol))
            .await
            .map_err(|e| retryable("history", e))?;
    }
    for v in &c.appended {
        append_kpi_snapshot(
            &mut tx,
            &snapshot_row(l.user_id, account_id, tc, v, ANALYSIS_PROTOCOL),
        )
        .await
        .map_err(|e| retryable("snapshots", e))?;
    }
    tx.commit().await.map_err(|e| retryable("commit", e))?;
    stages.insert("persist_ms".into(), ms(t));
    if deleted > 0 {
        tracing::info!(%account_id, tc = tc.as_str(), deleted, "history rows replaced by the replay");
    }

    let summary = BuildProfileSummary {
        estimates: c.estimates.len(),
        snapshots: c.appended.len(),
        transitions: c.fresh.len(),
    };

    // 7. Events, right after the commit. They notify; the state is in Postgres, so a failed
    // publish is logged, not retried (a retry would find nothing new to announce).
    let appended: Vec<&KpiValue> = c.appended.iter().collect();
    for area in changed_areas(&appended) {
        let p = profile_updated_payload(tc, area, &c.snapshot_values);
        if let Err(e) = publish(pool, account_id, UserEventType::ProfileUpdated, p).await {
            tracing::warn!(%account_id, area = area.as_str(), error = %e, "publish profile.updated failed");
        }
    }
    for change in &c.changes {
        let p = weakness_changed_payload(tc, change);
        if let Err(e) = publish(pool, account_id, UserEventType::WeaknessChanged, p).await {
            tracing::warn!(%account_id, key = %change.key, error = %e, "publish weakness.changed failed");
        }
    }
    // The repertoire has its own lock and transaction. A failure retries the job, which finds
    // the profile rows written and rebuilds the repertoire.
    let t = Instant::now();
    rebuild_repertoire(pool, account_id, tc)
        .await
        .map_err(refdata_error)?;
    stages.insert("repertoire_ms".into(), ms(t));

    // 8. Follow-up build: release the key first, then check, then enqueue.
    if let Some(job) = job {
        deps.queue
            .release_dedupe_key(job)
            .await
            .map_err(|e| retryable("release dedupe key", e))?;
    }
    let mut conn = pool
        .acquire()
        .await
        .map_err(|e| retryable("follow-up check", e))?;
    let now_print = games_fingerprint(&mut conn, account_id, tc)
        .await
        .map_err(|e| retryable("fingerprint", e))?;
    let analysed_later: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM games WHERE account_id = $1 \
         AND time_class = $2::time_class AND analysed_at > $3)",
    )
    .bind(account_id)
    .bind(tc.as_str())
    .bind(started_at)
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| retryable("follow-up check", e))?;
    drop(conn);
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

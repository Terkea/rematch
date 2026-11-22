//! The schedule-analysis job (R2 Task 29, ADR-R2-16; TS M2 Task 23 `handleScheduleAnalysis`).
//!
//! The importer stores new games as `imported` and enqueues one `schedule-analysis` job per
//! account after its run commits. This job:
//! 1. locks the account row and loads every game of the account (the selection needs the
//!    primary time class, the tier counts and the in-flight backfill) plus its weakness
//!    estimates;
//! 2. stops backfill (P1/P2) for every time class whose estimates are precise ([`is_precise`]);
//! 3. runs [`select_for_analysis`] and, in the same transaction, sets the selected games
//!    `pending` with `analysis_tier`, `analysis_priority`, `selected_at` and `pending_since`
//!    (fenced on `imported`) and clears the "waiting for the scheduler" marker (`pending_since`)
//!    of every other `imported` game it read (unless there is no primary time class);
//! 4. after the commit, enqueues one analyse job per selected game at its priority (one
//!    [`enqueue_analysis_page_with_priority`] batch: the rows
//!    [`analysis::jobs::analyse::enqueue_analyse_with_priority`] writes, in one commit) and, on
//!    the first run, previews for the account's first [`PREVIEW_GAMES`] tier-0 games (read
//!    back from the database), and only then sets `analysis_scheduled_at`, so a failed preview
//!    enqueue is retried as a first run.
//!
//! Mapping onto [`select_for_analysis`] (whose candidates are `Pending` with no tier):
//! - `imported` rows are passed as `Pending` with no tier;
//! - `pending` rows are passed as `Analysing`: they are already queued (R1 imported every game as
//!   `pending` and enqueued it), so they are never re-selected;
//! - `created_at` is the game's `played_at`. `games` has no insertion time, and a game played
//!   after the first run is necessarily imported after it; a backfilled older game therefore never
//!   spends the P3 (new games) budget.
//!
//! An enqueue that fails after the commit leaves `pending` games without a job; the reconciler
//! re-enqueues them at their stored priority. A game whose marker is still set (a run that read
//! the account before the game was committed) is scheduled again by the reconciler.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use analysis::jobs::reconcile::enqueue_analysis_page_with_priority;
use chess_core::TimeClass;
use db::games::AnalysisStatus;
use futures::FutureExt;
use futures::future::BoxFuture;
use profile_core::types::WeaknessKey;
use queue::queue::Queue;
use queue::runner::{Handler, JobContext};
use queue::{Job, JobError, KIND_SCHEDULE, SchedulePayload};
use serde::Serialize;
use serde_json::Value;
use sqlx::PgPool;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

use crate::precision::{PrecisionInput, is_precise};
use crate::preview::enqueue_previews;
use crate::select::{
    CandidateGame, PREVIEW_GAMES, Plan, SelectInput, primary_time_class, select_for_analysis,
};

/// The rolling window of the P3 (new games) budget.
pub const P3_WINDOW: Duration = Duration::days(7);

pub struct ScheduleDeps {
    pub pool: PgPool,
    pub queue: Queue,
    /// The clock of the selection (`OffsetDateTime::now_utc` in production).
    pub now: fn() -> OffsetDateTime,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleSummary {
    /// Games set `pending` by this run.
    pub selected: usize,
    /// Of those, per tier 0..=3.
    pub by_tier: [usize; 4],
    /// Preview jobs newly queued (first run only).
    pub previews: usize,
}

/// `"beta"` (or none: the default until M7), `"free"` or `"pro"`; anything else is a bad
/// payload.
pub fn parse_plan(plan: Option<&str>) -> Option<Plan> {
    match plan {
        None | Some("beta") => Some(Plan::Beta),
        Some("free") => Some(Plan::Free),
        Some("pro") => Some(Plan::Pro),
        Some(_) => None,
    }
}

fn parse_time_class(s: &str) -> Option<TimeClass> {
    Some(match s {
        "bullet" => TimeClass::Bullet,
        "blitz" => TimeClass::Blitz,
        "rapid" => TimeClass::Rapid,
        "classical" => TimeClass::Classical,
        "daily" => TimeClass::Daily,
        _ => return None,
    })
}

#[derive(sqlx::FromRow)]
struct GameRow {
    id: Uuid,
    time_class: String,
    played_at: OffsetDateTime,
    rated: bool,
    ply_count: Option<i32>,
    has_lichess_evals: bool,
    analysis_status: String,
    analysis_tier: Option<i16>,
    selected_at: Option<OffsetDateTime>,
    waiting: bool,
}

#[derive(sqlx::FromRow)]
struct EstimateRow {
    time_class: String,
    series_key: String,
    ci_low: f64,
    ci_high: f64,
    band_median: Option<f64>,
    n_unweighted: i32,
}

const GAMES_SQL: &str = "\
SELECT id, time_class::text AS time_class, played_at, rated, ply_count,
  lichess_evals IS NOT NULL AS has_lichess_evals, analysis_status::text AS analysis_status,
  analysis_tier, selected_at, pending_since IS NOT NULL AS waiting
FROM games WHERE account_id = $1";

const ESTIMATES_SQL: &str = "\
SELECT time_class::text AS time_class, series_key, ci_low, ci_high, band_median, n_unweighted
FROM weakness_estimates WHERE account_id = $1";

/// Sets the selected games pending, fenced on `imported`, and returns what it promoted.
const PROMOTE_SQL: &str = "\
UPDATE games g SET analysis_status = 'pending', analysis_tier = s.tier,
  analysis_priority = s.priority, selected_at = $4, pending_since = now(), analysis_error = NULL
FROM unnest($1::uuid[], $2::int2[], $3::int2[]) AS s(id, tier, priority)
WHERE g.id = s.id AND g.account_id = $5 AND g.analysis_status = 'imported'
RETURNING g.id, g.analysis_tier, g.analysis_priority";

/// The account's first tier-0 games still to be analysed, in the order their analyses run.
const FIRST_PREVIEWS_SQL: &str = "\
SELECT id FROM games
WHERE account_id = $1 AND analysis_tier = 0 AND analysis_status IN ('pending', 'analysing')
ORDER BY analysis_priority, rated DESC, played_at DESC, id LIMIT $2";

/// Clears the marker of `imported` games this run read and did not select.
const SEEN_SQL: &str = "\
UPDATE games SET pending_since = NULL
WHERE id = ANY($1) AND analysis_status = 'imported'";

fn retryable(message: String) -> JobError {
    JobError::Retryable {
        message,
        code: None,
    }
}

fn db_err(e: sqlx::Error) -> JobError {
    retryable(format!("database error: {e}"))
}

/// Selects the account's next games for analysis (see the module docs).
pub async fn handle_schedule(
    deps: &ScheduleDeps,
    payload: SchedulePayload,
) -> Result<ScheduleSummary, JobError> {
    let plan = parse_plan(payload.plan.as_deref()).ok_or_else(|| JobError::Unrecoverable {
        message: format!("invalid schedule-analysis plan {:?}", payload.plan),
        code: None,
    })?;
    let account_id = payload.account_id;
    let now = (deps.now)();

    let mut tx = deps.pool.begin().await.map_err(db_err)?;
    let scheduled_at: Option<Option<OffsetDateTime>> = sqlx::query_scalar(
        "SELECT analysis_scheduled_at FROM linked_accounts WHERE id = $1 FOR UPDATE",
    )
    .bind(account_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(db_err)?;
    let Some(scheduled_at) = scheduled_at else {
        return Err(JobError::Unrecoverable {
            message: format!("Linked account not found: {account_id}"),
            code: None,
        });
    };
    let first_run = scheduled_at.is_none();

    let rows: Vec<GameRow> = sqlx::query_as(GAMES_SQL)
        .bind(account_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(db_err)?;
    let estimates: Vec<EstimateRow> = sqlx::query_as(ESTIMATES_SQL)
        .bind(account_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(db_err)?;

    let mut games = Vec::with_capacity(rows.len());
    let mut in_flight_backfill: HashMap<TimeClass, u32> = HashMap::new();
    let mut p3_selected_last_7d: u32 = 0;
    let mut waiting: Vec<Uuid> = Vec::new();
    for r in &rows {
        let Some(time_class) = parse_time_class(&r.time_class) else {
            continue;
        };
        let status: AnalysisStatus = r
            .analysis_status
            .parse()
            .map_err(|e| retryable(format!("{e}")))?;
        let live = matches!(status, AnalysisStatus::Pending | AnalysisStatus::Analysing);
        if live && matches!(r.analysis_tier, Some(1 | 2)) {
            *in_flight_backfill.entry(time_class).or_insert(0) += 1;
        }
        if r.analysis_tier == Some(3) && r.selected_at.is_some_and(|t| t > now - P3_WINDOW) {
            p3_selected_last_7d += 1;
        }
        if status == AnalysisStatus::Imported && r.waiting {
            waiting.push(r.id);
        }
        let (status, tier) = match status {
            AnalysisStatus::Imported => (AnalysisStatus::Pending, None),
            AnalysisStatus::Pending => (AnalysisStatus::Analysing, r.analysis_tier),
            other => (other, r.analysis_tier),
        };
        games.push(CandidateGame {
            id: r.id,
            time_class,
            played_at: r.played_at,
            created_at: r.played_at,
            rated: r.rated,
            ply_count: r.ply_count,
            has_lichess_evals: r.has_lichess_evals,
            status,
            tier,
        });
    }
    let stopped = stopped_time_classes(&estimates);

    let selected = select_for_analysis(&SelectInput {
        games: &games,
        plan,
        now,
        first_scheduled_at: scheduled_at.unwrap_or(now),
        p3_selected_last_7d,
        stopped: &stopped,
        in_flight_backfill: &in_flight_backfill,
    });

    let ids: Vec<Uuid> = selected.iter().map(|s| s.game_id).collect();
    let tiers: Vec<i16> = selected.iter().map(|s| i16::from(s.tier)).collect();
    let priorities: Vec<i16> = selected.iter().map(|s| s.priority).collect();
    let promoted: Vec<(Uuid, Option<i16>, Option<i16>)> = sqlx::query_as(PROMOTE_SQL)
        .bind(&ids)
        .bind(&tiers)
        .bind(&priorities)
        .bind(now)
        .bind(account_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(db_err)?;
    let promoted_ids: HashSet<Uuid> = promoted.iter().map(|p| p.0).collect();
    let seen: Vec<Uuid> = waiting
        .into_iter()
        .filter(|id| !promoted_ids.contains(id))
        .collect();
    // With no primary time class (every game future-dated) nothing could be selected: keep the
    // markers, so a later run looks at these games again.
    if !seen.is_empty() && primary_time_class(&games, now).is_some() {
        sqlx::query(SEEN_SQL)
            .bind(&seen)
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
    }
    tx.commit().await.map_err(db_err)?;

    // After the commit.
    let mut summary = ScheduleSummary::default();
    let mut jobs = Vec::with_capacity(promoted.len());
    let stored: HashMap<Uuid, (i16, i16)> = promoted
        .iter()
        .filter_map(|&(id, tier, priority)| Some((id, (tier?, priority?))))
        .collect();
    for s in &selected {
        let Some(&(tier, priority)) = stored.get(&s.game_id) else {
            continue; // it left `imported` since it was read
        };
        summary.selected += 1;
        if let Some(n) = usize::try_from(tier)
            .ok()
            .and_then(|t| summary.by_tier.get_mut(t))
        {
            *n += 1;
        }
        jobs.push((s.game_id, account_id, priority));
    }
    enqueue_analysis_page_with_priority(&deps.queue, &jobs)
        .await
        .map_err(|e| retryable(format!("enqueueing analyses: {e}")))?;
    if first_run {
        // The first run ends only once its previews are queued: the account's first tier-0
        // games are read back from the database, so a retry after a failed enqueue finds the
        // same games (the `preview:<game>` dedupe makes a repeat harmless), and
        // `analysis_scheduled_at` is set last.
        let preview_ids: Vec<Uuid> = sqlx::query_scalar(FIRST_PREVIEWS_SQL)
            .bind(account_id)
            .bind(i64::try_from(PREVIEW_GAMES).unwrap_or(i64::MAX))
            .fetch_all(&deps.pool)
            .await
            .map_err(db_err)?;
        if !preview_ids.is_empty() {
            summary.previews = enqueue_previews(&deps.queue, account_id, &preview_ids)
                .await
                .map_err(|e| retryable(format!("enqueueing previews: {e}")))?;
        }
        sqlx::query(
            "UPDATE linked_accounts SET analysis_scheduled_at = $2 \
             WHERE id = $1 AND analysis_scheduled_at IS NULL",
        )
        .bind(account_id)
        .bind(now)
        .execute(&deps.pool)
        .await
        .map_err(db_err)?;
    }
    tracing::info!(
        account_id = %account_id,
        plan = ?plan,
        candidates = rows.len(),
        selected = summary.selected,
        by_tier = ?summary.by_tier,
        previews = summary.previews,
        stopped = stopped.len(),
        first_run,
        "schedule-analysis"
    );
    Ok(summary)
}

/// Time classes whose rankable estimates are all precise: no more backfill for them.
fn stopped_time_classes(estimates: &[EstimateRow]) -> HashSet<TimeClass> {
    let mut by_tc: HashMap<TimeClass, Vec<PrecisionInput>> = HashMap::new();
    for e in estimates {
        let (Some(tc), Ok(key)) = (
            parse_time_class(&e.time_class),
            e.series_key.parse::<WeaknessKey>(),
        ) else {
            continue; // motif, opening and endgame series have no precision target
        };
        by_tc.entry(tc).or_default().push(PrecisionInput {
            key,
            ci_low: e.ci_low,
            ci_high: e.ci_high,
            band_median: e.band_median,
            n_unweighted: f64::from(e.n_unweighted),
        });
    }
    by_tc
        .into_iter()
        .filter(|(_, est)| is_precise(est))
        .map(|(tc, _)| tc)
        .collect()
}

/// The queue handler for [`KIND_SCHEDULE`].
pub struct ScheduleHandler(pub Arc<ScheduleDeps>);

impl Handler for ScheduleHandler {
    fn kind(&self) -> &'static str {
        KIND_SCHEDULE
    }

    fn handle(&self, job: Job, _ctx: JobContext) -> BoxFuture<'_, Result<Value, JobError>> {
        async move {
            let payload: SchedulePayload =
                serde_json::from_value(job.payload.clone()).map_err(|e| {
                    JobError::Unrecoverable {
                        message: format!("invalid schedule-analysis payload: {e}"),
                        code: None,
                    }
                })?;
            let summary = handle_schedule(&self.0, payload).await?;
            serde_json::to_value(summary).map_err(|e| retryable(e.to_string()))
        }
        .boxed()
    }
}

//! Requeue helpers (R2 Task 31): re-running the hook for games that missed it, and the
//! pending-Maia sweep that recomputes the games it fills.
//!
//! - [`requeue_unenriched`] puts every `done` game whose `enrichment_protocol` is not
//!   `enr-v1` (the hook failed, or the game was analysed before R2) back to `pending` at tier 2
//!   and enqueues it. Its main evals come from the cache, so only the enrichment costs engine
//!   time. It pages by keyset on the primary key (T3 review), like R1's reconciler sweep.
//! - [`run_difficulty_sweep`] runs Task 25's `reenrich_pending_difficulty`, then recomputes the
//!   metrics, opening and series rows of every *marked* game ([`recompute_game_features`]) and
//!   enqueues their profile builds. The mark (`games.profile_stale_at`) is set by a trigger in
//!   the same statement as each difficulty write and cleared in the recompute's transaction, so
//!   a cancel, a DB error or a crash between the two loses no recompute. [`DifficultySweepHandler`] runs it as the recurring
//!   [`KIND_DIFFICULTY_SWEEP`] job ([`ensure_difficulty_sweep_scheduled`]).
//! - [`mark_stale_metrics`] (worker start and `cli recompute-metrics`, ADR-FIX-C-1) marks every
//!   game whose `game_metrics` row has another `PLAYER_GAME_METRICS_VERSION`, so the same
//!   recompute rewrites it from the stored evals: its metrics, openings and series rows and its
//!   `games.white_accuracy` / `black_accuracy`, then its profile build (which rebuilds the KPI
//!   snapshots). [`recompute_marked`] drains every mark at once; with
//!   [`RecomputeDeps::cache_only`] it never runs an engine.

use std::sync::Arc;
use std::time::Duration;

use analysis::eval_cache::{DbEvalCache, EvalCache};
use analysis::evaluate::{EvaluateDeps, evaluate_game};
use analysis::hook::clear_hook_rows;
use analysis::jobs::analyse::analyse_enqueue_options_with_priority;
use chess_core::Color;
use chess_core::game_accuracy::game_accuracy;
use chess_core::move_record::build_move_records;
use chess_core::pgn::parse_pgn;
use chess_core::score::win_percent;
use chess_core::{ClockInfo, EngineScore, MoveRecord};
use db::games::{AnalysisStatus, get_game};
use engine::{ANALYSIS_PROTOCOL, AnalyseOptions, Analyser, AnalysisResult, EngineError};
use enrich::difficulty::DifficultySource;
use enrich::protocol::ENRICHMENT_PROTOCOL;
use enrich::reenrich::{SweepOptions, SweepReport, reenrich_pending_difficulty};
use futures::FutureExt;
use futures::future::BoxFuture;
use profile_core::types::{Difficulty, MultipvLine, RawMoveFeatures, ThreatProbe};
use queue::queue::{Queue, QueueError};
use queue::runner::{Handler, JobContext};
use queue::{EnqueueOptions, Job, JobError, KIND_ANALYSE};
use scheduler::priority::job_priority;
use scheduler::select::Plan;
use serde_json::{Value, json};
use sqlx::types::Json;
use sqlx::{FromRow, PgPool};
use time::OffsetDateTime;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::analyse_features::{
    FeatureInput, GameFeatures, compute_game_features, game_context, previous_own_rating,
};
use crate::hook_impl::{
    DerivedTarget, derived_target, enqueue_build_profile, load_game_explorer, write_derived_rows,
};

/// Games per requeue page.
pub const REQUEUE_PAGE: i64 = 500;
/// The scheduler tier of a requeued game (backfill).
pub const REQUEUE_TIER: u8 = 2;

/// One requeue page: a *window* of the next `$2` games after the keyset cursor `$1` (NULL for
/// the first), each flagged `due` when it is a `done` game without the current enrichment.
///
/// The window has no other filter, so it is always an ordered range scan of `games_pkey` that
/// stops after `$2` entries (no index serves `done` games by id, and a filtered scan could read
/// the whole table for one page). The cursor advances to the window's last id.
pub const REQUEUE_PAGE_SQL: &str = "\
SELECT id, analysis_status = 'done' AND enrichment_protocol IS DISTINCT FROM 'enr-v1' AS due \
FROM games \
WHERE id > coalesce($1::uuid, '00000000-0000-0000-0000-000000000000'::uuid) \
ORDER BY id LIMIT $2";

/// Fenced: only games that are still `done` and unenriched. `$2`/`$3` are the tier priorities
/// with and without Lichess evals.
const REQUEUE_SQL: &str = "\
UPDATE games SET analysis_status = 'pending', analysis_attempts = 0, analysis_error = NULL, \
  pending_since = now(), analysis_tier = $4, \
  analysis_priority = CASE WHEN lichess_evals IS NULL THEN $3 ELSE $2 END \
WHERE id = ANY($1) AND analysis_status = 'done' \
  AND enrichment_protocol IS DISTINCT FROM 'enr-v1' \
RETURNING id, account_id, analysis_priority";

#[derive(FromRow)]
struct PageRow {
    id: Uuid,
    due: bool,
}

/// See the module docs. Returns how many games were requeued.
pub async fn requeue_unenriched(pool: &PgPool, queue: &Queue) -> Result<usize, QueueError> {
    debug_assert_eq!(ENRICHMENT_PROTOCOL, "enr-v1");
    let with_evals = job_priority(REQUEUE_TIER, Plan::Beta, true);
    let without = job_priority(REQUEUE_TIER, Plan::Beta, false);
    let mut cursor: Option<Uuid> = None;
    let mut total = 0;
    loop {
        let page: Vec<PageRow> = sqlx::query_as(REQUEUE_PAGE_SQL)
            .bind(cursor)
            .bind(REQUEUE_PAGE)
            .fetch_all(pool)
            .await?;
        let Some(last) = page.last() else {
            break;
        };
        cursor = Some(last.id);
        let due: Vec<Uuid> = page.iter().filter(|r| r.due).map(|r| r.id).collect();
        if !due.is_empty() {
            let requeued: Vec<(Uuid, Uuid, Option<i16>)> = sqlx::query_as(REQUEUE_SQL)
                .bind(&due)
                .bind(with_evals)
                .bind(without)
                .bind(i16::from(REQUEUE_TIER))
                .fetch_all(pool)
                .await?;
            for (game_id, account_id, priority) in requeued {
                let priority = priority.unwrap_or(without);
                // Deduped onto a live job when one exists; the reconciler re-enqueues a pending
                // game whose job is gone.
                queue
                    .enqueue(
                        KIND_ANALYSE,
                        &json!({ "gameId": game_id }),
                        analyse_enqueue_options_with_priority(game_id, account_id, priority),
                    )
                    .await?;
                total += 1;
            }
        }
        if i64::try_from(page.len()).unwrap_or(i64::MAX) < REQUEUE_PAGE {
            break;
        }
    }
    if total > 0 {
        tracing::info!(requeued = total, "unenriched games requeued for enrichment");
    }
    Ok(total)
}

// ---------------------------------------------------------------------------------------------
// The pending-Maia sweep

/// The recurring job kind of the pending-Maia sweep.
pub const KIND_DIFFICULTY_SWEEP: &str = "difficulty-sweep";
/// Its dedupe key (one live sweep).
pub const DIFFICULTY_SWEEP_KEY: &str = "difficulty-sweep";
/// How often the sweep runs. A sweep with Maia down reads nothing (Task 25), so a short period
/// costs one `/healthz` probe.
pub const DIFFICULTY_SWEEP_EVERY: Duration = Duration::from_secs(600);

pub struct DifficultySweepDeps {
    pub pool: PgPool,
    pub queue: Queue,
    /// For the recompute: the main evals come from `cache` (every position of a done game is
    /// cached); the analyser only fills a missing entry.
    pub analyser: Arc<dyn Analyser>,
    pub cache: Arc<dyn EvalCache>,
    pub source: Arc<dyn DifficultySource>,
    pub options: SweepOptions,
}

impl DifficultySweepDeps {
    fn recompute_deps(&self) -> RecomputeDeps {
        RecomputeDeps {
            pool: self.pool.clone(),
            queue: self.queue.clone(),
            analyser: self.analyser.clone(),
            cache: self.cache.clone(),
            cache_only: false,
        }
    }
}

/// What [`recompute_game_features`] needs.
#[derive(Clone)]
pub struct RecomputeDeps {
    pub pool: PgPool,
    pub queue: Queue,
    /// The main evals come from `cache` (every position of a done game is cached); the analyser
    /// only fills a missing entry.
    pub analyser: Arc<dyn Analyser>,
    pub cache: Arc<dyn EvalCache>,
    /// The analyser is [`NoEngine`]: a game that cannot be evaluated from the cache keeps its
    /// rows and its mark for the worker's sweep instead of being abandoned.
    pub cache_only: bool,
}

impl RecomputeDeps {
    /// A recompute from the eval cache alone (`cli recompute-metrics`): no engine is started.
    pub fn cache_only(pool: PgPool, queue: Queue) -> Self {
        RecomputeDeps {
            cache: Arc::new(DbEvalCache::new(pool.clone())),
            analyser: Arc::new(NoEngine),
            pool,
            queue,
            cache_only: true,
        }
    }
}

/// An analyser that refuses every request: a recompute with it reads the eval cache only.
pub struct NoEngine;

impl Analyser for NoEngine {
    fn analyse<'a>(
        &'a self,
        _fen: &'a str,
        _opts: AnalyseOptions,
    ) -> BoxFuture<'a, Result<AnalysisResult, EngineError>> {
        Box::pin(async {
            Err(EngineError::Spawn(
                "cache-only recompute: no engine".to_owned(),
            ))
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DifficultySweepReport {
    pub sweep: SweepReport,
    /// Games whose metrics, opening and series rows were rewritten.
    pub recomputed: Vec<Uuid>,
    /// Games whose recompute failed (logged). An evaluation or decode failure deletes their
    /// derived rows and clears `enrichment_protocol`, so [`requeue_unenriched`] re-runs the hook;
    /// a database error keeps the mark for the next sweep.
    pub failed: Vec<Uuid>,
}

#[derive(thiserror::Error, Debug)]
pub enum RecomputeError {
    #[error("database: {0}")]
    Db(#[from] sqlx::Error),
    #[error("evaluation: {0}")]
    Evaluate(String),
    #[error("cancelled")]
    Cancelled,
    #[error("undecodable game: {0}")]
    Decode(String),
}

/// One sweep plus the recompute of the games it changed.
pub async fn run_difficulty_sweep(
    deps: &DifficultySweepDeps,
    cancel: &CancellationToken,
) -> Result<DifficultySweepReport, sqlx::Error> {
    let sweep = reenrich_pending_difficulty(&deps.pool, deps.source.as_ref(), deps.options).await?;
    let mut report = DifficultySweepReport {
        sweep,
        ..DifficultySweepReport::default()
    };
    // The games to recompute are the marked ones (`games.profile_stale_at`, set by a trigger in
    // the difficulty write's own statement), not `report.sweep.games`: a sweep cut off before
    // this loop, or a crash, leaves its marks for the next run.
    let marked: Vec<(Uuid, OffsetDateTime)> = sqlx::query_as(STALE_GAMES_SQL)
        .bind(RECOMPUTE_BATCH)
        .fetch_all(&deps.pool)
        .await?;
    let recompute_deps = deps.recompute_deps();
    for (game_id, marked_at) in marked {
        if cancel.is_cancelled() {
            break;
        }
        match recompute_game_features(&recompute_deps, game_id, marked_at, cancel).await {
            Ok(true) => report.recomputed.push(game_id),
            Ok(false) => {}
            Err(RecomputeError::Cancelled) => break,
            Err(e) => {
                tracing::warn!(%game_id, error = %e, "recompute after the difficulty sweep failed");
                report.failed.push(game_id);
            }
        }
    }
    Ok(report)
}

/// Marked games per sweep run (the rest wait for the next run).
pub const RECOMPUTE_BATCH: i64 = 500;

/// The account × time classes whose stored profile predates the colour split: they have
/// `Both` estimates but no `Black` one. Every build writes one estimate per binomial key and
/// side (a side without games included), so after one build the pair is never due again.
pub const COLOUR_SPLIT_DUE_SQL: &str = "\
SELECT DISTINCT e.account_id, e.time_class::text \
FROM weakness_estimates e \
WHERE e.side = 'both' AND NOT EXISTS ( \
  SELECT 1 FROM weakness_estimates s \
  WHERE s.account_id = e.account_id AND s.time_class = e.time_class AND s.side = 'black') \
ORDER BY 1, 2";

/// Enqueues a profile build for every account × time class of [`COLOUR_SPLIT_DUE_SQL`] (worker
/// start). The build recomputes the per-side rows from the stored metrics and series points: no
/// engine runs. Deduped onto a live build. Returns how many pairs were due.
pub async fn requeue_colour_split(pool: &PgPool, queue: &Queue) -> Result<usize, QueueError> {
    let due: Vec<(Uuid, String)> = sqlx::query_as(COLOUR_SPLIT_DUE_SQL)
        .fetch_all(pool)
        .await?;
    for (account_id, tc) in &due {
        let Some(tc) = crate::analyse_features::parse_time_class(tc) else {
            continue;
        };
        enqueue_build_profile(queue, *account_id, tc).await?;
    }
    if !due.is_empty() {
        tracing::info!(
            due = due.len(),
            "profiles without colour rows queued for a rebuild"
        );
    }
    Ok(due.len())
}

/// Marks the `done` games whose metrics row has another version than this code writes (an
/// existing mark is kept, so its time still orders the queue).
pub const MARK_STALE_METRICS_SQL: &str = "\
UPDATE games g SET profile_stale_at = now() \
FROM game_metrics gm \
WHERE gm.game_id = g.id AND gm.metrics_version <> $1 \
  AND g.analysis_status = 'done' AND g.profile_stale_at IS NULL";

/// See [`MARK_STALE_METRICS_SQL`]. Returns how many games were marked.
pub async fn mark_stale_metrics(pool: &PgPool) -> Result<u64, sqlx::Error> {
    let version =
        i32::try_from(profile_core::types::PLAYER_GAME_METRICS_VERSION).unwrap_or(i32::MAX);
    let marked = sqlx::query(MARK_STALE_METRICS_SQL)
        .bind(version)
        .execute(pool)
        .await?
        .rows_affected();
    if marked > 0 {
        tracing::info!(
            marked,
            version,
            "games with stale metrics marked for recompute"
        );
    }
    Ok(marked)
}

/// What [`recompute_marked`] did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RecomputeReport {
    pub recomputed: Vec<Uuid>,
    /// Marks cleared without a rewrite (the game is not current; a re-analysis owns it).
    pub skipped: Vec<Uuid>,
    pub failed: Vec<Uuid>,
}

/// Recomputes every marked game, batch by batch, until no mark is left that this run has not
/// tried (a game whose failure keeps its mark is not tried twice).
pub async fn recompute_marked(
    deps: &RecomputeDeps,
    cancel: &CancellationToken,
) -> Result<RecomputeReport, sqlx::Error> {
    let mut report = RecomputeReport::default();
    let mut tried = std::collections::HashSet::new();
    loop {
        let limit = RECOMPUTE_BATCH.saturating_add(i64::try_from(tried.len()).unwrap_or(i64::MAX));
        let marked: Vec<(Uuid, OffsetDateTime)> = sqlx::query_as(STALE_GAMES_SQL)
            .bind(limit)
            .fetch_all(&deps.pool)
            .await?;
        let fresh: Vec<_> = marked
            .into_iter()
            .filter(|(id, _)| !tried.contains(id))
            .collect();
        if fresh.is_empty() {
            return Ok(report);
        }
        for (game_id, marked_at) in fresh {
            if cancel.is_cancelled() {
                return Ok(report);
            }
            tried.insert(game_id);
            match recompute_game_features(deps, game_id, marked_at, cancel).await {
                Ok(true) => report.recomputed.push(game_id),
                Ok(false) => report.skipped.push(game_id),
                Err(RecomputeError::Cancelled) => return Ok(report),
                Err(RecomputeError::Db(e)) => return Err(e),
                Err(e) => {
                    tracing::warn!(%game_id, error = %e, "metrics recompute failed");
                    report.failed.push(game_id);
                }
            }
        }
    }
}

/// The marked games, oldest mark first (served by `games_profile_stale_idx`).
pub const STALE_GAMES_SQL: &str = "\
SELECT id, profile_stale_at FROM games WHERE profile_stale_at IS NOT NULL \
ORDER BY profile_stale_at, id LIMIT $1";

/// Clears the mark unless it was set again after `marked_at`.
const CLEAR_MARK_SQL: &str =
    "UPDATE games SET profile_stale_at = NULL WHERE id = $1 AND profile_stale_at <= $2";

async fn clear_mark(
    conn: &mut sqlx::PgConnection,
    game_id: Uuid,
    marked_at: OffsetDateTime,
) -> Result<(), sqlx::Error> {
    sqlx::query(CLEAR_MARK_SQL)
        .bind(game_id)
        .bind(marked_at)
        .execute(conn)
        .await?;
    Ok(())
}

/// A recompute that failed: when the game is still at the analysed generation, its derived rows
/// are deleted and it is left for [`requeue_unenriched`] (no stale rows are served); otherwise a
/// newer analysis owns it. The mark is cleared either way, so later sweeps do not retry it.
async fn abandon(
    pool: &PgPool,
    game_id: Uuid,
    generation: i32,
    marked_at: OffsetDateTime,
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    let same: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM games WHERE id = $1 AND analysis_status = 'done' \
         AND analysis_generation = $2 AND analysed_generation = $2 FOR UPDATE",
    )
    .bind(game_id)
    .bind(generation)
    .fetch_optional(&mut *tx)
    .await?;
    if same.is_some() {
        clear_hook_rows(&mut tx, game_id).await?;
    } else {
        clear_mark(&mut tx, game_id, marked_at).await?;
    }
    tx.commit().await
}

#[derive(FromRow)]
struct RawRow {
    ply: i32,
    multipv: Option<Json<Vec<MultipvLine>>>,
    best_pv: Vec<String>,
    refutation_pv: Vec<String>,
    threat: Option<Json<ThreatProbe>>,
    tb_wdl_before: Option<i16>,
    tb_wdl_after: Option<i16>,
    p_best: Option<f32>,
    p_ok: Option<f32>,
    difficulty_model: Option<String>,
}

/// The stored raw features of `records` (one entry per record, `None` where no row exists).
async fn load_raw(
    pool: &PgPool,
    game_id: Uuid,
    records: &[MoveRecord],
) -> Result<(Vec<Option<RawMoveFeatures>>, Option<String>), sqlx::Error> {
    let rows: Vec<RawRow> = sqlx::query_as(
        "SELECT ply, multipv, best_pv, refutation_pv, threat, tb_wdl_before, tb_wdl_after, \
         p_best, p_ok, difficulty_model FROM move_features WHERE game_id = $1 ORDER BY ply",
    )
    .bind(game_id)
    .fetch_all(pool)
    .await?;
    let mut raw: Vec<Option<RawMoveFeatures>> = vec![None; records.len()];
    let mut model = None;
    let wdl = |v: Option<i16>| v.and_then(|w| i8::try_from(w).ok());
    for r in rows {
        let Some(i) = records
            .iter()
            .position(|rec| i64::from(rec.ply) == i64::from(r.ply))
        else {
            continue;
        };
        let difficulty = match (r.p_best, r.p_ok, r.difficulty_model) {
            (Some(p_best), Some(p_ok), Some(m)) => {
                model.get_or_insert_with(|| m.clone());
                Some(Difficulty {
                    p_best: f64::from(p_best),
                    p_ok: f64::from(p_ok),
                    model: m,
                })
            }
            _ => None,
        };
        raw[i] = Some(RawMoveFeatures {
            ply: records[i].ply,
            multipv: r.multipv.map(|j| j.0),
            best_pv: r.best_pv,
            refutation_pv: r.refutation_pv,
            threat: r.threat.map(|j| j.0),
            tb_wdl_before: wdl(r.tb_wdl_before),
            tb_wdl_after: wdl(r.tb_wdl_after),
            difficulty,
        });
    }
    Ok((raw, model))
}

/// Supplied Lichess evals, if well formed (as the analyse job reads them).
fn lichess_evals(game: &db::games::GameRow) -> Option<Vec<Option<EngineScore>>> {
    serde_json::from_value(game.lichess_evals.clone()?).ok()
}

/// The recomputed rows of one game.
struct Recomputed {
    target: DerivedTarget,
    /// Both colours' game accuracy (`games.white_accuracy` / `black_accuracy`).
    accuracy: (f64, f64),
    parsed: chess_core::ParsedGame,
    features: GameFeatures,
    model: Option<String>,
}

/// Recomputes one marked game (marked at `marked_at`) from its stored analysis and enrichment
/// and rewrites its `game_metrics`, `game_openings` and series rows (not `move_features`, which
/// keep the sweep's difficulty), clearing the mark in the same transaction.
///
/// - `false`, with only the mark cleared, when the game is not `done` at the current protocols
///   and generation, before the work or under the row lock at write time: whatever makes it
///   current again (a re-analysis, the requeue) runs the whole hook.
/// - A failure to evaluate or decode the game deletes its derived rows and leaves it for
///   [`requeue_unenriched`] ([`abandon`]); a database error keeps the mark for the next sweep;
///   so does a cancel.
pub async fn recompute_game_features(
    deps: &RecomputeDeps,
    game_id: Uuid,
    marked_at: OffsetDateTime,
    cancel: &CancellationToken,
) -> Result<bool, RecomputeError> {
    let pool = &deps.pool;
    let Some(game) = get_game(pool, game_id).await? else {
        return Ok(false);
    };
    let current = game.analysis_status == AnalysisStatus::Done
        && game.analysis_protocol.as_deref() == Some(ANALYSIS_PROTOCOL)
        && game.analysed_generation == Some(game.analysis_generation)
        && game.enrichment_protocol.as_deref() == Some(ENRICHMENT_PROTOCOL);
    if !current {
        clear_mark(&mut *pool.acquire().await?, game_id, marked_at).await?;
        return Ok(false);
    }
    let generation = game.analysis_generation;
    let r = match recompute(deps, &game, cancel).await {
        Ok(r) => r,
        // Without an engine, a missing cache entry is the sweep's to fill: rows and mark stay.
        Err(e @ RecomputeError::Evaluate(_)) if deps.cache_only => return Err(e),
        Err(e @ (RecomputeError::Evaluate(_) | RecomputeError::Decode(_))) => {
            abandon(pool, game_id, generation, marked_at).await?;
            return Err(e);
        }
        Err(e) => return Err(e),
    };

    let mut tx = pool.begin().await?;
    let still: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM games WHERE id = $1 AND analysis_status = 'done'          AND analysis_generation = $2 AND analysed_generation = $2          AND analysis_protocol = $3 AND enrichment_protocol = $4 FOR UPDATE",
    )
    .bind(game_id)
    .bind(generation)
    .bind(ANALYSIS_PROTOCOL)
    .bind(ENRICHMENT_PROTOCOL)
    .fetch_optional(&mut *tx)
    .await?;
    if still.is_some() {
        sqlx::query("UPDATE games SET white_accuracy = $2, black_accuracy = $3 WHERE id = $1")
            .bind(game_id)
            .bind(r.accuracy.0)
            .bind(r.accuracy.1)
            .execute(&mut *tx)
            .await?;
        write_derived_rows(
            &mut tx,
            &r.target,
            &r.features,
            &r.parsed,
            r.model.as_deref(),
        )
        .await?;
    }
    clear_mark(&mut tx, game_id, marked_at).await?;
    tx.commit().await?;
    if still.is_none() {
        return Ok(false);
    }
    if let Err(e) =
        enqueue_build_profile(&deps.queue, r.target.account_id, r.target.time_class).await
    {
        tracing::warn!(%game_id, error = %e, "enqueue build-profile failed");
    }
    Ok(true)
}

async fn recompute(
    deps: &RecomputeDeps,
    game: &db::games::GameRow,
    cancel: &CancellationToken,
) -> Result<Recomputed, RecomputeError> {
    let pool = &deps.pool;
    let target = derived_target(game).map_err(RecomputeError::Decode)?;
    let parsed = parse_pgn(&game.pgn).map_err(|e| RecomputeError::Decode(e.to_string()))?;
    let lichess = lichess_evals(game);
    let eval_deps = EvaluateDeps {
        analyser: deps.analyser.as_ref(),
        cache: deps.cache.as_ref(),
        concurrency: 2,
        on_progress: None,
        cancel: Some(cancel.clone()),
        deadline: None,
    };
    let evals = match evaluate_game(&eval_deps, &parsed, lichess.as_deref()).await {
        Ok(ev) => ev.evals,
        Err(_) if cancel.is_cancelled() => return Err(RecomputeError::Cancelled),
        Err(f) => return Err(RecomputeError::Evaluate(f.error.to_string())),
    };
    let clock = match (game.base_seconds, game.increment_seconds) {
        (Some(base), Some(inc)) => Some(ClockInfo {
            base_seconds: f64::from(base),
            increment_seconds: f64::from(inc),
        }),
        _ => None,
    };
    let records = build_move_records(&parsed, &evals, clock)
        .map_err(|e| RecomputeError::Evaluate(e.to_string()))?;
    let (raw, model) = load_raw(pool, game.id, &records).await?;
    let wins: Vec<f64> = evals.iter().map(|e| win_percent(e.score)).collect();
    let both = (
        game_accuracy(&wins, Color::White),
        game_accuracy(&wins, Color::Black),
    );
    let accuracy = (!records.is_empty()).then(|| game_accuracy(&wins, target.player_color));
    let previous = previous_own_rating(pool, game.id).await?;
    let ctx = game_context(game, accuracy, ANALYSIS_PROTOCOL, previous)
        .map_err(RecomputeError::Decode)?;
    let explorer = load_game_explorer(pool, game.account_id, target.time_class, &parsed).await;
    let features = compute_game_features(FeatureInput {
        ctx: &ctx,
        parsed: &parsed,
        evals: &evals,
        records: &records,
        raw: &raw,
        explorer: explorer.as_ref(),
    });
    Ok(Recomputed {
        target,
        accuracy: both,
        parsed,
        features,
        model,
    })
}

/// Schedules the recurring sweep (idempotent: deduped on [`DIFFICULTY_SWEEP_KEY`]).
pub async fn ensure_difficulty_sweep_scheduled(queue: &Queue) -> Result<(), QueueError> {
    queue
        .enqueue(
            KIND_DIFFICULTY_SWEEP,
            &json!({}),
            EnqueueOptions {
                dedupe_key: Some(DIFFICULTY_SWEEP_KEY.to_owned()),
                priority: 150,
                max_attempts: 1,
                recur_every: Some(DIFFICULTY_SWEEP_EVERY),
                ..EnqueueOptions::default()
            },
        )
        .await?;
    Ok(())
}

/// The queue handler for [`KIND_DIFFICULTY_SWEEP`].
pub struct DifficultySweepHandler(pub Arc<DifficultySweepDeps>);

impl Handler for DifficultySweepHandler {
    fn kind(&self) -> &'static str {
        KIND_DIFFICULTY_SWEEP
    }

    fn handle(&self, _job: Job, ctx: JobContext) -> BoxFuture<'_, Result<Value, JobError>> {
        async move {
            let sweep = run_difficulty_sweep(&self.0, &ctx.cancel);
            tokio::select! {
                res = sweep => match res {
                    Ok(r) => Ok(json!({
                        "maiaHealthy": r.sweep.maia_healthy,
                        "resolved": r.sweep.resolved,
                        "stillPending": r.sweep.still_pending,
                        "parked": r.sweep.parked,
                        "recomputed": r.recomputed.len(),
                        "failed": r.failed.len(),
                    })),
                    Err(e) => Err(JobError::Retryable {
                        message: e.to_string(),
                        code: None,
                    }),
                },
                // Every write is fenced and idempotent; a cut-off sweep resumes next time.
                () = ctx.cancel.cancelled() => Err(JobError::Release {
                    delay: Duration::ZERO,
                    reason: "shutdown".to_owned(),
                }),
            }
        }
        .boxed()
    }
}

//! Reference-game analysis jobs and band reference finalisation (R2 Task 32, TS M2 Task 14,
//! ADR-R2-11).
//!
//! [`handle_reference_game`] runs the full per-game pipeline of one `analyse-reference-game`
//! job: parse, evaluate (`sf19-n300k-v1`, our own engine only: Lichess `[%eval]`s are another
//! protocol), move records, enrichment of **both** colours, then the features and metrics of
//! each side. It writes one `reference_observations` row per rated side and marks the game
//! `done` in one transaction, records a `job_metrics` row, and once no game of the build is
//! `pending` or `analysing` it calls [`finalize_band_reference`].
//!
//! - **Missing rows are no-ops.** Activating a build deletes the superseded build's
//!   `reference_games`, so a queued job may find nothing ([`ReferenceOutcome::Missing`]). A
//!   build that is no longer `building` takes no rows ([`ReferenceOutcome::NotBuilding`]).
//! - **Difficulty is Maia only** (owner override). The Maia model that produced a game's
//!   difficulty values is recorded on the build (`reference_builds.difficulty_model`) in the
//!   game's transaction. A game whose model differs from the build's fails alone (references
//!   are comparable only within one model). Without Maia the column stays NULL and the
//!   difficulty-based detectors treat the plies as unknown (`pending_maia`; plies ≤ 10 are
//!   `not_applicable`), never estimated.
//! - **A bad game never blocks the build.** Poison (an illegal PGN or records) fails the game,
//!   an unsupported or too-short one is skipped, and an engine error is retried, then failed on
//!   the final attempt (and by `on_dead`). Each of these still checks for finalisation.
//!
//! [`finalize_band_reference`] is idempotent and evidence-gated. Under
//! `pg_advisory_xact_lock(hashtext(build_id))` it returns at once for an active build
//! ([`FinalizeOutcome::AlreadyActive`]), any other non-building status, or outstanding games.
//! Otherwise it aggregates the build's observations ([`aggregate_band_reference_report`],
//! stale metrics versions left out). A build without one usable weakness row
//! ([`has_minimum_evidence`]) is failed, so the previous active build keeps serving. Otherwise
//! the rows are written and the build activated (`activate_build_in`) in the same
//! transaction. The advisory lock only avoids duplicate work: correctness comes from the
//! idempotent upserts, `activate_build`'s own kind lock and the `NotBuilding` handling.
//!
//! **Pending Maia.** A game whose enrichment left plies `pending_maia` because Maia was
//! unavailable (not merely unconfigured: then nothing could ever answer) is flagged
//! (`reference_games.difficulty_pending_at`). While a flagged game is younger than
//! [`MAIA_GRACE`], finalisation waits ([`FinalizeOutcome::WaitingForMaia`]).
//! [`reenrich_reference_games`] (the recurring [`KIND_REFERENCE_DIFFICULTY_SWEEP`] job) re-runs
//! flagged games once Maia is healthy, then finalises every finished build, including one whose
//! grace has run out. A build finalised with flagged games keeps those moves out of the
//! difficulty metrics' denominators: a pending ply is never an opportunity and never an
//! estimate (Maia only). The count is reported.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use analysis::eval_cache::EvalCache;
use analysis::evaluate::{DeadlineInfo, EvaluateDeps, EvaluateError, evaluate_game};
use chess_core::game_accuracy::game_accuracy;
use chess_core::move_record::build_move_records;
use chess_core::pgn::parse_pgn;
use chess_core::score::win_percent;
use chess_core::{ClockInfo, Color, ParsedGame, TimeClass};
use db::games::AnalysisStatus;
use db::stage_metrics::{StageMetrics, record_stage_metrics};
use engine::{ANALYSIS_PROTOCOL, Analyser, EngineError};
use enrich::difficulty::{DifficultyOutcome, PendingReason};
use enrich::enrich_game::{EnrichDeps, EnrichError, EnrichInput, enrich_game};
use enrich::protocol::ENRICHMENT_PROTOCOL;
use futures::FutureExt;
use futures::future::BoxFuture;
use profile_core::bands::band_of;
use profile_core::openings::lines::BandSource;
use profile_core::openings::repertoire::{ExplorerIndex, opening_plies};
use profile_core::types::{Band, GameContext, PLAYER_GAME_METRICS_VERSION, PlatformId};
use queue::runner::{Handler, JobContext};
use queue::{Job, JobError, KIND_REFERENCE_GAME, ReferenceGamePayload};
use refdata::band_reference::write_band_reference;
use refdata::builds::{BuildStatus, RefdataError, activate_build_in, fail_build};
use refdata::reference_games::{
    ObservationRow, PENDING_REFERENCE_SQL, pending_reference_count, set_reference_game_status,
    write_observations,
};
use serde::Serialize;
use serde_json::Value;
use sqlx::{FromRow, PgConnection, PgPool};
use time::OffsetDateTime;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::analyse_features::{FeatureInput, compute_game_features, player_score};
use crate::reference_aggregate::{
    ReferenceSide, aggregate_band_reference_report, has_minimum_evidence,
};

/// `job_metrics.kind` of a reference game.
pub const REFERENCE_JOB_KIND: &str = KIND_REFERENCE_GAME;
/// Parallel engine requests per reference game.
pub const REFERENCE_CONCURRENCY: usize = 4;
/// The deadline reported if the job's token fires mid-evaluation.
pub const REFERENCE_DEADLINE: Duration = Duration::from_secs(600);
const MAX_ERROR_CHARS: usize = 500;
/// A build waits this long for Maia to answer its flagged games before it is finalised without
/// their difficulty.
pub const MAIA_GRACE: Duration = Duration::from_secs(24 * 3600);
/// The recurring re-enrichment of reference games with pending Maia moves.
pub const KIND_REFERENCE_DIFFICULTY_SWEEP: &str = "reference-difficulty-sweep";
/// Its dedupe key (one live sweep).
pub const REFERENCE_DIFFICULTY_SWEEP_KEY: &str = "reference-difficulty-sweep";
/// How often it runs. With Maia down a run costs one `/healthz` probe plus the finaliser check.
pub const REFERENCE_DIFFICULTY_SWEEP_EVERY: Duration = Duration::from_secs(600);
/// Games re-enriched per sweep run.
pub const REFERENCE_SWEEP_LIMIT: usize = 200;

/// Options of [`finalize_band_reference_with`].
#[derive(Clone, Debug, PartialEq)]
pub struct FinalizeOptions {
    /// How long flagged pending-Maia games hold finalisation back.
    pub maia_grace: Duration,
}

impl Default for FinalizeOptions {
    fn default() -> Self {
        Self {
            maia_grace: MAIA_GRACE,
        }
    }
}

/// What a reference-game job needs.
pub struct ReferenceGameDeps {
    pub pool: PgPool,
    pub analyser: Arc<dyn Analyser>,
    pub cache: Arc<dyn EvalCache>,
    /// The enrichment's engines, store, tablebase and Maia. Its `cancel` is replaced by each
    /// job's token.
    pub enrich: EnrichDeps,
}

/// Errors of [`finalize_band_reference`].
#[derive(Debug, thiserror::Error)]
pub enum ReferenceError {
    #[error(transparent)]
    Refdata(#[from] RefdataError),
    #[error("database: {0}")]
    Db(#[from] sqlx::Error),
}

/// Result of [`finalize_band_reference`].
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "finalize", rename_all = "snake_case")]
pub enum FinalizeOutcome {
    /// Rows written and the build activated (the previous active build superseded).
    Activated {
        rows: usize,
        observations: usize,
        stale_observations: u32,
        undecodable_observations: u32,
        /// Games finalised with moves still pending Maia (kept out of the difficulty n).
        maia_pending_games: i64,
    },
    /// Another finaliser already activated it.
    AlreadyActive,
    /// Failed, superseded or unverified: nothing to do.
    NotBuilding { status: String },
    /// Games are still `pending` or `analysing`.
    Pending { outstanding: i64 },
    /// Every game is analysed, but `games` flagged ones still wait for Maia (within the grace).
    WaitingForMaia { games: i64 },
    /// No usable weakness row: the build was failed and the previous active build kept.
    InsufficientEvidence { rows: usize, observations: usize },
    /// Fewer usable cells than the active build: failed, the active build kept.
    ThinnerThanActive {
        active_build: Uuid,
        usable_cells: i64,
        active_usable_cells: i64,
    },
    /// The dump is not newer than the active build's: failed, the active build kept.
    OlderThanActive { month: String, active_month: String },
}

/// Result of one reference-game job.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum ReferenceOutcome {
    /// Analysed; `observations` sides written (sides without both ratings are left out).
    Done {
        observations: usize,
        finalize: Option<FinalizeOutcome>,
    },
    /// A duplicate delivery of a game already `done`, `failed` or `skipped`.
    AlreadyFinished { finalize: Option<FinalizeOutcome> },
    /// Unsupported (variant, too short): no observations.
    Skipped {
        reason: String,
        finalize: Option<FinalizeOutcome>,
    },
    /// Poison or out of attempts: marked `failed`.
    Failed {
        reason: String,
        finalize: Option<FinalizeOutcome>,
    },
    /// The row is gone (its build was superseded or failed).
    Missing,
    /// The build no longer takes rows.
    NotBuilding { status: String },
}

#[derive(FromRow)]
struct RefGame {
    id: Uuid,
    build_id: Uuid,
    lichess_game_id: String,
    pgn: String,
    time_class: String,
    base_seconds: Option<i32>,
    increment_seconds: Option<i32>,
    white_rating: Option<i32>,
    black_rating: Option<i32>,
    white_hash: String,
    black_hash: String,
    /// The sampled sides (T21 review I1): only they are reference players.
    white_selected: bool,
    black_selected: bool,
    result: String,
    termination: Option<String>,
    played_at: OffsetDateTime,
    status: String,
    build_status: String,
}

const GAME_SQL: &str = "SELECT g.id, g.build_id, g.lichess_game_id, g.pgn, \
       g.time_class::text AS time_class, g.base_seconds, g.increment_seconds, g.white_rating, \
       g.black_rating, g.white_hash, g.black_hash, g.white_selected, g.black_selected, \
       g.result::text AS result, g.termination, \
       g.played_at, g.status::text AS status, b.status::text AS build_status \
     FROM reference_games g JOIN reference_builds b ON b.id = g.build_id WHERE g.id = $1";

fn parse_tc(s: &str) -> Option<TimeClass> {
    crate::analyse_features::parse_time_class(s)
}

fn truncate(s: &str) -> String {
    s.chars().take(MAX_ERROR_CHARS).collect()
}

fn retryable(message: String) -> JobError {
    JobError::Retryable {
        message,
        code: None,
    }
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

/// Why the pipeline stopped.
enum Stop {
    Skip(String),
    Poison(String),
    Retry(String),
    Cancelled,
}

/// One game's prepared rows.
struct Prepared {
    rows: Vec<ObservationRow>,
    difficulty_model: Option<String>,
    /// Some plies wait for an unavailable Maia (flag the game for the re-enrichment sweep).
    maia_pending: bool,
    stages: BTreeMap<String, f64>,
}

/// Sets a game's status on a pooled connection. `Ok(false)` when the row or its build is gone
/// or no longer building.
async fn set_status(
    pool: &PgPool,
    id: Uuid,
    status: AnalysisStatus,
    error: Option<&str>,
) -> Result<bool, JobError> {
    let mut conn = pool
        .acquire()
        .await
        .map_err(|e| retryable(format!("database error: {e}")))?;
    match set_reference_game_status(&mut conn, id, status, error).await {
        Ok(()) => Ok(true),
        Err(RefdataError::NotFound(_) | RefdataError::NotBuilding { .. }) => Ok(false),
        Err(e) => Err(retryable(format!("reference game status: {e}"))),
    }
}

/// Finalises the build when nothing is outstanding.
async fn maybe_finalize(pool: &PgPool, build: Uuid) -> Result<Option<FinalizeOutcome>, JobError> {
    let outstanding = pending_reference_count(pool, build)
        .await
        .map_err(|e| retryable(format!("pending count: {e}")))?;
    if outstanding > 0 {
        return Ok(None);
    }
    finalize_band_reference(pool, build)
        .await
        .map(Some)
        // The game is final, so a retry of this job comes straight back here.
        .map_err(|e| retryable(format!("finalize band reference: {e}")))
}

/// Analyses one reference game (see the module docs).
pub async fn handle_reference_game(
    deps: &ReferenceGameDeps,
    job: &Job,
    payload: ReferenceGamePayload,
    cancel: CancellationToken,
) -> Result<ReferenceOutcome, JobError> {
    let started = Instant::now();
    let id = payload.reference_game_id;
    let game: Option<RefGame> = sqlx::query_as(GAME_SQL)
        .bind(id)
        .fetch_optional(&deps.pool)
        .await
        .map_err(|e| retryable(format!("database error: {e}")))?;
    let Some(game) = game else {
        tracing::info!(reference_game_id = %id, "reference game gone (superseded build): no-op");
        return Ok(ReferenceOutcome::Missing);
    };
    if game.build_status != BuildStatus::Building.as_str() {
        return Ok(ReferenceOutcome::NotBuilding {
            status: game.build_status,
        });
    }
    let build = game.build_id;
    if matches!(game.status.as_str(), "done" | "failed" | "skipped") {
        let finalize = maybe_finalize(&deps.pool, build).await?;
        return Ok(ReferenceOutcome::AlreadyFinished { finalize });
    }
    if !set_status(&deps.pool, id, AnalysisStatus::Analysing, None).await? {
        return not_building(&deps.pool, build).await;
    }

    let token = cancel.child_token();
    let prepared = match run_pipeline(deps, &game, &token).await {
        Ok(p) => p,
        Err(Stop::Cancelled) => {
            set_status(&deps.pool, id, AnalysisStatus::Pending, Some("interrupted")).await?;
            return Err(JobError::Release {
                delay: Duration::ZERO,
                reason: "shutdown".to_owned(),
            });
        }
        Err(Stop::Skip(reason)) => {
            set_status(&deps.pool, id, AnalysisStatus::Skipped, Some(&reason)).await?;
            let finalize = maybe_finalize(&deps.pool, build).await?;
            return Ok(ReferenceOutcome::Skipped { reason, finalize });
        }
        Err(Stop::Poison(reason)) => return fail_game(deps, build, id, reason).await,
        Err(Stop::Retry(reason)) => {
            if job.is_final_attempt() {
                return fail_game(deps, build, id, reason).await;
            }
            set_status(&deps.pool, id, AnalysisStatus::Pending, Some(&reason)).await?;
            return Err(retryable(reason));
        }
    };

    let t = Instant::now();
    match persist(&deps.pool, build, id, &prepared).await {
        Ok(()) => {}
        Err(PersistError::NotBuilding) => return not_building(&deps.pool, build).await,
        Err(PersistError::ModelMismatch(reason)) => {
            return fail_game(deps, build, id, reason).await;
        }
        Err(PersistError::Db(e)) => {
            let reason = format!("persist: {e}");
            set_status(&deps.pool, id, AnalysisStatus::Pending, Some(&reason)).await?;
            return Err(retryable(reason));
        }
    }
    let mut stages = prepared.stages;
    stages.insert("persist_ms".to_owned(), ms(t));
    stages.insert("observations".to_owned(), prepared.rows.len() as f64);
    let m = StageMetrics {
        job_id: Some(job.id),
        kind: REFERENCE_JOB_KIND.to_owned(),
        game_id: Some(id),
        account_id: None,
        total_ms: ms(started),
        stages,
    };
    if let Err(e) = record_stage_metrics(&deps.pool, &m).await {
        tracing::warn!(reference_game_id = %id, error = %e, "recording reference metrics failed");
    }
    let finalize = maybe_finalize(&deps.pool, build).await?;
    Ok(ReferenceOutcome::Done {
        observations: prepared.rows.len(),
        finalize,
    })
}

async fn not_building(pool: &PgPool, build: Uuid) -> Result<ReferenceOutcome, JobError> {
    let status: Option<String> =
        sqlx::query_scalar("SELECT status::text FROM reference_builds WHERE id = $1")
            .bind(build)
            .fetch_optional(pool)
            .await
            .map_err(|e| retryable(format!("database error: {e}")))?;
    Ok(match status {
        Some(status) => ReferenceOutcome::NotBuilding { status },
        None => ReferenceOutcome::Missing,
    })
}

async fn fail_game(
    deps: &ReferenceGameDeps,
    build: Uuid,
    id: Uuid,
    reason: String,
) -> Result<ReferenceOutcome, JobError> {
    let reason = truncate(&reason);
    tracing::warn!(reference_game_id = %id, %reason, "reference game failed");
    if !set_status(&deps.pool, id, AnalysisStatus::Failed, Some(&reason)).await? {
        return not_building(&deps.pool, build).await;
    }
    let finalize = maybe_finalize(&deps.pool, build).await?;
    Ok(ReferenceOutcome::Failed { reason, finalize })
}

enum PersistError {
    NotBuilding,
    ModelMismatch(String),
    Db(String),
}

/// Records the game's Maia model on the build, writes the observations and marks the game done,
/// in one transaction. The build row is updated first, so concurrent games never upgrade a
/// share lock (no deadlock with `write_observations`' `FOR SHARE`).
async fn persist(pool: &PgPool, build: Uuid, id: Uuid, p: &Prepared) -> Result<(), PersistError> {
    let db = |e: sqlx::Error| PersistError::Db(e.to_string());
    let refdata = |e: RefdataError| match e {
        RefdataError::NotBuilding { .. } | RefdataError::NotFound(_) => PersistError::NotBuilding,
        other => PersistError::Db(other.to_string()),
    };
    let mut tx = pool.begin().await.map_err(db)?;
    if let Some(model) = &p.difficulty_model {
        let recorded: Option<Option<String>> = sqlx::query_scalar(
            "UPDATE reference_builds SET difficulty_model = COALESCE(difficulty_model, $2) \
             WHERE id = $1 AND status = 'building' RETURNING difficulty_model",
        )
        .bind(build)
        .bind(model)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?;
        match recorded {
            None => return Err(PersistError::NotBuilding),
            Some(Some(m)) if m != *model => {
                return Err(PersistError::ModelMismatch(format!(
                    "difficulty model {model} differs from the build's {m}"
                )));
            }
            Some(_) => {}
        }
    }
    write_observations(&mut tx, build, &p.rows)
        .await
        .map_err(refdata)?;
    let conn: &mut PgConnection = &mut tx;
    set_reference_game_status(conn, id, AnalysisStatus::Done, None)
        .await
        .map_err(refdata)?;
    // The first pending time is kept, so a re-enrichment that is still pending cannot extend
    // the grace.
    sqlx::query(
        "UPDATE reference_games SET difficulty_pending_at = \
         CASE WHEN $2 THEN COALESCE(difficulty_pending_at, now()) ELSE NULL END WHERE id = $1",
    )
    .bind(id)
    .bind(p.maia_pending)
    .execute(&mut *tx)
    .await
    .map_err(db)?;
    tx.commit().await.map_err(db)
}

async fn load_explorer(
    pool: &PgPool,
    parsed: &ParsedGame,
    band: Band,
    tc: TimeClass,
) -> Option<ExplorerIndex> {
    let keys: Vec<String> = opening_plies(&parsed.plies)
        .into_iter()
        .map(|p| p.fen_key_before)
        .collect();
    if keys.is_empty() {
        return None;
    }
    match refdata::repertoire::load_explorer_with_fallback(
        pool,
        &keys,
        band,
        BandSource::Rating,
        tc,
    )
    .await
    {
        Ok((index, _)) => Some(index),
        Err(e) => {
            tracing::warn!(error = %e, "explorer unavailable; no book exit");
            None
        }
    }
}

async fn run_pipeline(
    deps: &ReferenceGameDeps,
    g: &RefGame,
    token: &CancellationToken,
) -> Result<Prepared, Stop> {
    let mut stages = BTreeMap::new();
    let tc = parse_tc(&g.time_class)
        .ok_or_else(|| Stop::Poison(format!("time class {}", g.time_class)))?;
    let t = Instant::now();
    let parsed = match parse_pgn(&g.pgn) {
        Ok(p) => p,
        Err(e) if e.is_unsupported() => return Err(Stop::Skip(e.to_string())),
        Err(e) => return Err(Stop::Poison(format!("pgn: {e}"))),
    };
    stages.insert("parse_ms".to_owned(), ms(t));
    if parsed.plies.len() < 2 {
        return Err(Stop::Skip("fewer than 2 plies".to_owned()));
    }

    let t = Instant::now();
    let eval_deps = EvaluateDeps {
        analyser: deps.analyser.as_ref(),
        cache: deps.cache.as_ref(),
        concurrency: REFERENCE_CONCURRENCY,
        on_progress: None,
        cancel: Some(token.clone()),
        deadline: Some(DeadlineInfo {
            kind: KIND_REFERENCE_GAME,
            ms: u64::try_from(REFERENCE_DEADLINE.as_millis()).unwrap_or(u64::MAX),
        }),
    };
    // Our own protocol only: the dump's [%eval]s come from another analysis.
    let evals = match evaluate_game(&eval_deps, &parsed, None).await {
        Ok(ev) => ev.evals,
        Err(f) => {
            return Err(match f.error {
                _ if token.is_cancelled() => Stop::Cancelled,
                EvaluateError::Engine(EngineError::PoolClosed) => Stop::Cancelled,
                EvaluateError::NoAnalysis(m) => Stop::Poison(m),
                other => Stop::Retry(format!("evaluate: {other}")),
            });
        }
    };
    stages.insert("evaluate_ms".to_owned(), ms(t));
    let clock = match (g.base_seconds, g.increment_seconds) {
        (Some(base), Some(inc)) => Some(ClockInfo {
            base_seconds: f64::from(base),
            increment_seconds: f64::from(inc),
        }),
        _ => None,
    };
    let records = build_move_records(&parsed, &evals, clock)
        .map_err(|e| Stop::Poison(format!("records: {e}")))?;

    let enrich_deps = EnrichDeps {
        analyser: deps.enrich.analyser.clone(),
        store: deps.enrich.store.clone(),
        tb: deps.enrich.tb.clone(),
        difficulty: deps.enrich.difficulty.clone(),
        cancel: token.clone(),
    };
    let input = EnrichInput {
        parsed: &parsed,
        evals: &evals,
        records: &records,
        colors: &[Color::White, Color::Black],
        elo_white: g.white_rating,
        elo_black: g.black_rating,
    };
    let enriched = enrich_game(&enrich_deps, &input)
        .await
        .map_err(|e| match e {
            EnrichError::Cancelled | EnrichError::Engine(EngineError::PoolClosed) => {
                Stop::Cancelled
            }
            other => Stop::Retry(format!("enrich: {other}")),
        })?;
    if token.is_cancelled() {
        return Err(Stop::Cancelled);
    }
    stages.extend(enriched.stages.clone());
    let pending_difficulty = enriched
        .difficulty
        .iter()
        .flatten()
        .filter(|d| d.status() == "pending_maia")
        .count();
    // Unconfigured Maia can never answer later, so only an unavailable one flags the game.
    let maia_pending = enriched.difficulty.iter().flatten().any(|d| {
        matches!(d, DifficultyOutcome::PendingMaia { reason } if *reason != PendingReason::MaiaNotConfigured)
    });
    stages.insert(
        "difficulty_pending_plies".to_owned(),
        pending_difficulty as f64,
    );

    let t = Instant::now();
    let wins: Vec<f64> = evals.iter().map(|e| win_percent(e.score)).collect();
    let mut rows = Vec::with_capacity(2);
    let mut unrated = 0u32;
    let mut unsampled = 0u32;
    for color in [Color::White, Color::Black] {
        let (own, opp, player_hash, selected) = match color {
            Color::White => (
                g.white_rating,
                g.black_rating,
                &g.white_hash,
                g.white_selected,
            ),
            Color::Black => (
                g.black_rating,
                g.white_rating,
                &g.black_hash,
                g.black_selected,
            ),
        };
        if !selected {
            // The opponent of a sampled player is not a reference player: analysed (both
            // colours are enriched) but never observed.
            unsampled += 1;
            continue;
        }
        let (Some(own), Some(opp)) = (own, opp) else {
            // Without both ratings a side has no band and no gap: no evidence, never a guess.
            unrated += 1;
            continue;
        };
        let band = band_of(f64::from(own));
        let ctx = reference_context(g, tc, color, own, opp, Some(game_accuracy(&wins, color)));
        let explorer = load_explorer(&deps.pool, &parsed, band, tc).await;
        let features = compute_game_features(FeatureInput {
            ctx: &ctx,
            parsed: &parsed,
            evals: &evals,
            records: &records,
            raw: &enriched.raw,
            explorer: explorer.as_ref(),
        });
        for (k, v) in &features.counts {
            *stages
                .entry(format!("{}_{k}", color.as_str()))
                .or_insert(0.0) += v;
        }
        rows.push(ObservationRow {
            reference_game_id: g.id,
            color,
            player_hash: player_hash.clone(),
            band,
            time_class: tc,
            player_rating: own,
            opponent_rating: opp,
            metrics: features.metrics,
        });
    }
    stages.insert("features_ms".to_owned(), ms(t));
    stages.insert("sides_without_ratings".to_owned(), f64::from(unrated));
    stages.insert("sides_not_sampled".to_owned(), f64::from(unsampled));
    Ok(Prepared {
        rows,
        difficulty_model: enriched.difficulty_model,
        maia_pending,
        stages,
    })
}

/// The context of one side of a reference game (game id = the reference game's id).
#[allow(clippy::too_many_arguments)]
fn reference_context(
    g: &RefGame,
    tc: TimeClass,
    color: Color,
    own: i32,
    opp: i32,
    accuracy: Option<f64>,
) -> GameContext {
    let secs = |v: Option<i32>| v.and_then(|s| u32::try_from(s).ok());
    GameContext {
        game_id: g.id,
        platform_game_id: g.lichess_game_id.clone(),
        platform: PlatformId::Lichess,
        played_at: g.played_at,
        time_class: tc,
        base_seconds: secs(g.base_seconds),
        increment_seconds: secs(g.increment_seconds),
        // The Lichess dump's reference games are rated games.
        rated: true,
        player_color: color,
        player_rating: Some(own),
        opponent_rating: Some(opp),
        result: g.result.clone(),
        score: player_score(&g.result, color),
        termination: g.termination.clone(),
        accuracy,
        protocol: ANALYSIS_PROTOCOL.to_owned(),
    }
}

#[derive(FromRow)]
struct ObsRow {
    reference_game_id: Uuid,
    color: String,
    player_hash: String,
    band: String,
    time_class: String,
    player_rating: i32,
    opponent_rating: i32,
    metrics: Value,
    lichess_game_id: String,
    pgn: String,
    base_seconds: Option<i32>,
    increment_seconds: Option<i32>,
    white_rating: Option<i32>,
    black_rating: Option<i32>,
    white_hash: String,
    black_hash: String,
    result: String,
    termination: Option<String>,
    played_at: OffsetDateTime,
}

/// The sides of a build, with how many were stale and how many undecodable.
struct LoadedSides {
    sides: Vec<ReferenceSide>,
    stale: u32,
    undecodable: u32,
}

async fn load_sides(conn: &mut PgConnection, build: Uuid) -> Result<LoadedSides, ReferenceError> {
    let rows: Vec<ObsRow> = sqlx::query_as(
        "SELECT o.reference_game_id, o.color::text AS color, o.player_hash, o.band::text AS band, \
           o.time_class::text AS time_class, o.player_rating, o.opponent_rating, o.metrics, \
           g.lichess_game_id, '' AS pgn, g.base_seconds, g.increment_seconds, g.white_rating, \
           g.black_rating, g.white_hash, g.black_hash, g.result::text AS result, \
           g.termination, g.played_at \
         FROM reference_observations o JOIN reference_games g ON g.id = o.reference_game_id \
         WHERE o.build_id = $1 AND g.status = 'done' \
           AND CASE o.color WHEN 'white' THEN g.white_selected ELSE g.black_selected END \
         ORDER BY o.reference_game_id, o.color",
    )
    .bind(build)
    .fetch_all(&mut *conn)
    .await?;
    let mut out = LoadedSides {
        sides: Vec::with_capacity(rows.len()),
        stale: 0,
        undecodable: 0,
    };
    for r in rows {
        let decoded = (|| {
            let metrics: profile_core::types::PlayerGameMetrics =
                serde_json::from_value(r.metrics.clone()).ok()?;
            let color = crate::analyse_features::parse_color(&r.color)?;
            let band: Band = r.band.parse().ok()?;
            let tc = parse_tc(&r.time_class)?;
            Some((metrics, color, band, tc))
        })();
        let Some((metrics, color, band, tc)) = decoded else {
            out.undecodable += 1;
            continue;
        };
        if metrics.version != PLAYER_GAME_METRICS_VERSION {
            out.stale += 1;
            continue;
        }
        let game = RefGame {
            id: r.reference_game_id,
            build_id: build,
            lichess_game_id: r.lichess_game_id,
            pgn: r.pgn,
            time_class: r.time_class,
            base_seconds: r.base_seconds,
            increment_seconds: r.increment_seconds,
            white_rating: r.white_rating,
            black_rating: r.black_rating,
            white_hash: r.white_hash,
            black_hash: r.black_hash,
            white_selected: true,
            black_selected: true,
            result: r.result,
            termination: r.termination,
            played_at: r.played_at,
            status: String::new(),
            build_status: String::new(),
        };
        let ctx = reference_context(
            &game,
            tc,
            color,
            r.player_rating,
            r.opponent_rating,
            metrics.accuracy,
        );
        out.sides.push(ReferenceSide {
            player_hash: r.player_hash,
            band,
            tc,
            player_rating: r.player_rating,
            opponent_rating: r.opponent_rating,
            metrics,
            ctx,
        });
    }
    Ok(out)
}

/// The one global lock held from the supersede comparison to the activation (transaction
/// scoped). Finalisers take the per-build lock first, then this one, then (inside
/// `activate_build_in`) the kind lock, always in that order.
pub const ACTIVATE_LOCK_SQL: &str =
    "SELECT pg_advisory_xact_lock(hashtext('band-reference-activate'))";

/// The active band-reference build other than `$1` and its usable (not low-sample) `Both` rows.
pub const ACTIVE_USABLE_SQL: &str = "SELECT b.id, (SELECT count(*) FROM band_reference r \
       WHERE r.build_id = b.id AND r.side = 'both' AND NOT r.low_sample) \
     FROM reference_builds b \
     WHERE b.kind = 'band_reference' AND b.status = 'active' AND b.id <> $1";

/// Usable (not low-sample) band x time class x metric cells of the `Both` side.
fn usable_cells(rows: &[refdata::band_reference::BandReferenceRow]) -> usize {
    rows.iter()
        .filter(|r| r.side == profile_core::types::Side::Both && !r.stat.low_sample)
        .count()
}

/// Fails a build that must not be activated, after the finaliser's transaction was rolled back
/// (`fail_build` locks the build row itself). `Some(NotBuilding)` when a concurrent finaliser
/// or operator already moved it on; a build failed twice just updates its error.
async fn fail_finalize(
    pool: &PgPool,
    build_id: Uuid,
    reason: &str,
) -> Result<Option<FinalizeOutcome>, ReferenceError> {
    match fail_build(pool, build_id, reason).await {
        Ok(()) => {
            tracing::warn!(%build_id, %reason, "band reference build failed; previous build kept");
            Ok(None)
        }
        Err(RefdataError::InvalidTransition { from, .. }) => {
            Ok(Some(FinalizeOutcome::NotBuilding {
                status: from.as_str().to_owned(),
            }))
        }
        Err(e) => Err(e.into()),
    }
}

/// Writes the build's band reference rows and activates it (see the module docs). Idempotent:
/// safe to call from every worker that finishes a game, and again after activation.
pub async fn finalize_band_reference(
    pool: &PgPool,
    build_id: Uuid,
) -> Result<FinalizeOutcome, ReferenceError> {
    finalize_band_reference_with(pool, build_id, &FinalizeOptions::default()).await
}

/// [`finalize_band_reference`] with an explicit Maia grace.
pub async fn finalize_band_reference_with(
    pool: &PgPool,
    build_id: Uuid,
    opts: &FinalizeOptions,
) -> Result<FinalizeOutcome, ReferenceError> {
    let mut tx = pool.begin().await?;
    // Only a work-avoider: two finalisers would still end with one active build (see the
    // module docs).
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
        .bind(build_id.to_string())
        .execute(&mut *tx)
        .await?;
    let status: Option<String> = sqlx::query_scalar(
        "SELECT status::text FROM reference_builds WHERE id = $1 AND kind = 'band_reference'",
    )
    .bind(build_id)
    .fetch_optional(&mut *tx)
    .await?;
    let status: BuildStatus = status
        .ok_or(RefdataError::NotFound(build_id))?
        .parse()
        .map_err(|e: refdata::builds::UnknownLabel| RefdataError::Decode(e.to_string()))?;
    match status {
        BuildStatus::Building => {}
        BuildStatus::Active => return Ok(FinalizeOutcome::AlreadyActive),
        other => {
            return Ok(FinalizeOutcome::NotBuilding {
                status: other.as_str().to_owned(),
            });
        }
    }
    let outstanding: i64 = sqlx::query_scalar(PENDING_REFERENCE_SQL)
        .bind(build_id)
        .fetch_one(&mut *tx)
        .await?;
    if outstanding > 0 {
        return Ok(FinalizeOutcome::Pending { outstanding });
    }
    let (waiting, maia_pending_games): (i64, i64) = sqlx::query_as(
        "SELECT count(*) FILTER (WHERE difficulty_pending_at > now() - $2 * interval '1 second'), \
           count(*) FILTER (WHERE difficulty_pending_at IS NOT NULL) \
         FROM reference_games WHERE build_id = $1 AND status = 'done'",
    )
    .bind(build_id)
    .bind(opts.maia_grace.as_secs_f64())
    .fetch_one(&mut *tx)
    .await?;
    if waiting > 0 {
        return Ok(FinalizeOutcome::WaitingForMaia { games: waiting });
    }
    let loaded = load_sides(&mut tx, build_id).await?;
    let observations = loaded.sides.len();
    let report = aggregate_band_reference_report(&loaded.sides);
    tracing::info!(
        %build_id,
        observations,
        stale = loaded.stale,
        undecodable = loaded.undecodable,
        stale_sides = report.stale_sides,
        invalid_values = report.invalid_values,
        failed_rows = report.failed_rows,
        rows = report.rows.len(),
        "band reference aggregated"
    );
    if !has_minimum_evidence(&report.rows) {
        // Release the lock's transaction first: `fail_build` locks the build row itself.
        tx.rollback().await?;
        let reason = format!(
            "insufficient evidence: no weakness row with at least {} players ({} observations)",
            crate::reference_aggregate::LOW_SAMPLE_PLAYERS,
            observations
        );
        if let Some(done) = fail_finalize(pool, build_id, &reason).await? {
            return Ok(done);
        }
        return Ok(FinalizeOutcome::InsufficientEvidence {
            rows: report.rows.len(),
            observations,
        });
    }
    // Supersede rule (review I1): a build replaces the active one only when it serves at least
    // as many usable (band, time class, metric) cells. The first build needs only the evidence
    // gate above.
    //
    // The comparison and the activation run under one global transaction-scoped lock, and the
    // active build is read after it is granted. Two builds finalising at once are therefore
    // compared in turn: the second one compares against the first if the first was activated.
    sqlx::query(ACTIVATE_LOCK_SQL).execute(&mut *tx).await?;
    let usable = i64::try_from(usable_cells(&report.rows)).unwrap_or(i64::MAX);
    let active: Option<(Uuid, i64)> = sqlx::query_as(ACTIVE_USABLE_SQL)
        .bind(build_id)
        .fetch_optional(&mut *tx)
        .await?;
    if let Some((active_build, active_usable)) = active
        && usable < active_usable
    {
        tx.rollback().await?;
        let reason = format!(
            "thinner than active build {active_build}: {usable} vs {active_usable} usable cells"
        );
        if let Some(done) = fail_finalize(pool, build_id, &reason).await? {
            return Ok(done);
        }
        return Ok(FinalizeOutcome::ThinnerThanActive {
            active_build,
            usable_cells: usable,
            active_usable_cells: active_usable,
        });
    }
    // lichess-db creates the build without protocols (it depends on neither engine nor enrich,
    // ADR-R2-11); the games were analysed and enriched under these (Task 35).
    sqlx::query(
        "UPDATE reference_builds SET protocol = coalesce(protocol, $2), \
         enrichment_protocol = coalesce(enrichment_protocol, $3) WHERE id = $1",
    )
    .bind(build_id)
    .bind(ANALYSIS_PROTOCOL)
    .bind(ENRICHMENT_PROTOCOL)
    .execute(&mut *tx)
    .await?;
    match write_band_reference(&mut tx, build_id, &report.rows).await {
        Ok(()) => {}
        Err(RefdataError::NotBuilding { status, .. }) => {
            return Ok(FinalizeOutcome::NotBuilding {
                status: status.as_str().to_owned(),
            });
        }
        Err(e) => return Err(e.into()),
    }
    match activate_build_in(&mut tx, build_id).await {
        Ok(()) => {}
        // An older dump can never be activated: fail it rather than retry it for ever.
        Err(RefdataError::NotNewer {
            month,
            active_month,
            ..
        }) => {
            tx.rollback().await?;
            let reason =
                format!("older than the active build: dump month {month} vs {active_month}");
            if let Some(done) = fail_finalize(pool, build_id, &reason).await? {
                return Ok(done);
            }
            return Ok(FinalizeOutcome::OlderThanActive {
                month,
                active_month,
            });
        }
        Err(e) => return Err(e.into()),
    }
    tx.commit().await?;
    tracing::info!(
        %build_id,
        rows = report.rows.len(),
        usable,
        maia_pending_games,
        "band reference build activated"
    );
    Ok(FinalizeOutcome::Activated {
        rows: report.rows.len(),
        observations,
        stale_observations: loaded.stale + report.stale_sides,
        undecodable_observations: loaded.undecodable,
        maia_pending_games,
    })
}

/// The `analyse-reference-game` runner handler.
pub struct ReferenceGameHandler(pub Arc<ReferenceGameDeps>);

impl Handler for ReferenceGameHandler {
    fn kind(&self) -> &'static str {
        KIND_REFERENCE_GAME
    }

    fn handle(&self, job: Job, ctx: JobContext) -> BoxFuture<'_, Result<Value, JobError>> {
        async move {
            let payload: ReferenceGamePayload = serde_json::from_value(job.payload.clone())
                .map_err(|e| JobError::Unrecoverable {
                    message: format!("invalid analyse-reference-game payload: {e}"),
                    code: Some("invalid_payload".to_owned()),
                })?;
            let outcome = handle_reference_game(&self.0, &job, payload, ctx.cancel).await?;
            Ok(serde_json::to_value(&outcome).unwrap_or(Value::Null))
        }
        .boxed()
    }

    /// A dead-lettered game is marked `failed` (unless already final), so it never holds the
    /// build open, and the build is finalised if it was the last one.
    fn on_dead<'a>(&'a self, job: &'a Job, reason: &'a str) -> BoxFuture<'a, ()> {
        async move {
            let Ok(payload) = serde_json::from_value::<ReferenceGamePayload>(job.payload.clone())
            else {
                return;
            };
            let pool = &self.0.pool;
            let build: Result<Option<Uuid>, sqlx::Error> = sqlx::query_scalar(
                "SELECT build_id FROM reference_games WHERE id = $1 \
                 AND status IN ('pending', 'analysing')",
            )
            .bind(payload.reference_game_id)
            .fetch_optional(pool)
            .await;
            let Ok(Some(build)) = build else {
                return;
            };
            let reason = truncate(reason);
            match set_status(
                pool,
                payload.reference_game_id,
                AnalysisStatus::Failed,
                Some(&reason),
            )
            .await
            {
                Ok(true) => {
                    if let Err(e) = maybe_finalize(pool, build).await {
                        tracing::warn!(%build, error = %e, "finalise after a dead job failed");
                    }
                }
                Ok(false) => {}
                Err(e) => tracing::warn!(error = %e, "marking a dead reference game failed"),
            }
        }
        .boxed()
    }
}

// ---------------------------------------------------------------------------------------------
// Pending-Maia re-enrichment of reference games

/// Result of one [`reenrich_reference_games`] run.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceSweepReport {
    /// A fresh `/healthz` answered; without it no game is touched.
    pub maia_healthy: bool,
    /// Flagged games re-enriched with every difficulty answered (flag cleared).
    pub resolved: usize,
    /// Re-enriched, but some plies still wait for Maia (flag and its first time kept).
    pub still_pending: usize,
    /// Re-enrichment failed (logged); the game keeps its observations and its flag.
    pub failed: usize,
    /// Games whose build stopped taking rows meanwhile.
    pub not_building: usize,
    /// Builds whose finaliser failed (logged; retried by the next run).
    pub finalize_errors: usize,
    /// The finaliser's verdict for every finished `building` band-reference build.
    pub finalized: Vec<(Uuid, FinalizeOutcome)>,
}

const FLAGGED_GAMES_SQL: &str = "SELECT g.id, g.difficulty_pending_at FROM reference_games g \
     JOIN reference_builds b ON b.id = g.build_id \
     WHERE g.difficulty_pending_at IS NOT NULL AND g.status = 'done' AND b.status = 'building' \
       AND ($2::timestamptz IS NULL OR (g.difficulty_pending_at, g.id) > ($2, $3)) \
     ORDER BY g.difficulty_pending_at, g.id LIMIT $1";

const FINISHED_BUILDS_SQL: &str = "SELECT b.id FROM reference_builds b \
     WHERE b.kind = 'band_reference' AND b.status = 'building' AND b.source_sha256 <> '' \
       AND EXISTS (SELECT 1 FROM reference_games g WHERE g.build_id = b.id) \
       AND NOT EXISTS (SELECT 1 FROM reference_games g WHERE g.build_id = b.id \
                       AND g.status IN ('pending', 'analysing')) \
     ORDER BY b.started_at, b.id";

/// Re-runs up to `limit` flagged reference games of `building` builds once Maia is healthy (the
/// evals and the MultiPV and threat probes come back from the caches, so only Maia is asked
/// again), rewriting their observations and flags. Then it finalises every finished `building`
/// band-reference build, which also ends the wait of a build whose [`MAIA_GRACE`] ran out while
/// Maia stayed down.
pub async fn reenrich_reference_games(
    deps: &ReferenceGameDeps,
    limit: usize,
    cancel: &CancellationToken,
) -> Result<ReferenceSweepReport, ReferenceError> {
    let maia_healthy = match &deps.enrich.difficulty {
        Some(source) => source.healthy().await,
        None => false,
    };
    let mut report = ReferenceSweepReport {
        maia_healthy,
        ..ReferenceSweepReport::default()
    };
    if report.maia_healthy {
        // Pages of `limit` by keyset, until none is left (or a cancel): a re-persisted game that
        // is still pending keeps its first time, so it is not visited twice in one run.
        let mut after: Option<(OffsetDateTime, Uuid)> = None;
        'pages: loop {
            let page: Vec<(Uuid, OffsetDateTime)> = sqlx::query_as(FLAGGED_GAMES_SQL)
                .bind(i64::try_from(limit.max(1)).unwrap_or(i64::MAX))
                .bind(after.map(|a| a.0))
                .bind(after.map(|a| a.1))
                .fetch_all(&deps.pool)
                .await?;
            let Some(&(last_id, last_at)) = page.last() else {
                break;
            };
            after = Some((last_at, last_id));
            for (id, _) in page {
                if cancel.is_cancelled() {
                    break 'pages;
                }
                let game: Option<RefGame> = sqlx::query_as(GAME_SQL)
                    .bind(id)
                    .fetch_optional(&deps.pool)
                    .await?;
                let Some(game) = game.filter(|g| {
                    g.status == "done" && g.build_status == BuildStatus::Building.as_str()
                }) else {
                    continue;
                };
                let prepared = match run_pipeline(deps, &game, cancel).await {
                    Ok(p) => p,
                    Err(Stop::Cancelled) => break 'pages,
                    Err(Stop::Skip(r) | Stop::Poison(r) | Stop::Retry(r)) => {
                        tracing::warn!(reference_game_id = %id, reason = %r, "re-enrichment failed");
                        report.failed += 1;
                        continue;
                    }
                };
                match persist(&deps.pool, game.build_id, id, &prepared).await {
                    Ok(()) if prepared.maia_pending => report.still_pending += 1,
                    Ok(()) => report.resolved += 1,
                    Err(PersistError::NotBuilding) => report.not_building += 1,
                    Err(PersistError::ModelMismatch(r) | PersistError::Db(r)) => {
                        tracing::warn!(reference_game_id = %id, reason = %r, "re-enrichment not stored");
                        report.failed += 1;
                    }
                }
            }
        }
    }
    let builds: Vec<Uuid> = sqlx::query_scalar(FINISHED_BUILDS_SQL)
        .fetch_all(&deps.pool)
        .await?;
    for build in builds {
        match finalize_band_reference(&deps.pool, build).await {
            Ok(outcome) => report.finalized.push((build, outcome)),
            // One build's error never drops the others or the report; the next run retries.
            Err(e) => {
                tracing::warn!(%build, error = %e, "finalising a band reference build failed");
                report.finalize_errors += 1;
            }
        }
    }
    Ok(report)
}

/// Schedules the recurring reference re-enrichment (idempotent: deduped on
/// [`REFERENCE_DIFFICULTY_SWEEP_KEY`]). Worker registration is Task 35's.
pub async fn ensure_reference_difficulty_sweep_scheduled(
    queue: &queue::queue::Queue,
) -> Result<(), queue::queue::QueueError> {
    queue
        .enqueue(
            KIND_REFERENCE_DIFFICULTY_SWEEP,
            &serde_json::json!({}),
            queue::EnqueueOptions {
                dedupe_key: Some(REFERENCE_DIFFICULTY_SWEEP_KEY.to_owned()),
                // Below user work, like the reference games themselves.
                priority: queue::REFERENCE_PRIORITY,
                max_attempts: 1,
                recur_every: Some(REFERENCE_DIFFICULTY_SWEEP_EVERY),
                ..queue::EnqueueOptions::default()
            },
        )
        .await?;
    Ok(())
}

/// The queue handler for [`KIND_REFERENCE_DIFFICULTY_SWEEP`].
pub struct ReferenceDifficultySweepHandler(pub Arc<ReferenceGameDeps>);

impl Handler for ReferenceDifficultySweepHandler {
    fn kind(&self) -> &'static str {
        KIND_REFERENCE_DIFFICULTY_SWEEP
    }

    fn handle(&self, _job: Job, ctx: JobContext) -> BoxFuture<'_, Result<Value, JobError>> {
        async move {
            // Every write is idempotent; a cut-off run resumes next time.
            match reenrich_reference_games(&self.0, REFERENCE_SWEEP_LIMIT, &ctx.cancel).await {
                Ok(r) => Ok(serde_json::to_value(&r).unwrap_or(Value::Null)),
                Err(e) => Err(retryable(e.to_string())),
            }
        }
        .boxed()
    }
}

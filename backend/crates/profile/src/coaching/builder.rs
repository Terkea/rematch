//! The CoachingInput builder (R2 Task 34, TS M2 Task 26 `buildCoachingInput`, ADR-R2-18).
//!
//! [`build_coaching_input`] reads one account × time class and assembles the M3 contract:
//! - **evidence** is the games with current-version metrics (`load_analysed_games`, the version
//!   gate) and their series points; stale games are no evidence;
//! - `timeControl` is the most common `(base, increment)` of those games (ties: the smaller
//!   pair), `None` for daily or without clocks; `rating` is the latest own rating; `band` is
//!   R-4 with ruling Q2 (`player_band`);
//! - `basis`: `analysedGames` = evidence games, `importedGames` = every game of the time class,
//!   the window = the evidence games' first and last UTC dates (R-5: `now`'s date without
//!   games), `backfillComplete` = no game of the time class pending or analysing and nothing
//!   left for `select_for_analysis` (the schedule job's mapping, the default beta plan);
//! - `areaScores` and `metrics` come from the latest `last50` snapshots (Task 33), the area
//!   scores recomputed from those rows so `sufficient`/`needMore` are known (R-2 for metrics);
//! - `weaknesses` re-estimate every binomial key at `now` with the benchmark the last
//!   build-profile stored (`account_benchmark`, ADR 0080; the same code as build-profile) and replay the series for the state and `P_imp`; only claimable
//!   estimates (R-3, T12 ruling, `priority > 0`) with at least one exemplar (R3 T8) are ranked,
//!   up to 5 exemplars for ranks 1-3 and 2 after, from `exemplar_plies` within 6 months, by ΔW
//!   then recency, distinct games;
//! - `strengths` ([`super::strengths`]);
//! - `openings`: `opening_lines` with a verdict and at least one example (the fix spot, else
//!   the worst ΔW move of the player in the line's first 30 plies; up to 3 distinct games),
//!   by games descending;
//! - `changes`: onset rows of the stored history (which equals the current replay, T33 ruling)
//!   with their onset in the last 90 days, newest first, with their stored causes and up to 3
//!   examples on each side of the onset;
//! - v2 (ADR 0085): `byColor` ([`super::colors`]), each colour from its own games, snapshot
//!   rows, series points and benchmark rows, and `benchmark`, the stored benchmark band.
//!
//! Every list is cut to `LIMITS` by its own order (R-6), and [`CoachingInput::validate`] runs
//! last: a failure is a bug and returns [`BuildError::Contract`].

use std::collections::{BTreeMap, HashMap, HashSet};

use chess_core::TimeClass;
use chess_core::fen::fen_key;
use coaching_input::{
    AreaScore, Basis, ByColor, ChangeExample, CoachingInput, ExampleSide, InputVersion, Issue,
    LIMITS, Metric, RankedWeakness, Strength, TimeControl, TimelineChange, WeaknessState,
};
use db::games::AnalysisStatus;
use profile_core::constants::{weakness_area, weakness_label};
use profile_core::kpi::area_scores::compute_area_scores;
use profile_core::kpi::catalogue::kpi_catalogue;
use profile_core::kpi::snapshots::{KpiValue, Window, WindowKind};
use profile_core::openings::lines::LineSummary;
use profile_core::openings::repertoire::MAX_REPERTOIRE_PLIES;
use profile_core::stats::priority::{
    EstimateReason, WINDOW_DAYS, WeaknessEstimate, rank_weaknesses,
};
use profile_core::timeline::series::SeriesOutcome;
use profile_core::timeline::states::DETECTOR_VERSION;
use profile_core::types::{
    BandReference, GameContext, MetricId, PlayerGameMetrics, SeriesKey, Side, StrengthKind,
    WeaknessKey,
};
use refdata::builds::RefdataError;
use refdata::repertoire::player_band;
use scheduler::precision::{PrecisionInput, is_precise};
use scheduler::schedule_job::P3_WINDOW;
use scheduler::select::{CandidateGame, Plan, SelectInput, select_for_analysis};
use sqlx::PgPool;
use time::{Date, Duration, OffsetDateTime, UtcOffset};
use uuid::Uuid;

use super::colors::{benchmark_info, color_summary};
use super::exemplars::{
    ExemplarRow, Skipped, by_delta_then_recency, by_recency, load_exemplar_rows, pick_distinct,
};
use super::mapping::{
    candidate_causes, claimable_weakness, contract_line_key, contract_platform, has_time_control,
    metric, opening_verdict, ranked_weakness, series_label, weakness_id_of_series,
};
use super::strengths::{
    AREA_STRENGTH_SCORE, MAX_STRENGTH_EXEMPLARS, StrengthCandidate, StrengthSource,
    WEAKNESS_STRENGTH_CONFIDENCE, area_kinds, select_strengths, weakness_kinds,
};
use crate::analyse_features::parse_time_class;
use crate::estimates::{age_days, compute_estimates, games_in_window};
use crate::load::{LoadError, load_analysed_games, load_series_points};
use crate::peer_benchmark::{load_account_benchmark, load_account_benchmark_side};
use crate::snapshots::LAST_N;
use crate::timeline_job::evaluate_all;

/// Exemplars of ranks 1 to [`TOP_RANKS`]; later ranks get [`LATER_RANK_EXEMPLARS`].
pub const TOP_RANK_EXEMPLARS: usize = 5;
pub const TOP_RANKS: u32 = 3;
pub const LATER_RANK_EXEMPLARS: usize = 2;
/// Changes whose onset is at most this many days old.
pub const CHANGE_DAYS: i64 = 90;
/// Examples per side of a change.
pub const CHANGE_EXAMPLES_PER_SIDE: usize = 3;

#[derive(thiserror::Error, Debug)]
pub enum BuildError {
    #[error("database: {0}")]
    Db(#[from] sqlx::Error),
    /// The built input broke the contract: a builder bug.
    #[error("coaching input breaks the contract: {0:?}")]
    Contract(Vec<Issue>),
}

fn decode(message: String) -> sqlx::Error {
    sqlx::Error::Decode(message.into())
}

impl From<LoadError> for BuildError {
    fn from(e: LoadError) -> Self {
        match e {
            LoadError::Db(e) => BuildError::Db(e),
            LoadError::Decode(m) => BuildError::Db(decode(m)),
        }
    }
}

impl From<RefdataError> for BuildError {
    fn from(e: RefdataError) -> Self {
        match e {
            RefdataError::Db(e) => BuildError::Db(e),
            RefdataError::NotFound(_) => BuildError::Db(sqlx::Error::RowNotFound),
            other => BuildError::Db(decode(other.to_string())),
        }
    }
}

fn utc_date(t: OffsetDateTime) -> Date {
    t.to_offset(UtcOffset::UTC).date()
}

/// The most common `(base, increment)`; ties go to the smaller pair.
fn typical_time_control(
    tc: TimeClass,
    games: &[(GameContext, PlayerGameMetrics)],
) -> Option<TimeControl> {
    if !has_time_control(tc) {
        return None;
    }
    let mut counts: BTreeMap<(u32, u32), u32> = BTreeMap::new();
    for (ctx, _) in games {
        if let Some(base) = ctx.base_seconds {
            *counts
                .entry((base, ctx.increment_seconds.unwrap_or(0)))
                .or_default() += 1;
        }
    }
    let mut best: Option<((u32, u32), u32)> = None;
    for (pair, n) in counts {
        if best.is_none_or(|(_, b)| n > b) {
            best = Some((pair, n));
        }
    }
    best.and_then(|((base, inc), _)| {
        (base <= coaching_input::limits::MAX_BASE_SECONDS
            && inc <= coaching_input::limits::MAX_INCREMENT_SECONDS)
            .then_some(TimeControl {
                base_seconds: base,
                increment_seconds: inc,
            })
    })
}

#[derive(sqlx::FromRow)]
struct ScheduleGame {
    id: Uuid,
    time_class: String,
    played_at: OffsetDateTime,
    rated: bool,
    ply_count: Option<i32>,
    has_lichess_evals: bool,
    analysis_status: String,
    analysis_tier: Option<i16>,
    selected_at: Option<OffsetDateTime>,
}

/// `backfillComplete`: nothing of the time class is pending or analysing, and the scheduler's
/// selection (mapped as the schedule job maps it, default beta plan) picks nothing more in it.
async fn backfill_complete(
    pool: &PgPool,
    account_id: Uuid,
    tc: TimeClass,
    now: OffsetDateTime,
) -> sqlx::Result<bool> {
    let scheduled_at: Option<OffsetDateTime> =
        sqlx::query_scalar("SELECT analysis_scheduled_at FROM linked_accounts WHERE id = $1")
            .bind(account_id)
            .fetch_one(pool)
            .await?;
    let rows: Vec<ScheduleGame> = sqlx::query_as(
        "SELECT id, time_class::text AS time_class, played_at, rated, ply_count, \
           lichess_evals IS NOT NULL AS has_lichess_evals, analysis_status::text AS analysis_status, \
           analysis_tier, selected_at FROM games WHERE account_id = $1",
    )
    .bind(account_id)
    .fetch_all(pool)
    .await?;
    let estimates: Vec<(String, String, f64, f64, Option<f64>, i32)> = sqlx::query_as(
        "SELECT time_class::text, series_key, ci_low, ci_high, band_median, n_unweighted \
         FROM weakness_estimates WHERE account_id = $1 AND side = 'both'",
    )
    .bind(account_id)
    .fetch_all(pool)
    .await?;
    let mut by_tc: HashMap<TimeClass, Vec<PrecisionInput>> = HashMap::new();
    for (t, key, ci_low, ci_high, band_median, n) in estimates {
        if let (Some(t), Ok(key)) = (parse_time_class(&t), key.parse::<WeaknessKey>()) {
            by_tc.entry(t).or_default().push(PrecisionInput {
                key,
                ci_low,
                ci_high,
                band_median,
                n_unweighted: f64::from(n),
            });
        }
    }
    let stopped: HashSet<TimeClass> = by_tc
        .into_iter()
        .filter(|(_, e)| is_precise(e))
        .map(|(t, _)| t)
        .collect();
    let mut games = Vec::with_capacity(rows.len());
    let mut in_flight: HashMap<TimeClass, u32> = HashMap::new();
    let mut p3_recent = 0u32;
    for r in &rows {
        let (Some(time_class), Ok(status)) = (
            parse_time_class(&r.time_class),
            r.analysis_status.parse::<AnalysisStatus>(),
        ) else {
            continue;
        };
        let live = matches!(status, AnalysisStatus::Pending | AnalysisStatus::Analysing);
        if live && time_class == tc {
            return Ok(false);
        }
        if live && matches!(r.analysis_tier, Some(1 | 2)) {
            *in_flight.entry(time_class).or_insert(0) += 1;
        }
        if r.analysis_tier == Some(3) && r.selected_at.is_some_and(|t| t > now - P3_WINDOW) {
            p3_recent += 1;
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
    let selected = select_for_analysis(&SelectInput {
        games: &games,
        plan: Plan::Beta,
        now,
        first_scheduled_at: scheduled_at.unwrap_or(now),
        p3_selected_last_7d: p3_recent,
        stopped: &stopped,
        in_flight_backfill: &in_flight,
    });
    let in_tc: HashSet<Uuid> = games
        .iter()
        .filter(|g| g.time_class == tc)
        .map(|g| g.id)
        .collect();
    Ok(!selected.iter().any(|s| in_tc.contains(&s.game_id)))
}

/// The latest `last50` row per metric (Task 33's snapshots), and the newest `computed_at` among
/// them.
async fn latest_last50(
    pool: &PgPool,
    account_id: Uuid,
    tc: TimeClass,
    side: Side,
) -> sqlx::Result<(Vec<KpiValue>, Option<OffsetDateTime>)> {
    type Raw = (
        String,
        Option<f64>,
        Option<f64>,
        i32,
        Option<f64>,
        Option<f64>,
        OffsetDateTime,
    );
    let key = Window::LastN(LAST_N).key();
    let rows: Vec<Raw> = sqlx::query_as(
        "SELECT DISTINCT ON (metric) metric, value, value_adjusted, n, ci_low, ci_high,            computed_at          FROM kpi_snapshot WHERE account_id = $1 AND time_class = $2::time_class          AND side = $4::side_scope AND window_key = $3 ORDER BY metric, id DESC",
    )
    .bind(account_id)
    .bind(tc.as_str())
    .bind(&key)
    .bind(side.as_str())
    .fetch_all(pool)
    .await?;
    let computed_at = rows.iter().map(|r| r.6).max();
    let values = rows
        .into_iter()
        .filter_map(|(m, value, value_adjusted, n, ci_low, ci_high, _)| {
            Some(KpiValue {
                metric: m.parse::<MetricId>().ok()?,
                window_kind: WindowKind::LastN,
                window_key: key.clone(),
                value,
                value_adjusted,
                value_adjusted_raw: None,
                n: u32::try_from(n).ok()?,
                ci_low,
                ci_high,
            })
        })
        .collect();
    Ok((values, computed_at))
}

/// Where each game's exemplar plies are.
#[derive(Default)]
struct Plies {
    weakness: HashMap<WeaknessKey, Vec<(Uuid, u32)>>,
    strength: Vec<(StrengthKind, Uuid, u32)>,
}

fn collect_plies(games: &[(GameContext, PlayerGameMetrics)]) -> Plies {
    let mut p = Plies::default();
    for (ctx, m) in games {
        for (key, obs) in &m.weakness {
            for &ply in &obs.exemplar_plies {
                p.weakness.entry(*key).or_default().push((ctx.game_id, ply));
            }
        }
        for s in &m.strengths {
            p.strength.push((s.kind, ctx.game_id, s.ply));
        }
    }
    p
}

fn rows_of<'a>(
    rows: &'a HashMap<(Uuid, u32), ExemplarRow>,
    refs: &[(Uuid, u32)],
) -> Vec<&'a ExemplarRow> {
    refs.iter().filter_map(|r| rows.get(r)).collect()
}

fn in_window(now: OffsetDateTime, r: &ExemplarRow) -> bool {
    age_days(now, r.played_at) <= WINDOW_DAYS
}

/// Ranked weaknesses (module docs).
fn weaknesses(
    estimates: &[WeaknessEstimate],
    outcomes: &BTreeMap<SeriesKey, SeriesOutcome>,
    plies: &Plies,
    rows: &HashMap<(Uuid, u32), ExemplarRow>,
    now: OffsetDateTime,
    skipped: &Skipped,
) -> Vec<RankedWeakness> {
    let mut out = Vec::new();
    for e in rank_weaknesses(estimates.to_vec()) {
        if out.len() >= LIMITS.weaknesses {
            break;
        }
        if !claimable_weakness(&e) {
            continue;
        }
        let refs = plies.weakness.get(&e.key).map_or(&[][..], Vec::as_slice);
        let candidates: Vec<&ExemplarRow> = rows_of(rows, refs)
            .into_iter()
            .filter(|r| in_window(now, r))
            .collect();
        let rank = u32::try_from(out.len() + 1).unwrap_or(u32::MAX);
        let n = if rank <= TOP_RANKS {
            TOP_RANK_EXEMPLARS
        } else {
            LATER_RANK_EXEMPLARS
        };
        let exemplars = pick_distinct(candidates, n, by_delta_then_recency, skipped);
        if exemplars.is_empty() {
            continue; // R3 T8: an item without an exemplar cannot be cited.
        }
        let (state, p_imp) = outcomes
            .get(&SeriesKey::Weakness(e.key))
            .map_or((WeaknessState::NotAnIssue, None), |o| (o.state, o.p_imp));
        if let Some(w) = ranked_weakness(&e, state, p_imp, rank, exemplars) {
            out.push(w);
        }
    }
    out
}

fn strength_exemplars(
    (kinds, endgame_only): (&[StrengthKind], bool),
    plies: &Plies,
    rows: &HashMap<(Uuid, u32), ExemplarRow>,
    now: OffsetDateTime,
    skipped: &Skipped,
) -> Vec<coaching_input::ExemplarPosition> {
    let candidates: Vec<&ExemplarRow> = plies
        .strength
        .iter()
        .filter(|(k, _, _)| kinds.contains(k))
        .filter_map(|(_, g, p)| rows.get(&(*g, *p)))
        .filter(|r| in_window(now, r) && (!endgame_only || r.phase == "endgame"))
        .collect();
    pick_distinct(candidates, MAX_STRENGTH_EXEMPLARS, by_recency, skipped)
}

#[allow(clippy::too_many_arguments)]
fn strengths(
    area_scores: &[AreaScore],
    estimates: &[WeaknessEstimate],
    ranked: &[RankedWeakness],
    metrics: &[Metric],
    plies: &Plies,
    rows: &HashMap<(Uuid, u32), ExemplarRow>,
    now: OffsetDateTime,
    skipped: &Skipped,
) -> Vec<Strength> {
    let mut candidates = Vec::new();
    for a in area_scores {
        let Some(score) = a
            .score
            .filter(|s| a.sufficient && *s >= AREA_STRENGTH_SCORE)
        else {
            continue;
        };
        candidates.push(StrengthCandidate {
            area: a.area,
            label: format!("Strong area: {}", a.area.as_str()),
            kind: StrengthSource::Area,
            metric_key: None,
            band_percentile: Some(score),
            n: a.n,
            exemplars: strength_exemplars(area_kinds(a.area), plies, rows, now, skipped),
        });
    }
    let ranked_ids: HashSet<(String, Option<String>)> = ranked
        .iter()
        .map(|w| (w.id.clone(), w.variant.clone()))
        .collect();
    let metric_keys: HashSet<&str> = metrics.iter().map(|m| m.key.as_str()).collect();
    for e in estimates {
        let (id, variant) = e.key.contract_id();
        let better = e.reason == EstimateReason::Ok
            && e.min_evidence_met
            && e.k_unweighted > 0.0
            && e.confidence.is_finite()
            && 1.0 - e.confidence >= WEAKNESS_STRENGTH_CONFIDENCE
            && !ranked_ids.contains(&(id.to_owned(), variant.map(str::to_owned)));
        let Some(percentile) = e.percentile.filter(|p| better && p.is_finite()) else {
            continue;
        };
        let area = weakness_area(e.key);
        let metric_id = MetricId::Weakness(e.key);
        candidates.push(StrengthCandidate {
            area,
            label: format!("Few errors: {}", weakness_label(e.key)),
            kind: StrengthSource::Weakness(e.key),
            metric_key: metric_keys
                .contains(metric_id.to_string().as_str())
                .then_some(metric_id),
            band_percentile: Some(percentile),
            n: e.n_unweighted.round().clamp(0.0, f64::from(u32::MAX)) as u32,
            exemplars: strength_exemplars(weakness_kinds(e.key), plies, rows, now, skipped),
        });
    }
    select_strengths(candidates)
}

#[derive(sqlx::FromRow)]
struct LineGame {
    game_id: Uuid,
    line_key: String,
    line_kind: String,
    color: String,
}

#[derive(sqlx::FromRow)]
struct OpeningMove {
    game_id: Uuid,
    ply: i32,
    uci: String,
    fen_before: String,
    delta_w: f64,
}

/// `opening_lines` with a verdict, parsed (an unreadable summary is skipped and logged).
async fn opening_lines(
    pool: &PgPool,
    account_id: Uuid,
    tc: TimeClass,
) -> sqlx::Result<Vec<LineSummary>> {
    let rows: Vec<(String, serde_json::Value)> = sqlx::query_as(
        "SELECT line_key, summary FROM opening_lines WHERE account_id = $1 \
         AND time_class = $2::time_class AND verdict IS NOT NULL \
         ORDER BY games DESC, line_key, kind",
    )
    .bind(account_id)
    .bind(tc.as_str())
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(|(key, v)| match serde_json::from_value::<LineSummary>(v) {
            Ok(s) => Some(s),
            Err(e) => {
                tracing::warn!(%account_id, key, error = %e, "unreadable opening line summary");
                None
            }
        })
        .collect())
}

/// Up to 3 example moves per line: the fix spot's move in each game, else the player's worst
/// ΔW move among the first 30 plies; ordered by ΔW then recency, one per game.
async fn opening_example_refs(
    pool: &PgPool,
    account_id: Uuid,
    tc: TimeClass,
    lines: &[LineSummary],
) -> sqlx::Result<Vec<Vec<(Uuid, u32)>>> {
    if lines.is_empty() {
        return Ok(Vec::new());
    }
    let line_games: Vec<LineGame> = sqlx::query_as(
        "SELECT game_id, line_key, line_kind::text AS line_kind, color::text AS color \
         FROM game_openings WHERE account_id = $1 AND time_class = $2::time_class \
         AND line_key IS NOT NULL AND line_kind IS NOT NULL",
    )
    .bind(account_id)
    .bind(tc.as_str())
    .fetch_all(pool)
    .await?;
    let mut by_line: HashMap<(String, String, String), Vec<Uuid>> = HashMap::new();
    for g in line_games {
        by_line
            .entry((g.line_key, g.line_kind, g.color))
            .or_default()
            .push(g.game_id);
    }
    let wanted: Vec<(String, String, String)> = lines
        .iter()
        .map(|l| {
            (
                l.line_key.clone(),
                l.kind.as_str().to_owned(),
                l.color.as_str().to_owned(),
            )
        })
        .collect();
    let ids: Vec<Uuid> = wanted
        .iter()
        .filter_map(|k| by_line.get(k))
        .flatten()
        .copied()
        .collect();
    let moves: Vec<OpeningMove> = sqlx::query_as(
        "SELECT m.game_id, m.ply, m.uci, m.fen_before, m.delta_w FROM moves m \
         JOIN games g ON g.id = m.game_id \
         WHERE m.game_id = ANY($1) AND m.color = g.player_color AND m.ply <= $2          ORDER BY m.game_id, m.ply",
    )
    .bind(&ids)
    .bind(i32::try_from(MAX_REPERTOIRE_PLIES).unwrap_or(i32::MAX))
    .fetch_all(pool)
    .await?;
    let played: HashMap<Uuid, OffsetDateTime> = sqlx::query_as::<_, (Uuid, OffsetDateTime)>(
        "SELECT id, played_at FROM games WHERE id = ANY($1)",
    )
    .bind(&ids)
    .fetch_all(pool)
    .await?
    .into_iter()
    .collect();
    let mut by_game: HashMap<Uuid, Vec<&OpeningMove>> = HashMap::new();
    for m in &moves {
        by_game.entry(m.game_id).or_default().push(m);
    }
    Ok(lines
        .iter()
        .zip(&wanted)
        .map(|(line, key)| {
            let mut picks: Vec<(&OpeningMove, OffsetDateTime)> = by_line
                .get(key)
                .into_iter()
                .flatten()
                .filter_map(|g| {
                    let ms = by_game.get(g)?;
                    let pick = match &line.fix_spot {
                        Some(f) => ms
                            .iter()
                            .find(|m| m.uci == f.uci && fen_key(&m.fen_before) == f.fen_key)
                            .copied(),
                        None => ms
                            .iter()
                            .max_by(|a, b| a.delta_w.total_cmp(&b.delta_w).then(b.ply.cmp(&a.ply)))
                            .copied(),
                    }?;
                    Some((pick, *played.get(g)?))
                })
                .collect();
            picks.sort_by(|a, b| {
                b.0.delta_w
                    .total_cmp(&a.0.delta_w)
                    .then(b.1.cmp(&a.1))
                    .then(a.0.game_id.cmp(&b.0.game_id))
            });
            // More than 3 candidates, so an unusable move still leaves examples.
            picks
                .into_iter()
                .filter_map(|(m, _)| Some((m.game_id, u32::try_from(m.ply).ok()?)))
                .take(LIMITS.examples_per_opening * 3)
                .collect()
        })
        .collect())
}

#[derive(sqlx::FromRow)]
struct OnsetRow {
    series_key: String,
    from_state: Option<String>,
    to_state: String,
    onset_at: OffsetDateTime,
    onset_low: OffsetDateTime,
    onset_high: OffsetDateTime,
    p_change: Option<f32>,
    rate_before: Option<f32>,
    rate_after: Option<f32>,
    cause_hints: serde_json::Value,
    platform_game_id: String,
}

/// The exemplar rows of a series (a missed motif: own-tactic exemplars whose best line has the
/// motif; an allowed motif: hung-material exemplars whose refutation has it).
fn series_rows<'a>(
    key: &SeriesKey,
    plies: &Plies,
    rows: &'a HashMap<(Uuid, u32), ExemplarRow>,
) -> Vec<&'a ExemplarRow> {
    let of = |k: WeaknessKey| rows_of(rows, plies.weakness.get(&k).map_or(&[][..], Vec::as_slice));
    match key {
        SeriesKey::Weakness(k) => of(*k),
        SeriesKey::MotifMissed(t) => of(WeaknessKey::W3Shallow)
            .into_iter()
            .chain(of(WeaknessKey::W3Deep))
            .filter(|r| r.best_tags.contains(t))
            .collect(),
        SeriesKey::MotifAllowed(t) => of(WeaknessKey::W1)
            .into_iter()
            .filter(|r| r.refutation_tags.contains(t))
            .collect(),
        SeriesKey::Opening(_) | SeriesKey::Endgame(_) => Vec::new(),
    }
}

fn unit(x: Option<f32>) -> Option<f64> {
    x.map(f64::from)
        .filter(|v| v.is_finite() && (0.0..=1.0).contains(v))
}

#[allow(clippy::too_many_arguments)]
async fn changes(
    pool: &PgPool,
    account_id: Uuid,
    tc: TimeClass,
    now: OffsetDateTime,
    plies: &Plies,
    rows: &HashMap<(Uuid, u32), ExemplarRow>,
    opening_names: &HashMap<String, String>,
    skipped: &Skipped,
) -> sqlx::Result<Vec<TimelineChange>> {
    let onsets: Vec<OnsetRow> = sqlx::query_as(
        "SELECT h.series_key, h.from_state::text AS from_state, h.to_state::text AS to_state, \
           h.onset_at, h.onset_low, h.onset_high, h.p_change, h.rate_before, h.rate_after, \
           h.cause_hints, g.platform_game_id \
         FROM weakness_state_history h JOIN games g ON g.id = h.onset_game_id \
         WHERE h.account_id = $1 AND h.time_class = $2::time_class AND h.kind = 'onset' \
           AND h.detector_version = $3 AND h.onset_at >= $4 AND h.onset_at <= $5 \
           AND h.onset_low IS NOT NULL AND h.onset_high IS NOT NULL \
         ORDER BY h.onset_at DESC, h.id DESC",
    )
    .bind(account_id)
    .bind(tc.as_str())
    .bind(DETECTOR_VERSION)
    .bind(now - Duration::days(CHANGE_DAYS))
    .bind(now)
    .fetch_all(pool)
    .await?;
    let mut out = Vec::new();
    for o in onsets {
        if out.len() >= LIMITS.changes {
            break;
        }
        let Ok(key) = o.series_key.parse::<SeriesKey>() else {
            continue;
        };
        let (Some(rate_before), Some(rate_after), Some(posterior_prob)) =
            (unit(o.rate_before), unit(o.rate_after), unit(o.p_change))
        else {
            continue; // a change without its evidence is not stated
        };
        let from_state = match &o.from_state {
            None => WeaknessState::NotAnIssue, // R-7
            Some(s) => match s.parse() {
                Ok(s) => s,
                Err(_) => continue,
            },
        };
        let Ok(to_state) = o.to_state.parse::<WeaknessState>() else {
            continue;
        };
        let (onset_from, onset_date, onset_to) = (
            utc_date(o.onset_low),
            utc_date(o.onset_at),
            utc_date(o.onset_high),
        );
        let id_len = o.platform_game_id.chars().count();
        // R-7: an opening series is keyed by its (cut) line key; any other key is short.
        let series_key = match &key {
            SeriesKey::Opening(line) => format!("opening:{}", contract_line_key(line)),
            other => other.to_string(),
        };
        if !(onset_from <= onset_date && onset_date <= onset_to)
            || !(1..=LIMITS.id_chars).contains(&id_len)
            || series_key.chars().count() > LIMITS.id_chars
        {
            tracing::warn!(%account_id, key = o.series_key, "history onset row not stateable, left out");
            continue;
        }
        let candidates = series_rows(&key, plies, rows);
        let (before, after): (Vec<&ExemplarRow>, Vec<&ExemplarRow>) = candidates
            .into_iter()
            .partition(|r| r.played_at < o.onset_at);
        let examples: Vec<ChangeExample> =
            [(ExampleSide::Before, before), (ExampleSide::After, after)]
                .into_iter()
                .flat_map(|(side, rs)| {
                    pick_distinct(rs, CHANGE_EXAMPLES_PER_SIDE, by_delta_then_recency, skipped)
                        .into_iter()
                        .map(move |position| ChangeExample { position, side })
                })
                .collect();
        let opening_name = match &key {
            SeriesKey::Opening(line) => opening_names.get(line).map(String::as_str),
            _ => None,
        };
        out.push(TimelineChange {
            series_key,
            weakness_id: weakness_id_of_series(&key),
            label: series_label(&key, opening_name),
            from_state,
            to_state,
            onset_game_id: o.platform_game_id,
            onset_date,
            onset_from,
            onset_to,
            rate_before,
            rate_after,
            posterior_prob,
            candidate_causes: candidate_causes(&o.cause_hints),
            examples,
        });
    }
    Ok(out)
}

/// v2 `byColor` (ADR 0085, [`super::colors`]): each colour from its own games, snapshot rows,
/// series points and benchmark rows.
async fn by_color(
    pool: &PgPool,
    account_id: Uuid,
    tc: TimeClass,
    games: &[(GameContext, PlayerGameMetrics)],
    series: &BTreeMap<SeriesKey, Vec<profile_core::timeline::series::SeriesPoint>>,
    band: profile_core::types::Band,
    now: OffsetDateTime,
) -> Result<ByColor, BuildError> {
    let mut out = Vec::with_capacity(2);
    for side in [Side::White, Side::Black] {
        let side_games: Vec<&(GameContext, PlayerGameMetrics)> = games
            .iter()
            .filter(|(c, _)| side.covers(c.player_color))
            .collect();
        let ids: HashSet<Uuid> = side_games.iter().map(|(c, _)| c.game_id).collect();
        let side_series: BTreeMap<SeriesKey, Vec<profile_core::timeline::series::SeriesPoint>> =
            series
                .iter()
                .map(|(k, pts)| {
                    let kept: Vec<_> = pts
                        .iter()
                        .filter(|p| ids.contains(&p.game_id))
                        .cloned()
                        .collect();
                    (k.clone(), kept)
                })
                .filter(|(_, pts)| !pts.is_empty())
                .collect();
        let reference = load_account_benchmark_side(pool, account_id, tc, side).await?;
        let in_window = games_in_window(side_games.iter().map(|(c, _)| c.played_at), now);
        let estimates = compute_estimates(&side_series, in_window, band, reference.as_ref(), now);
        let snapshots = if side_games.is_empty() {
            Vec::new()
        } else {
            latest_last50(pool, account_id, tc, side).await?.0
        };
        out.push(color_summary(
            side_games.iter().map(|(c, _)| c),
            &snapshots,
            &estimates,
            reference.as_ref(),
        ));
    }
    let black = out.pop().expect("two colours");
    let white = out.pop().expect("two colours");
    Ok(ByColor { white, black })
}

/// v2 `benchmark`: the account's stored benchmark band, when any of its both rows is served
/// (the web's `getBenchmark` rule).
async fn stored_benchmark(
    pool: &PgPool,
    account_id: Uuid,
    tc: TimeClass,
) -> sqlx::Result<Option<coaching_input::Benchmark>> {
    type Row = (String, Option<i32>, Option<i32>, Option<i32>, Option<i32>);
    let row: Option<Row> = sqlx::query_as(
        "SELECT b.source, b.rating_low, b.rating_high, b.n_players, b.n_games \
         FROM account_benchmark b \
         WHERE b.account_id = $1 AND b.time_class = $2::time_class \
           AND EXISTS (SELECT 1 FROM account_benchmark_stat s WHERE s.account_id = b.account_id \
             AND s.time_class = b.time_class AND s.side = 'both' AND NOT s.low_sample)",
    )
    .bind(account_id)
    .bind(tc.as_str())
    .fetch_optional(pool)
    .await?;
    Ok(row.and_then(|(source, lo, hi, players, games)| {
        benchmark_info(&source, lo, hi, players, games)
    }))
}

/// A built input with what the contract does not carry.
#[derive(Clone, Debug, PartialEq)]
pub struct BuildReport {
    /// `generated_at` is the builder's `now`.
    pub input: CoachingInput,
    /// Newest `computed_at` of the `last50` snapshots behind `metrics` and `areaScores`; `None`
    /// when none was used. The strict M3 contract has no field for it (adding one breaks zod
    /// parity), so it travels beside the input for callers that show or log it.
    pub snapshots_computed_at: Option<OffsetDateTime>,
    /// Stored exemplar rows skipped because no valid position could be stated from them.
    pub skipped_exemplars: u32,
}

/// Builds and validates the account × time class `CoachingInput` (module docs).
pub async fn build_coaching_input(
    pool: &PgPool,
    account_id: Uuid,
    tc: TimeClass,
    now: OffsetDateTime,
) -> Result<CoachingInput, BuildError> {
    Ok(build_coaching_input_report(pool, account_id, tc, now)
        .await?
        .input)
}

/// [`build_coaching_input`] with the [`BuildReport`] extras.
pub async fn build_coaching_input_report(
    pool: &PgPool,
    account_id: Uuid,
    tc: TimeClass,
    now: OffsetDateTime,
) -> Result<BuildReport, BuildError> {
    let platform: String =
        sqlx::query_scalar("SELECT platform::text FROM linked_accounts WHERE id = $1")
            .bind(account_id)
            .fetch_optional(pool)
            .await?
            .ok_or(sqlx::Error::RowNotFound)?;
    let platform = platform
        .parse::<profile_core::types::PlatformId>()
        .map_err(|_| decode(format!("platform {platform:?}")))?;

    // Evidence: current-version games and their series points.
    let games = load_analysed_games(pool, account_id, tc).await?;
    let loaded: HashSet<Uuid> = games.iter().map(|(c, _)| c.game_id).collect();
    let mut series = load_series_points(pool, account_id, tc).await?;
    for points in series.values_mut() {
        points.retain(|p| loaded.contains(&p.game_id));
    }
    series.retain(|_, p| !p.is_empty());

    let (band, _source) = player_band(pool, account_id, tc).await?;
    // What the account's last build-profile compared with (ADR 0080), so the input and the
    // stored estimates agree.
    let reference: Option<BandReference> = load_account_benchmark(pool, account_id, tc).await?;
    let rating: Option<i32> = sqlx::query_scalar(
        "SELECT CASE WHEN player_color = 'white' THEN white_rating ELSE black_rating END \
         FROM games WHERE account_id = $1 AND time_class = $2::time_class \
         AND (CASE WHEN player_color = 'white' THEN white_rating ELSE black_rating END) \
         IS NOT NULL ORDER BY played_at DESC, id LIMIT 1",
    )
    .bind(account_id)
    .bind(tc.as_str())
    .fetch_optional(pool)
    .await?
    .flatten();
    let imported: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM games WHERE account_id = $1 AND time_class = $2::time_class",
    )
    .bind(account_id)
    .bind(tc.as_str())
    .fetch_one(pool)
    .await?;

    let first = games.iter().map(|(c, _)| c.played_at).min();
    let last = games.iter().map(|(c, _)| c.played_at).max();
    let basis = Basis {
        analysed_games: u32::try_from(games.len()).unwrap_or(u32::MAX),
        imported_games: u32::try_from(imported).unwrap_or(u32::MAX),
        window_start: first.map_or(utc_date(now), utc_date),
        window_end: last.map_or(utc_date(now), utc_date),
        backfill_complete: backfill_complete(pool, account_id, tc, now).await?,
    };

    // Estimates and states at `now`, as build-profile computes them.
    let in_window = games_in_window(games.iter().map(|(c, _)| c.played_at), now);
    let estimates = compute_estimates(&series, in_window, band, reference.as_ref(), now);
    let outcomes = evaluate_all(&series, reference.as_ref());

    // Metrics and area scores from the latest snapshots (none without evidence games).
    let (kpis, snapshots_computed_at) = if games.is_empty() {
        (Vec::new(), None)
    } else {
        latest_last50(pool, account_id, tc, Side::Both).await?
    };
    let component_rows: Vec<KpiValue> = kpis
        .iter()
        .filter(|k| !matches!(k.metric, MetricId::Area(_)))
        .cloned()
        .collect();
    let area_scores = compute_area_scores(&component_rows, reference.as_ref());
    let metrics: Vec<Metric> = kpi_catalogue()
        .iter()
        .filter_map(|def| {
            let row = component_rows.iter().find(|r| r.metric == def.metric)?;
            metric(def, row, reference.as_ref())
        })
        .take(LIMITS.metrics)
        .collect();

    // Exemplar moves of every game (weakness and strength plies), loaded once.
    let plies = collect_plies(&games);
    let protocols: HashMap<Uuid, String> = games
        .iter()
        .map(|(c, _)| (c.game_id, c.protocol.clone()))
        .collect();
    let mut refs: Vec<(Uuid, u32)> = plies.weakness.values().flatten().copied().collect();
    refs.extend(plies.strength.iter().map(|(_, g, p)| (*g, *p)));
    let lines = opening_lines(pool, account_id, tc).await?;
    let line_refs = opening_example_refs(pool, account_id, tc, &lines).await?;
    refs.extend(line_refs.iter().flatten().copied());
    refs.sort_unstable();
    refs.dedup();
    let skipped = Skipped::default();
    let rows = load_exemplar_rows(pool, &refs, &protocols, &skipped).await?;

    let weaknesses = weaknesses(&estimates, &outcomes, &plies, &rows, now, &skipped);
    let strengths = strengths(
        &area_scores,
        &estimates,
        &weaknesses,
        &metrics,
        &plies,
        &rows,
        now,
        &skipped,
    );
    let mut openings: Vec<coaching_input::OpeningVerdict> = lines
        .iter()
        .zip(&line_refs)
        .filter_map(|(line, refs)| {
            let examples = pick_distinct(
                rows_of(&rows, refs),
                LIMITS.examples_per_opening,
                by_delta_then_recency,
                &skipped,
            );
            opening_verdict(line, examples)
        })
        .collect();
    openings.sort_by(|a, b| b.games.cmp(&a.games).then(a.line_key.cmp(&b.line_key)));
    openings.truncate(LIMITS.openings);
    let opening_names: HashMap<String, String> = lines
        .iter()
        .map(|l| (l.line_key.clone(), l.name.clone()))
        .collect();
    let changes = changes(
        pool,
        account_id,
        tc,
        now,
        &plies,
        &rows,
        &opening_names,
        &skipped,
    )
    .await?;

    let by_color = by_color(pool, account_id, tc, &games, &series, band, now).await?;
    let benchmark = stored_benchmark(pool, account_id, tc).await?;

    let input = CoachingInput {
        input_version: InputVersion::V2,
        account_id,
        platform: contract_platform(platform),
        time_class: tc.into(),
        time_control: typical_time_control(tc, &games),
        band,
        rating: rating
            .and_then(|r| u16::try_from(r).ok())
            .filter(|r| *r <= coaching_input::limits::MAX_RATING),
        generated_at: now,
        basis,
        area_scores,
        metrics,
        weaknesses,
        strengths,
        openings,
        changes,
        by_color: Some(by_color),
        benchmark,
    };
    input.validate().map_err(BuildError::Contract)?;
    if skipped.get() > 0 {
        tracing::info!(%account_id, tc = tc.as_str(), skipped = skipped.get(), "exemplar rows skipped");
    }
    Ok(BuildReport {
        input,
        snapshots_computed_at,
        skipped_exemplars: skipped.get(),
    })
}

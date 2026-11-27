//! `cli lab trajectory` and `cli lab timeline` (lab-v2, #125): two more production rules as JSON
//! steps, so EXP-0015 and EXP-0014 call them instead of writing their own (ADRs 0104 and 0106).
//!
//! 1. [`trajectory`] (rated games to the trajectory verdict, ADR 0089): each player's games go
//!    through `profile::coaching::trajectory::trajectory_input`, the loader's own rule (pre-game
//!    ratings, the latest stored rating as the current one, accuracy and blunders only from
//!    analysed games), then `compute` and `contract`, so the response is the CoachingInput's
//!    `trajectory` exactly as the coach receives it: the peak, the "then" and "now" windows and
//!    each verdict (`thenVsNow.rating.verdict` is the headline "worse than your peak").
//! 2. [`timeline`] (series points to change states, `tl-v1`): each player's series go through
//!    `profile::timeline_job::evaluate_all`, the build's own call, on the benchmark with the
//!    evaluation baselines added (`with_model_baselines`), the series' reference and constants
//!    chosen by `series_setup`; the response holds each series' current state,
//!    `P_imp` and every `weakness_state_history` row the build would write.
//!
//! Both are pure: no engine, no database, no clock. The seed of the trajectory's bootstrap is
//! the request's (production's is `trajectory:<account id>:<time class>`), so the same request
//! gives the same bytes. Game ids of a timeline are UUIDs, because the replay orders games of
//! the same instant by id, as production does.

use std::collections::BTreeMap;

use coaching_input::Trajectory;
use profile::coaching::trajectory::{
    GameResult, Prior, RatingRule, TRAJECTORY_WEAKNESSES, TrajectoryGame, WeaknessInput,
    compute_with, contract, trajectory_input,
};
use profile::estimates::with_model_baselines;
use profile::timeline_job::evaluate_all;
use profile_core::timeline::series::{Evidence, SeriesOutcome, SeriesPoint, TimelineRow};
use profile_core::timeline::states::DETECTOR_VERSION;
use profile_core::types::{
    BandReference, BandReferenceStat, SeriesKey, WeaknessKey, WeaknessState,
};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::lab::{SCHEMA_VERSION, check_version};

// ---- trajectory -----------------------------------------------------------------------------

/// A game's result from the player's side.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LabResult {
    Win,
    Draw,
    Loss,
    Unfinished,
}

impl From<LabResult> for GameResult {
    fn from(r: LabResult) -> Self {
        match r {
            LabResult::Win => GameResult::Win,
            LabResult::Draw => GameResult::Draw,
            LabResult::Loss => GameResult::Loss,
            LabResult::Unfinished => GameResult::Unfinished,
        }
    }
}

/// One rated game, as `load_trajectory` reads it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RatedGame {
    pub id: String,
    #[serde(with = "time::serde::rfc3339")]
    pub played_at: OffsetDateTime,
    /// The player's own stored rating: on chess.com the rating after the game, on Lichess the
    /// one before it (`chesscom` says which). `null`: none stored.
    #[serde(default)]
    pub rating: Option<i32>,
    pub result: LabResult,
    /// The game's analysis is done; only then do `accuracy` and `blunders` count.
    #[serde(default)]
    pub analysed: bool,
    #[serde(default)]
    pub accuracy: Option<f64>,
    /// The player's own moves judged a blunder.
    #[serde(default)]
    pub blunders: u32,
}

/// One game's events and opportunities of a weakness.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GamePoint {
    pub game_id: String,
    pub events: u32,
    pub opportunities: u32,
}

/// The Beta prior of a weakness's estimate (`Prior`, from `prior_of` in the builder).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LabPrior {
    pub mu: f64,
    pub n0: f64,
}

/// A top weakness whose rate is compared then against now.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrajectoryWeakness {
    pub series_key: WeaknessKey,
    pub prior: LabPrior,
    #[serde(default)]
    pub points: Vec<GamePoint>,
}

/// One player's rated games of one time class.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrajectoryPlayer {
    pub id: String,
    /// The bootstrap's seed (production: `trajectory:<account id>:<time class>`).
    pub seed: String,
    /// chess.com stores the rating after the game, so the pre-game rating is the previous
    /// game's (ADR-FIX-C-1).
    #[serde(default)]
    pub chesscom: bool,
    /// In the loader's order, `(played_at, id)` ascending (`ORDER BY played_at, id`): on
    /// chess.com a game's pre-game rating is the previous game's.
    pub games: Vec<RatedGame>,
    /// The top claimable weaknesses, at most `TRAJECTORY_WEAKNESSES` (3), as the builder picks
    /// them (`trajectory_weaknesses`): the estimates with `priority > 0`, by priority then key,
    /// each with `prior_of` its estimate.
    #[serde(default)]
    pub weaknesses: Vec<TrajectoryWeakness>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrajectoryRequest {
    pub schema_version: String,
    /// The rating rule (`RatingRule::name`): `peak-bootstrap`, `memory-halves` or
    /// `memory-before` (EXP-0030). `null` or absent: the production rule.
    #[serde(default)]
    pub rule: Option<String>,
    pub players: Vec<TrajectoryPlayer>,
}

/// One player's trajectory: the CoachingInput's `trajectory` (camelCase).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerTrajectory {
    pub id: String,
    pub trajectory: Trajectory,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TrajectoryResponse {
    pub schema_version: String,
    pub algorithm_id: String,
    pub players: Vec<PlayerTrajectory>,
}

/// The trajectory of one player (`trajectory_input`, `compute`, `contract`).
pub fn trajectory_player(p: &TrajectoryPlayer) -> anyhow::Result<PlayerTrajectory> {
    trajectory_player_with(p, RatingRule::PRODUCTION)
}

/// [`trajectory_player`] under a [`RatingRule`] (the lab's candidates, EXP-0030).
pub fn trajectory_player_with(
    p: &TrajectoryPlayer,
    rule: RatingRule,
) -> anyhow::Result<PlayerTrajectory> {
    if let Some(w) = p
        .games
        .windows(2)
        .find(|w| (w[1].played_at, &w[1].id) < (w[0].played_at, &w[0].id))
    {
        anyhow::bail!(
            "player {}: games must be oldest first, by played_at then id ({} is before {})",
            p.id,
            w[1].id,
            w[0].id
        );
    }
    anyhow::ensure!(
        p.weaknesses.len() <= TRAJECTORY_WEAKNESSES,
        "player {}: {} weaknesses, the trajectory takes at most {TRAJECTORY_WEAKNESSES}",
        p.id,
        p.weaknesses.len()
    );
    let games: Vec<TrajectoryGame> = p
        .games
        .iter()
        .map(|g| {
            Ok(TrajectoryGame {
                id: g.id.clone(),
                played_at: g.played_at,
                own_rating: g.rating,
                result: g.result.into(),
                analysed: g.analysed,
                accuracy: g.accuracy,
                blunders: i32::try_from(g.blunders)
                    .map_err(|_| anyhow::anyhow!("player {}: {} blunders", p.id, g.blunders))?,
            })
        })
        .collect::<anyhow::Result<_>>()?;
    let weaknesses = p
        .weaknesses
        .iter()
        .map(|w| {
            anyhow::ensure!(
                w.series_key.is_binomial(),
                "player {}: {} is not a binomial weakness key",
                p.id,
                w.series_key.as_str()
            );
            Ok(WeaknessInput {
                series_key: w.series_key.as_str().to_owned(),
                prior: Prior {
                    mu: w.prior.mu,
                    n0: w.prior.n0,
                },
                points: w
                    .points
                    .iter()
                    .map(|pt| (pt.game_id.clone(), (pt.events, pt.opportunities)))
                    .collect(),
            })
        })
        .collect::<anyhow::Result<_>>()?;
    let input = trajectory_input(&games, p.chesscom, weaknesses, p.seed.clone());
    let (series, computed) = compute_with(&input, rule);
    Ok(PlayerTrajectory {
        id: p.id.clone(),
        trajectory: contract(&series, &computed),
    })
}

/// `cli lab trajectory`.
pub fn trajectory(
    req: &TrajectoryRequest,
    algorithm_id: &str,
) -> anyhow::Result<TrajectoryResponse> {
    check_version(&req.schema_version)?;
    let rule = match req.rule.as_deref() {
        None => RatingRule::PRODUCTION,
        Some(name) => RatingRule::from_name(name)
            .ok_or_else(|| anyhow::anyhow!("unknown trajectory rule {name:?}"))?,
    };
    Ok(TrajectoryResponse {
        schema_version: SCHEMA_VERSION.to_owned(),
        algorithm_id: algorithm_id.to_owned(),
        players: req
            .players
            .iter()
            .map(|p| trajectory_player_with(p, rule))
            .collect::<anyhow::Result<_>>()?,
    })
}

// ---- timeline -------------------------------------------------------------------------------

/// One game of a series (a `weakness_series_point`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimelinePoint {
    /// The game: games of the same instant replay in id order.
    pub game_id: Uuid,
    #[serde(with = "time::serde::rfc3339")]
    pub played_at: OffsetDateTime,
    pub events: u32,
    pub opportunities: u32,
}

/// One player's series of one time class.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimelinePlayer {
    pub id: String,
    /// The served peer benchmark (`null`: none, so every series is `not_an_issue`).
    #[serde(default)]
    pub reference: Option<BandReference>,
    /// The evaluation baselines (ADR 0101, `model_baselines`; an `analyse` fact): the build
    /// adds them to the benchmark for the keys it lacks (`with_model_baselines`), as here.
    #[serde(default)]
    pub model_reference: BTreeMap<WeaknessKey, BandReferenceStat>,
    /// Points per series key (`W3.shallow`, `motif.missed:fork`, ...), in any order.
    pub series: BTreeMap<SeriesKey, Vec<TimelinePoint>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimelineRequest {
    pub schema_version: String,
    pub players: Vec<TimelinePlayer>,
}

/// A confirmed change's onset game and its uncertainty window.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LabOnset {
    pub game_id: Uuid,
    #[serde(with = "time::serde::rfc3339")]
    pub played_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub low: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub high: OffsetDateTime,
}

/// One `weakness_state_history` row.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LabTimelineRow {
    /// `transition` (the state changed, or the series' first row) or `onset` (a confirmed
    /// change, dated to its onset game).
    pub kind: String,
    pub from: Option<WeaknessState>,
    pub to: WeaknessState,
    pub at_game_id: Uuid,
    #[serde(with = "time::serde::rfc3339")]
    pub at_played_at: OffsetDateTime,
    pub onset: Option<LabOnset>,
    pub p_change: Option<f64>,
    pub rate_before: Option<f64>,
    pub rate_after: Option<f64>,
    pub evidence: Evidence,
}

/// One series' replay.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LabSeriesOutcome {
    pub state: WeaknessState,
    pub p_imp: Option<f64>,
    /// Points skipped because they have more events than opportunities.
    pub invalid_points: u32,
    pub rows: Vec<LabTimelineRow>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerTimeline {
    pub id: String,
    pub series: BTreeMap<SeriesKey, LabSeriesOutcome>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TimelineResponse {
    pub schema_version: String,
    pub algorithm_id: String,
    /// `profile_core::timeline::states::DETECTOR_VERSION`.
    pub detector_version: String,
    pub players: Vec<PlayerTimeline>,
}

fn lab_row(r: &TimelineRow) -> LabTimelineRow {
    LabTimelineRow {
        kind: r.kind.as_str().to_owned(),
        from: r.from,
        to: r.to,
        at_game_id: r.at_game_id,
        at_played_at: r.at_played_at,
        onset: r.onset.as_ref().map(|o| LabOnset {
            game_id: o.game_id,
            played_at: o.played_at,
            low: o.low,
            high: o.high,
        }),
        p_change: r.p_change,
        rate_before: r.rate_before,
        rate_after: r.rate_after,
        evidence: r.evidence,
    }
}

fn lab_outcome(o: &SeriesOutcome) -> LabSeriesOutcome {
    LabSeriesOutcome {
        state: o.state,
        p_imp: o.p_imp,
        invalid_points: o.invalid_points,
        rows: o.rows.iter().map(lab_row).collect(),
    }
}

/// The timeline of one player (`evaluate_all`, the build's own call).
pub fn timeline_player(p: &TimelinePlayer) -> PlayerTimeline {
    let series: BTreeMap<SeriesKey, Vec<SeriesPoint>> = p
        .series
        .iter()
        .map(|(key, points)| {
            let pts = points
                .iter()
                .map(|pt| SeriesPoint {
                    game_id: pt.game_id,
                    played_at: pt.played_at,
                    events: pt.events,
                    opportunities: pt.opportunities,
                    protocol: String::new(),
                })
                .collect();
            (key.clone(), pts)
        })
        .collect();
    PlayerTimeline {
        id: p.id.clone(),
        series: evaluate_all(
            &series,
            with_model_baselines(p.reference.as_ref(), &p.model_reference).as_ref(),
        )
        .iter()
        .map(|(k, o)| (k.clone(), lab_outcome(o)))
        .collect(),
    }
}

/// `cli lab timeline`.
pub fn timeline(req: &TimelineRequest, algorithm_id: &str) -> anyhow::Result<TimelineResponse> {
    check_version(&req.schema_version)?;
    Ok(TimelineResponse {
        schema_version: SCHEMA_VERSION.to_owned(),
        algorithm_id: algorithm_id.to_owned(),
        detector_version: DETECTOR_VERSION.to_owned(),
        players: req.players.iter().map(timeline_player).collect(),
    })
}

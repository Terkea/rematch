//! KPI catalogue (R2 Task 15, TS M2 Task 19, ADR-R2-7).
//!
//! 36 base rows (exactly [`BaseMetric::ALL`], in that order), then one `motif_miss_rate:<tag>`
//! row per player-visible [`MotifTag`] (EXP-0007, [`MotifTag::is_player_visible`]) and one `weakness:<key>` row per binomial [`WeaknessKey`]
//! ([`RANKABLE`]). Each row says how one game contributes: a value (averaged by `Mean`, the most
//! recent taken by `Last`) or event/opportunity counts (pooled by `Rate`). `None` means the game
//! carries no evidence for the row and stays out of its denominator.

use std::sync::OnceLock;

use chess_core::TimeClass;
use coaching_input::MetricUnit;

use crate::constants::{RANKABLE, n0, weakness_area, weakness_label};
use crate::types::{
    Area, BaseMetric, EndgameOutcome, GameContext, MetricId, MotifTag, PlayerGameMetrics,
    SessionInfo, WeaknessKey,
};

/// One game as the KPI layer sees it.
#[derive(Clone, Copy, Debug)]
pub struct KpiGame<'a> {
    pub ctx: &'a GameContext,
    pub metrics: &'a PlayerGameMetrics,
    pub session: Option<&'a SessionInfo>,
}

/// How a row aggregates its per-game contributions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KpiKind {
    /// Mean of per-game values; 80% percentile-bootstrap interval.
    Mean,
    /// Pooled events / opportunities; 80% Wilson interval.
    Rate,
    /// The most recent value; no interval, no opponent adjustment.
    Last,
}

/// One game's contribution to a row.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Extracted {
    Value(f64),
    /// `(events, opportunities)`.
    Counts(f64, f64),
}

/// Extractor of a row: `None` when the game has no evidence for it.
pub type Extract = fn(&KpiGame<'_>) -> Option<Extracted>;

/// A catalogue row.
#[derive(Clone, Copy, Debug)]
pub struct KpiDefinition {
    pub metric: MetricId,
    /// `None` for overall rows (`rating`, `perf_vs_expected`, `accuracy`), which are internal.
    pub area: Option<Area>,
    pub kind: KpiKind,
    /// Unit in the coaching-input contract (R-2); `None` = internal only.
    pub contract_unit: Option<MetricUnit>,
    /// `None` when neither direction is better (time spent per move).
    pub higher_is_better: Option<bool>,
    /// Minimum `n` (games, or opportunities for rates) before the value counts as sufficient.
    pub min_n: u32,
    pub label: &'static str,
    pub extract: Extract,
}

// ------------------------------------------------------------------ helpers

fn value(x: f64) -> Option<Extracted> {
    Some(Extracted::Value(x))
}

fn counts(events: u32, opportunities: u32) -> Option<Extracted> {
    Some(Extracted::Counts(
        f64::from(events),
        f64::from(opportunities),
    ))
}

fn flag(hit: bool) -> Option<Extracted> {
    counts(u32::from(hit), 1)
}

/// The game had a clock: not daily, and the per-game metrics saw clock data (`time_trouble` is
/// `None` exactly for games without clocks). Unclocked games stay out of every time row.
fn clocked(g: &KpiGame<'_>) -> bool {
    g.ctx.time_class != TimeClass::Daily && g.metrics.time_trouble.is_some()
}

/// The player made at least one analysed move, so per-game counts are evidence.
fn analysed(g: &KpiGame<'_>) -> bool {
    g.metrics.moves > 0
}

fn per_game(g: &KpiGame<'_>, count: u32) -> Option<Extracted> {
    analysed(g).then(|| Extracted::Value(f64::from(count)))
}

fn session_index(g: &KpiGame<'_>) -> Option<u32> {
    g.session.map(|s| s.index_in_session)
}

// ------------------------------------------------------------------ base extractors

fn rating(g: &KpiGame<'_>) -> Option<Extracted> {
    g.metrics
        .player_rating
        .or(g.ctx.player_rating)
        .and_then(|r| value(f64::from(r)))
}

fn perf_vs_expected(g: &KpiGame<'_>) -> Option<Extracted> {
    value(g.metrics.score? - g.metrics.expected_score?)
}

fn accuracy(g: &KpiGame<'_>) -> Option<Extracted> {
    value(g.metrics.accuracy?)
}

fn accuracy_opening(g: &KpiGame<'_>) -> Option<Extracted> {
    value(g.metrics.accuracy_by_phase.opening?)
}

fn accuracy_middlegame(g: &KpiGame<'_>) -> Option<Extracted> {
    value(g.metrics.accuracy_by_phase.middlegame?)
}

fn accuracy_endgame(g: &KpiGame<'_>) -> Option<Extracted> {
    value(g.metrics.accuracy_by_phase.endgame?)
}

fn blunders(g: &KpiGame<'_>) -> Option<Extracted> {
    per_game(g, g.metrics.blunders)
}

fn one_move_blunders(g: &KpiGame<'_>) -> Option<Extracted> {
    per_game(g, g.metrics.one_move_blunders)
}

fn blunder_rate_low_clock(g: &KpiGame<'_>) -> Option<Extracted> {
    if !clocked(g) {
        return None;
    }
    counts(g.metrics.low_clock.decisive, g.metrics.low_clock.moves)
}

fn conversion(g: &KpiGame<'_>) -> Option<Extracted> {
    let c = g.metrics.conversion;
    if !c.episode {
        return None;
    }
    flag(c.converted?)
}

fn resilience(g: &KpiGame<'_>) -> Option<Extracted> {
    let r = g.metrics.resilience;
    if !r.episode {
        return None;
    }
    flag(r.saved?)
}

fn missed_punish(g: &KpiGame<'_>) -> Option<Extracted> {
    let m = g.metrics.missed_punish;
    counts(m.misses, m.opportunities)
}

/// `None` (W3 unknown for the game) is no evidence: out of the numerator and of `n`.
fn tactics_missed(g: &KpiGame<'_>) -> Option<Extracted> {
    per_game(g, g.metrics.tactics.missed?)
}

fn tactics_allowed(g: &KpiGame<'_>) -> Option<Extracted> {
    per_game(g, g.metrics.tactics.allowed)
}

fn sharp_errors(g: &KpiGame<'_>) -> Option<Extracted> {
    let s = g.metrics.sharp;
    counts(s.errors, s.positions)
}

fn quiet_loss(g: &KpiGame<'_>) -> Option<Extracted> {
    let q = g.metrics.quiet;
    (q.moves > 0).then(|| Extracted::Value(q.loss / f64::from(q.moves)))
}

fn eval_trend(g: &KpiGame<'_>) -> Option<Extracted> {
    value(g.metrics.eval_trend_15_to_35?)
}

fn time_trouble(g: &KpiGame<'_>) -> Option<Extracted> {
    if !clocked(g) {
        return None;
    }
    flag(g.metrics.time_trouble?)
}

fn lost_on_time(g: &KpiGame<'_>) -> Option<Extracted> {
    clocked(g).then_some(())?;
    flag(g.metrics.lost_on_time)
}

fn time_per_move_opening(g: &KpiGame<'_>) -> Option<Extracted> {
    clocked(g).then_some(())?;
    value(g.metrics.time_per_move.opening?)
}

fn time_per_move_middlegame(g: &KpiGame<'_>) -> Option<Extracted> {
    clocked(g).then_some(())?;
    value(g.metrics.time_per_move.middlegame?)
}

fn time_per_move_endgame(g: &KpiGame<'_>) -> Option<Extracted> {
    clocked(g).then_some(())?;
    value(g.metrics.time_per_move.endgame?)
}

fn critical_time_ratio(g: &KpiGame<'_>) -> Option<Extracted> {
    clocked(g).then_some(())?;
    value(g.metrics.critical_time_ratio?)
}

fn eval_after_opening(g: &KpiGame<'_>) -> Option<Extracted> {
    value(g.metrics.opening.win_at_opening_end?)
}

fn opening_out_worse(g: &KpiGame<'_>) -> Option<Extracted> {
    flag(g.metrics.opening.out_worse?)
}

fn book_exit_ply(g: &KpiGame<'_>) -> Option<Extracted> {
    value(f64::from(g.metrics.opening.book_exit_ply?))
}

fn time_in_opening(g: &KpiGame<'_>) -> Option<Extracted> {
    clocked(g).then_some(())?;
    value(g.metrics.time_in_opening?)
}

fn endgame_conversion(g: &KpiGame<'_>) -> Option<Extracted> {
    match g.metrics.endgame.outcome? {
        EndgameOutcome::Converted => flag(true),
        EndgameOutcome::FailedConversion => flag(false),
        EndgameOutcome::Held | EndgameOutcome::FailedHold => None,
    }
}

fn endgame_hold(g: &KpiGame<'_>) -> Option<Extracted> {
    match g.metrics.endgame.outcome? {
        EndgameOutcome::Held => flag(true),
        EndgameOutcome::FailedHold => flag(false),
        EndgameOutcome::Converted | EndgameOutcome::FailedConversion => None,
    }
}

fn tb_errors(g: &KpiGame<'_>) -> Option<Extracted> {
    let e = g.metrics.endgame;
    (e.tb_positions > 0).then(|| Extracted::Value(f64::from(e.tb_errors)))
}

fn score_after_loss(g: &KpiGame<'_>) -> Option<Extracted> {
    let s = g.session?;
    (s.after_loss && s.index_in_session > 1).then_some(())?;
    value(g.metrics.score?)
}

/// `score_after_non_loss`: after a game in the same session that was not lost (a win or a
/// draw). `SessionInfo` carries only `after_loss` (ADR-R2-7).
fn score_after_non_loss(g: &KpiGame<'_>) -> Option<Extracted> {
    let s = g.session?;
    (!s.after_loss && s.index_in_session > 1).then_some(())?;
    value(g.metrics.score?)
}

/// Contributes the accuracy of first games (index 1) and of games with index >= 4; the
/// aggregation (`snapshots`) takes `mean(index >= 4) - mean(index 1)`.
fn session_decline_input(g: &KpiGame<'_>) -> Option<Extracted> {
    let i = session_index(g)?;
    (i == 1 || i >= SESSION_LATE_INDEX).then_some(())?;
    value(g.metrics.accuracy?)
}

/// First index counted as "late in a session" by `session_accuracy_decline`.
pub const SESSION_LATE_INDEX: u32 = 4;

fn first_game_accuracy(g: &KpiGame<'_>) -> Option<Extracted> {
    (session_index(g)? == 1).then_some(())?;
    value(g.metrics.accuracy?)
}

fn later_games_accuracy(g: &KpiGame<'_>) -> Option<Extracted> {
    (session_index(g)? >= 2).then_some(())?;
    value(g.metrics.accuracy?)
}

fn saveable_resignation(g: &KpiGame<'_>) -> Option<Extracted> {
    flag(g.metrics.resigned_saveable?)
}

// ------------------------------------------------------------------ generated extractors

fn motif_miss<const I: usize>(g: &KpiGame<'_>) -> Option<Extracted> {
    let m = g.metrics.missed_by_motif.get(&MotifTag::ALL[I])?;
    counts(m.misses, m.opportunities)
}

fn weakness_rate<const I: usize>(g: &KpiGame<'_>) -> Option<Extracted> {
    let o = g.metrics.weakness.get(&RANKABLE[I])?;
    counts(o.events, o.opportunities)
}

const MOTIF_EXTRACT: [Extract; 16] = [
    motif_miss::<0>,
    motif_miss::<1>,
    motif_miss::<2>,
    motif_miss::<3>,
    motif_miss::<4>,
    motif_miss::<5>,
    motif_miss::<6>,
    motif_miss::<7>,
    motif_miss::<8>,
    motif_miss::<9>,
    motif_miss::<10>,
    motif_miss::<11>,
    motif_miss::<12>,
    motif_miss::<13>,
    motif_miss::<14>,
    motif_miss::<15>,
];

const WEAKNESS_EXTRACT: [Extract; 17] = [
    weakness_rate::<0>,
    weakness_rate::<1>,
    weakness_rate::<2>,
    weakness_rate::<3>,
    weakness_rate::<4>,
    weakness_rate::<5>,
    weakness_rate::<6>,
    weakness_rate::<7>,
    weakness_rate::<8>,
    weakness_rate::<9>,
    weakness_rate::<10>,
    weakness_rate::<11>,
    weakness_rate::<12>,
    weakness_rate::<13>,
    weakness_rate::<14>,
    weakness_rate::<15>,
    weakness_rate::<16>,
];

const _: () = assert!(MOTIF_EXTRACT.len() == MotifTag::ALL.len());
const _: () = assert!(WEAKNESS_EXTRACT.len() == RANKABLE.len());

/// Total catalogue rows: 36 base + 10 player-visible motif + 17 weakness.
pub const CATALOGUE_LEN: usize =
    BaseMetric::ALL.len() + coaching_input::motifs::PLAYER_VISIBLE_MOTIFS.len() + RANKABLE.len();

/// Count-type `Mean` rows (an integer count per game): a zero-width bootstrap falls back to the
/// exact Poisson (Garwood) interval of the total count.
pub fn is_count_mean(metric: MetricId) -> bool {
    matches!(
        metric,
        MetricId::Base(
            BaseMetric::BlundersPerGame
                | BaseMetric::OneMoveBlundersPerGame
                | BaseMetric::TacticsMissedPerGame
                | BaseMetric::TacticsAllowedPerGame
                | BaseMetric::TbErrorsPerEndgame
        )
    )
}

fn motif_label(t: MotifTag) -> &'static str {
    match t {
        MotifTag::HangingPiece => "Missed free pieces",
        MotifTag::Fork => "Missed forks",
        MotifTag::Pin => "Missed pins",
        MotifTag::Skewer => "Missed skewers",
        MotifTag::DiscoveredAttack => "Missed discovered attacks",
        MotifTag::Deflection => "Missed deflections",
        MotifTag::Sacrifice => "Missed sacrifices",
        MotifTag::Promotion => "Missed promotions",
        MotifTag::Mate => "Missed mates",
        MotifTag::MateIn1 => "Missed mates in one",
        MotifTag::MateIn2 => "Missed mates in two",
        MotifTag::MateIn3 => "Missed mates in three",
        MotifTag::MateIn4Plus => "Missed long mates",
        MotifTag::MatingNet => "Missed mating nets",
        MotifTag::BackRankMate => "Missed back-rank mates",
        MotifTag::SmotheredMate => "Missed smothered mates",
    }
}

// ------------------------------------------------------------------ the table

use KpiKind::{Last, Mean, Rate};

#[allow(clippy::too_many_arguments)]
const fn row(
    m: BaseMetric,
    area: Option<Area>,
    kind: KpiKind,
    contract_unit: Option<MetricUnit>,
    higher_is_better: Option<bool>,
    min_n: u32,
    label: &'static str,
    extract: Extract,
) -> KpiDefinition {
    KpiDefinition {
        metric: MetricId::Base(m),
        area,
        kind,
        contract_unit,
        higher_is_better,
        min_n,
        label,
        extract,
    }
}

const UP: Option<bool> = Some(true);
const DOWN: Option<bool> = Some(false);
const ELO: Option<MetricUnit> = Some(MetricUnit::Elo);
const PCT: Option<MetricUnit> = Some(MetricUnit::Pct);
const PER_GAME: Option<MetricUnit> = Some(MetricUnit::PerGame);
const RATE: Option<MetricUnit> = Some(MetricUnit::Rate);
const SECONDS: Option<MetricUnit> = Some(MetricUnit::Seconds);
const OPENINGS: Option<Area> = Some(Area::Openings);
const MIDDLEGAME: Option<Area> = Some(Area::Middlegame);
const ENDGAME: Option<Area> = Some(Area::Endgame);
const TACTICS: Option<Area> = Some(Area::Tactics);
const CALCULATION: Option<Area> = Some(Area::Calculation);
const TIME: Option<Area> = Some(Area::Time);
const MINDSET: Option<Area> = Some(Area::Mindset);

#[rustfmt::skip]
const BASE_ROWS: [KpiDefinition; 36] = {
    use BaseMetric as B;
    [
        row(B::Rating, None, Last, ELO, UP, 1, "Rating", rating),
        row(B::PerfVsExpected, None, Mean, None, UP, 20, "Score against expectations", perf_vs_expected),
        row(B::Accuracy, None, Mean, PCT, UP, 20, "Accuracy", accuracy),
        row(B::AccuracyOpening, OPENINGS, Mean, PCT, UP, 20, "Opening accuracy", accuracy_opening),
        row(B::AccuracyMiddlegame, MIDDLEGAME, Mean, PCT, UP, 20, "Middlegame accuracy", accuracy_middlegame),
        row(B::AccuracyEndgame, ENDGAME, Mean, PCT, UP, 20, "Endgame accuracy", accuracy_endgame),
        row(B::BlundersPerGame, CALCULATION, Mean, PER_GAME, DOWN, 20, "Blunders per game", blunders),
        row(B::OneMoveBlundersPerGame, CALCULATION, Mean, PER_GAME, DOWN, 20, "One-move blunders per game", one_move_blunders),
        row(B::BlunderRateLowClock, TIME, Rate, RATE, DOWN, 40, "Blunders on a low clock", blunder_rate_low_clock),
        row(B::ConversionRate, MIDDLEGAME, Rate, RATE, UP, 15, "Winning positions converted", conversion),
        row(B::ResilienceRate, MIDDLEGAME, Rate, RATE, UP, 15, "Lost positions saved", resilience),
        row(B::MissedPunishRate, TACTICS, Rate, RATE, DOWN, 30, "Mistakes left unpunished", missed_punish),
        row(B::TacticsMissedPerGame, TACTICS, Mean, PER_GAME, DOWN, 20, "Tactics missed per game", tactics_missed),
        row(B::TacticsAllowedPerGame, TACTICS, Mean, PER_GAME, DOWN, 20, "Tactics allowed per game", tactics_allowed),
        row(B::SharpErrorRate, CALCULATION, Rate, RATE, DOWN, 30, "Errors in sharp positions", sharp_errors),
        row(B::QuietLossPerMove, MIDDLEGAME, Mean, PCT, DOWN, 20, "Win% lost per quiet move", quiet_loss),
        row(B::EvalTrend1535, MIDDLEGAME, Mean, None, UP, 20, "How the middlegame goes", eval_trend),
        row(B::TimeTroubleRate, TIME, Rate, RATE, DOWN, 20, "Games in time trouble", time_trouble),
        row(B::LossesOnTimeRate, TIME, Rate, RATE, DOWN, 20, "Games lost on time", lost_on_time),
        row(B::TimePerMoveOpening, TIME, Mean, SECONDS, None, 20, "Time per opening move", time_per_move_opening),
        row(B::TimePerMoveMiddlegame, TIME, Mean, SECONDS, None, 20, "Time per middlegame move", time_per_move_middlegame),
        row(B::TimePerMoveEndgame, TIME, Mean, SECONDS, None, 20, "Time per endgame move", time_per_move_endgame),
        row(B::CriticalTimeRatio, TIME, Mean, None, UP, 20, "Time on critical moves", critical_time_ratio),
        row(B::EvalAfterOpening, OPENINGS, Mean, PCT, UP, 20, "Win% after the opening", eval_after_opening),
        row(B::OpeningOutWorseRate, OPENINGS, Rate, RATE, DOWN, 20, "Openings left worse", opening_out_worse),
        row(B::BookExitPly, OPENINGS, Mean, None, UP, 20, "How long you stay in known theory", book_exit_ply),
        row(B::TimeInOpening, OPENINGS, Mean, SECONDS, None, 20, "Time spent in the opening", time_in_opening),
        row(B::EndgameConversionRate, ENDGAME, Rate, RATE, UP, 15, "Winning endgames converted", endgame_conversion),
        row(B::EndgameHoldRate, ENDGAME, Rate, RATE, UP, 15, "Worse endgames held", endgame_hold),
        row(B::TbErrorsPerEndgame, ENDGAME, Mean, PER_GAME, DOWN, 20, "Endgame errors per game", tb_errors),
        row(B::ScoreAfterLoss, MINDSET, Mean, RATE, UP, 20, "Score after a loss", score_after_loss),
        row(B::ScoreAfterNonLoss, MINDSET, Mean, RATE, UP, 20, "Score after a win or draw", score_after_non_loss),
        row(B::SessionAccuracyDecline, MINDSET, Mean, None, UP, 20, "Accuracy late in a session", session_decline_input),
        row(B::FirstGameAccuracy, MINDSET, Mean, PCT, UP, 20, "First-game accuracy", first_game_accuracy),
        row(B::LaterGamesAccuracy, MINDSET, Mean, PCT, UP, 20, "Later-games accuracy", later_games_accuracy),
        row(B::SaveableResignationRate, MINDSET, Rate, RATE, DOWN, 10, "Resigning too early", saveable_resignation),
    ]
};

/// Minimum opportunities of a `motif_miss_rate:<tag>` row.
pub const MOTIF_MIN_N: u32 = 20;

fn build() -> Vec<KpiDefinition> {
    let mut rows = BASE_ROWS.to_vec();
    rows.extend(
        MotifTag::ALL
            .iter()
            .zip(MOTIF_EXTRACT)
            .filter(|(t, _)| t.is_player_visible())
            .map(|(&t, extract)| KpiDefinition {
                metric: MetricId::MotifMissRate(t),
                area: TACTICS,
                kind: Rate,
                contract_unit: RATE,
                higher_is_better: DOWN,
                min_n: MOTIF_MIN_N,
                label: motif_label(t),
                extract,
            }),
    );
    rows.extend(
        RANKABLE
            .iter()
            .zip(WEAKNESS_EXTRACT)
            .map(|(&k, extract)| weakness_row(k, extract)),
    );
    rows
}

fn weakness_row(k: WeaknessKey, extract: Extract) -> KpiDefinition {
    KpiDefinition {
        metric: MetricId::Weakness(k),
        area: Some(weakness_area(k)),
        kind: Rate,
        contract_unit: RATE,
        higher_is_better: DOWN,
        // n0 is a whole number of opportunities (games for W9a).
        min_n: n0(k) as u32,
        label: weakness_label(k),
        extract,
    }
}

/// The whole catalogue: base rows in [`BaseMetric::ALL`] order, then the player-visible motif
/// rows in [`MotifTag::ALL`] order, then weakness rows in [`RANKABLE`] order.
pub fn kpi_catalogue() -> &'static [KpiDefinition] {
    static CATALOGUE: OnceLock<Vec<KpiDefinition>> = OnceLock::new();
    CATALOGUE.get_or_init(build)
}

/// The catalogue row of a metric; `None` for `area:*` ids and the continuous W9d and W12.
pub fn kpi_definition(metric: MetricId) -> Option<&'static KpiDefinition> {
    kpi_catalogue().iter().find(|d| d.metric == metric)
}

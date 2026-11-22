//! Per-game metrics (R2 Task 9, rules TS M2 Task 8; stored as `game_metrics.metrics`).
//!
//! [`compute_game_metrics`] reads the game through the detectors' [`GameView`] (moves sorted by
//! ply, duplicated plies dropped, invalid moves skipped), so every metric sees the same moves as
//! the weakness observation. Definitions:
//! - `blunders` / `mistakes` / `inaccuracies`: the player's valid moves by judgement.
//! - `accuracy_by_phase`: lila's game accuracy over each phase's moves (lila
//!   `AccuracyPercent.phaseAccuracies`, ADR-FIX-C-1), on the same scale as the game's accuracy;
//!   `None` without an own move in the phase or with an unusable move in it.
//! - `one_move_blunders`: decisive (ΔW ≥ 15) with a refutation realised in 1 ply, or a missed mate
//!   in 1 (best line `mate_in = 1`, the played move is not a mate).
//! - `missed_punish`: own moves right after a valid opponent mistake or blunder; a miss is ΔW ≥ 10.
//! - `sharp`: eligible own moves with `is_critical = true`; errors have any judgement.
//! - `tactics`: W3 opportunities and events (from the observation). `missed` is `None` when W3
//!   is unknown for the game (T8's whole-game best-line rule, or an unusable observation), never
//!   0. `allowed` counts eligible decisive moves whose refutation is a tactic (gain ≥ 2 or a
//!   mate).
//! - `missed_by_motif`: the observation's `motif.missed:*` rows that have opportunities.
//! - `quiet`: the W10 opportunity set (`detectors::is_quiet_move`); `loss` is the sum of their
//!   ΔW (0-100 scale).
//! - `low_clock` / `high_clock`: eligible own moves with clock < 30 s / > 60 s, in bullet, blitz
//!   and rapid (the W9b population); `decisive` counts ΔW ≥ 15 among them.
//! - `eval_trend_15_to_35`: player W after ply 70 minus after ply 30.
//! - `critical_time_ratio`: mean time on own `is_critical = true` moves over the mean on
//!   `is_critical = false` moves.
//! - `time_in_opening`: total own time in the opening phase.
//! - `opening`: W after the last opening ply, `out_worse` at W ≤ 40 (as W7.prep), and the White-POV
//!   `score_to_cp` there (R-8).
//! - `endgame`: type and player W at the first endgame ply; entry ≥ 70 → converted or
//!   failed_conversion by the result, entry ≤ 30 → held (not lost) or failed_hold, otherwise none.
//! - `resigned_saveable`: the player resigned and the player's W after the last ply was ≥ 25.
//!
//! Honesty rules (ADR-R2-4, progress ledger):
//! - Nothing is claimed from zero evidence: an unknown input gives `None`, never a default.
//! - A missing line is excluded only where it could change the answer: a decisive move without the
//!   line that decides a count is not counted and is reported (`unknown_*`); a non-decisive move
//!   needs no line.
//! - Totals and means are `None` when one of their inputs is unknown (`time_in_opening`) or when
//!   the denominator is empty or zero; no NaN or infinity reaches the output.
//! - The observation must be of this game and its rows must be consistent; otherwise they are
//!   left out and counted.

use std::collections::BTreeMap;

use chess_core::game_accuracy::game_accuracy_from;
use chess_core::score::{flip_score, score_to_cp};
use chess_core::{Color, Judgement, Phase, TimeClass};

use crate::types::{
    AnalysedGame, BinomialObs, ClockBucket, ConversionMetric, EndgameMetrics, EndgameOutcome,
    EnrichedMove, GameObservation, LineAnalysis, MissCounts, OpeningMetrics,
    PLAYER_GAME_METRICS_VERSION, PhaseValues, PlayerGameMetrics, QuietLoss, ResilienceMetric,
    SeriesKey, StrengthPly, WeaknessKey,
};
use crate::weakness::detectors::{
    self as d, GameView, LOW_CLOCK_SECONDS, OwnMove, PREP_MAX_WIN, TACTIC_MIN_GAIN, move_is_valid,
};
use crate::weakness::endgame::endgame_type;
use crate::weakness::episodes::Outcome;
use crate::weakness::observe_game::T_WIN;
use crate::weakness::sessions::expected_score;
use crate::weakness::time_trouble::{detect_time_trouble, is_time_loss, player_resigned};

/// Lila's game accuracy over the moves of `phase` (lila `AccuracyPercent.phaseAccuracies`: the
/// phase's slice of the W series, with its first mover as the start colour), so it is on the
/// game accuracy's scale. The series is White's W before the phase's first move, then after each
/// of its moves. `None` when the player made no move in the phase, or when a move of the phase
/// is unusable or its plies are not consecutive (a hole in the series).
fn phase_accuracy(v: &GameView<'_>, phase: Phase) -> Option<f64> {
    let slice: Vec<&EnrichedMove> = v
        .all
        .iter()
        .copied()
        .filter(|m| m.rec.phase == phase)
        .collect();
    let first = *slice.first()?;
    if slice.iter().any(|m| !move_is_valid(m))
        || slice.windows(2).any(|w| w[1].rec.ply != w[0].rec.ply + 1)
    {
        return None;
    }
    let white = |m: &EnrichedMove, w: f64| match m.rec.color {
        Color::White => w,
        Color::Black => 100.0 - w,
    };
    let mut series = Vec::with_capacity(slice.len() + 1);
    series.push(white(first, first.rec.win_before));
    series.extend(slice.iter().map(|m| white(m, m.rec.win_after)));
    finite(game_accuracy_from(&series, first.rec.color, v.player)?)
}

/// Missed punishment: ΔW at or above this after an opponent mistake or blunder.
pub const MISSED_PUNISH_DW: f64 = 10.0;
/// High-clock bucket: own clock above this many seconds.
pub const HIGH_CLOCK_SECONDS: f64 = 60.0;
/// Eval trend: player W after this ply ...
pub const TREND_FROM_PLY: u32 = 30;
/// ... subtracted from the player W after this ply.
pub const TREND_TO_PLY: u32 = 70;
/// Endgame conversion: entry W at or above this should be won.
pub const ENDGAME_CONVERT_MIN_ENTRY: f64 = 70.0;
/// Endgame hold: entry W at or below this should not be lost.
pub const ENDGAME_HOLD_MAX_ENTRY: f64 = 30.0;
/// A resignation is saveable with the player's final W at or above this.
pub const SAVEABLE_MIN_WIN: f64 = 25.0;

/// The metrics plus what was left out.
#[derive(Clone, Debug, PartialEq)]
pub struct GameMetricsScan {
    pub metrics: PlayerGameMetrics,
    /// Moves (either colour) with a non-finite or out-of-range W, ΔW or score.
    pub skipped_moves: u32,
    /// Moves dropped because their ply appears more than once (all copies).
    pub duplicate_plies: u32,
    /// Valid own moves whose accuracy is not finite in [0, 100].
    pub invalid_move_accuracy: u32,
    /// `ctx.accuracy` present but not finite in [0, 100].
    pub invalid_game_accuracy: bool,
    /// A rating present but not positive (no expected score).
    pub invalid_ratings: bool,
    /// Decisive own moves whose missing lines leave "one-move blunder" undecided.
    pub unknown_one_move_blunders: u32,
    /// Eligible decisive own moves without a refutation line (allowed tactic undecided).
    pub unknown_tactics_allowed: u32,
    /// The observation is of another game: weakness, motif, tactics and W9d were left out.
    pub observation_mismatch: bool,
    /// Observation rows (binomial or motif series) with `events > opportunities`, no
    /// opportunity, or a non-finite EPL; plus a non-finite `w9d_rho`.
    pub invalid_observation_rows: u32,
}

/// [`compute_game_metrics_counted`] without the counts.
pub fn compute_game_metrics(
    game: &AnalysedGame,
    obs: &GameObservation,
    strengths: Vec<StrengthPly>,
) -> PlayerGameMetrics {
    compute_game_metrics_counted(game, obs, strengths).metrics
}

pub fn compute_game_metrics_counted(
    game: &AnalysedGame,
    obs: &GameObservation,
    strengths: Vec<StrengthPly>,
) -> GameMetricsScan {
    let v = GameView::new(game);
    let ctx = &game.ctx;
    let mut scan = GameMetricsScan {
        metrics: PlayerGameMetrics::default(),
        skipped_moves: v.counts.skipped_moves,
        duplicate_plies: v.counts.duplicate_plies,
        invalid_move_accuracy: 0,
        invalid_game_accuracy: false,
        invalid_ratings: false,
        unknown_one_move_blunders: 0,
        unknown_tactics_allowed: 0,
        observation_mismatch: obs.game_id != ctx.game_id,
        invalid_observation_rows: 0,
    };
    let m = &mut scan.metrics;
    m.version = PLAYER_GAME_METRICS_VERSION;
    m.strengths = strengths;

    // Result and ratings.
    m.score = v.outcome.map(outcome_score);
    m.player_rating = ctx.player_rating;
    m.opponent_rating = ctx.opponent_rating;
    let positive = |r: Option<i32>| r.filter(|r| *r > 0);
    scan.invalid_ratings = [ctx.player_rating, ctx.opponent_rating]
        .iter()
        .any(|r| r.is_some_and(|r| r <= 0));
    m.expected_score = match (positive(ctx.player_rating), positive(ctx.opponent_rating)) {
        (Some(own), Some(opp)) => finite(expected_score(f64::from(own), f64::from(opp))),
        _ => None,
    };
    m.accuracy = ctx.accuracy.filter(|a| is_pct(*a));
    scan.invalid_game_accuracy = ctx.accuracy.is_some() && m.accuracy.is_none();

    // Per-move counts.
    m.moves = v.own.len() as u32;
    for o in &v.own {
        match o.m.rec.judgement {
            Some(Judgement::Blunder) => m.blunders += 1,
            Some(Judgement::Mistake) => m.mistakes += 1,
            Some(Judgement::Inaccuracy) => m.inaccuracies += 1,
            None => {}
        }
        if !is_pct(o.m.rec.accuracy) {
            scan.invalid_move_accuracy += 1;
        }
        match one_move_blunder(o) {
            Some(true) => m.one_move_blunders += 1,
            Some(false) => {}
            None => scan.unknown_one_move_blunders += 1,
        }
        if o.eligible && o.m.f.is_critical == Some(true) {
            m.sharp.positions += 1;
            m.sharp.errors += u32::from(o.m.rec.judgement.is_some());
        }
    }
    m.accuracy_by_phase = phase_values(|p| phase_accuracy(&v, p));
    m.missed_punish = missed_punish(&v);

    // Tactics allowed (move level) and the observation-based tactic counts.
    for o in v.own.iter().filter(|o| o.eligible && o.decisive) {
        match &o.m.f.refutation {
            Some(r) if is_tactic(r) => m.tactics.allowed += 1,
            Some(_) => {}
            None => scan.unknown_tactics_allowed += 1,
        }
    }
    if !scan.observation_mismatch {
        let (weakness, bad) = valid_binomial(&obs.binomial);
        scan.invalid_observation_rows += bad;
        m.weakness = weakness;
        // W3 is known for the game only when T8 computed it (every eligible own move has its
        // best line) and no W3 row was refused; otherwise `missed` stays `None`, never 0.
        let w3 = [WeaknessKey::W3Shallow, WeaknessKey::W3Deep];
        let w3_rows_valid = w3
            .iter()
            .all(|k| !obs.binomial.contains_key(k) || m.weakness.contains_key(k));
        if v.best_lines_complete && w3_rows_valid {
            let mut missed = 0;
            for b in w3.iter().filter_map(|k| m.weakness.get(k)) {
                m.tactics.opportunities += b.opportunities;
                missed += b.events;
            }
            m.tactics.missed = Some(missed);
        }
        for s in &obs.series {
            if let SeriesKey::MotifMissed(t) = s.series_key {
                if s.opportunities == 0 {
                    continue;
                }
                if s.events > s.opportunities || !s.epl.is_finite() {
                    scan.invalid_observation_rows += 1;
                    continue;
                }
                m.missed_by_motif.insert(
                    t,
                    MissCounts {
                        opportunities: s.opportunities,
                        misses: s.events,
                    },
                );
            }
        }
        m.w9d_rho = obs.w9d_rho.filter(|r| r.is_finite() && r.abs() <= 1.0);
        if obs.w9d_rho.is_some() && m.w9d_rho.is_none() {
            scan.invalid_observation_rows += 1;
        }
    }

    m.quiet = quiet(&v);
    m.eval_trend_15_to_35 = match (
        player_w_after(&v, TREND_FROM_PLY),
        player_w_after(&v, TREND_TO_PLY),
    ) {
        (Some(from), Some(to)) => finite(to - from),
        _ => None,
    };

    // Time.
    let fast = matches!(
        ctx.time_class,
        TimeClass::Bullet | TimeClass::Blitz | TimeClass::Rapid
    );
    if fast {
        for o in v.own.iter().filter(|o| o.eligible) {
            let bucket = match o.clock {
                Some(c) if c < LOW_CLOCK_SECONDS => &mut m.low_clock,
                Some(c) if c > HIGH_CLOCK_SECONDS => &mut m.high_clock,
                _ => continue,
            };
            *bucket = ClockBucket {
                moves: bucket.moves + 1,
                decisive: bucket.decisive + u32::from(o.decisive),
            };
        }
    }
    m.time_trouble = time_trouble(&v);
    m.lost_on_time = is_time_loss(ctx);
    m.time_per_move = phase_values(|p| {
        let t: Vec<f64> = v
            .own
            .iter()
            .filter(|o| o.m.rec.phase == p)
            .filter_map(|o| o.time_spent)
            .collect();
        mean(t.iter().sum(), t.len() as u32)
    });
    m.critical_time_ratio = critical_time_ratio(&v);
    m.time_in_opening = time_in_opening(&v);

    m.opening = opening(&v);
    m.endgame = endgame(&v);
    m.conversion = ConversionMetric::default();
    m.resilience = ResilienceMetric::default();
    let (winning, losing) = d::episodes(&v, Some(T_WIN));
    if let Some(e) = winning {
        m.conversion = ConversionMetric {
            episode: true,
            converted: Some(e.outcome == Outcome::Won),
            drop_ply: e.drop_ply,
        };
    }
    if let Some(e) = losing {
        m.resilience = ResilienceMetric {
            episode: true,
            saved: Some(e.outcome != Outcome::Lost),
        };
    }
    m.resigned_saveable = if player_resigned(ctx) {
        final_player_w(&v).map(|w| w >= SAVEABLE_MIN_WIN)
    } else {
        None
    };
    scan
}

fn outcome_score(o: Outcome) -> f64 {
    match o {
        Outcome::Won => 1.0,
        Outcome::Drawn => 0.5,
        Outcome::Lost => 0.0,
    }
}

fn is_pct(x: f64) -> bool {
    x.is_finite() && (0.0..=100.0).contains(&x)
}

fn finite(x: f64) -> Option<f64> {
    x.is_finite().then_some(if x == 0.0 { 0.0 } else { x })
}

fn mean(sum: f64, n: u32) -> Option<f64> {
    if n == 0 {
        return None;
    }
    finite(sum / f64::from(n))
}

fn phase_values(f: impl Fn(Phase) -> Option<f64>) -> PhaseValues {
    PhaseValues {
        opening: f(Phase::Opening),
        middlegame: f(Phase::Middlegame),
        endgame: f(Phase::Endgame),
    }
}

fn is_tactic(l: &LineAnalysis) -> bool {
    l.swing.gain >= TACTIC_MIN_GAIN || l.swing.mate_in.is_some()
}

/// `Some(true)` / `Some(false)` when decided, `None` when a missing line could change it.
fn one_move_blunder(o: &OwnMove) -> Option<bool> {
    if !o.decisive {
        return Some(false);
    }
    let refuted = o.m.f.refutation.as_ref().map(|r| r.swing.depth == Some(1));
    let missed_mate =
        o.m.f
            .best
            .as_ref()
            .map(|b| b.swing.mate_in == Some(1) && !o.m.rec.san.ends_with('#'));
    match (refuted, missed_mate) {
        (Some(true), _) | (_, Some(true)) => Some(true),
        (Some(false), Some(false)) => Some(false),
        _ => None,
    }
}

fn missed_punish(v: &GameView) -> MissCounts {
    let mut c = MissCounts::default();
    for o in &v.own {
        let Some(prev) = o.ply().checked_sub(1).and_then(|p| move_at(v, p)) else {
            continue;
        };
        let punishable = prev.rec.color != v.player
            && move_is_valid(prev)
            && matches!(
                prev.rec.judgement,
                Some(Judgement::Mistake | Judgement::Blunder)
            );
        if punishable {
            c.opportunities += 1;
            c.misses += u32::from(o.dw() >= MISSED_PUNISH_DW);
        }
    }
    c
}

/// The move at `ply` (after duplicate removal), valid or not.
fn move_at<'a>(v: &GameView<'a>, ply: u32) -> Option<&'a EnrichedMove> {
    v.all
        .binary_search_by_key(&ply, |m| m.rec.ply)
        .ok()
        .map(|i| v.all[i])
}

fn player_pov(v: &GameView, m: &EnrichedMove, w: f64) -> f64 {
    if m.rec.color == v.player {
        w
    } else {
        100.0 - w
    }
}

/// Player W after `ply`; `None` when that ply is missing or invalid.
fn player_w_after(v: &GameView, ply: u32) -> Option<f64> {
    let m = move_at(v, ply)?;
    move_is_valid(m).then(|| player_pov(v, m, m.rec.win_after))
}

fn final_player_w(v: &GameView) -> Option<f64> {
    let last = v.all.last()?;
    move_is_valid(last).then(|| player_pov(v, last, last.rec.win_after))
}

/// The W10 opportunity set (the shared [`d::is_quiet_move`] rule).
fn quiet(v: &GameView) -> QuietLoss {
    let mut q = QuietLoss::default();
    for o in v.own.iter().filter(|o| d::is_quiet_move(v, o)) {
        q.moves += 1;
        q.loss += o.dw();
    }
    q
}

fn time_trouble(v: &GameView) -> Option<bool> {
    if v.game.ctx.time_class == TimeClass::Daily {
        return None;
    }
    let recs: Vec<_> = v
        .all
        .iter()
        .map(|m| {
            let mut r = m.rec.clone();
            r.clock_before = r.clock_before.filter(|c| c.is_finite() && *c >= 0.0);
            r
        })
        .collect();
    detect_time_trouble(&recs, v.player, v.game.ctx.base_seconds).in_trouble
}

fn critical_time_ratio(v: &GameView) -> Option<f64> {
    let group = |critical: bool| {
        let t: Vec<f64> = v
            .own
            .iter()
            .filter(|o| o.m.f.is_critical == Some(critical))
            .filter_map(|o| o.time_spent)
            .collect();
        mean(t.iter().sum(), t.len() as u32)
    };
    let (crit, other) = (group(true)?, group(false)?);
    if other <= 0.0 {
        return None;
    }
    finite(crit / other)
}

/// Total own opening time; `None` without opening moves or when any own opening move (valid or
/// skipped) has no usable time.
fn time_in_opening(v: &GameView) -> Option<f64> {
    let opening = |m: &EnrichedMove| m.rec.phase == Phase::Opening;
    let skipped_opening = v
        .all
        .iter()
        .any(|m| m.rec.color == v.player && opening(m) && !move_is_valid(m));
    if skipped_opening {
        return None;
    }
    let moves: Vec<&OwnMove> = v.own.iter().filter(|o| opening(o.m)).collect();
    if moves.is_empty() {
        return None;
    }
    let mut total = 0.0;
    for o in moves {
        total += o.time_spent?;
    }
    finite(total)
}

fn opening(v: &GameView) -> OpeningMetrics {
    let end = v.opening_end();
    let white_cp = end.and_then(|(ply, _)| {
        let m = move_at(v, ply)?;
        let white = if m.rec.color == Color::White {
            m.score_after
        } else {
            flip_score(m.score_after)
        };
        Some(score_to_cp(white))
    });
    let win = end.map(|(_, w)| w);
    OpeningMetrics {
        line_key: v.game.opening_line_key.clone().filter(|k| !k.is_empty()),
        book_exit_ply: v.game.book_exit_ply,
        win_at_opening_end: win,
        out_worse: win.map(|w| w <= PREP_MAX_WIN),
        eval_at_opening_end_white_cp: white_cp,
    }
}

fn endgame(v: &GameView) -> EndgameMetrics {
    let mut e = EndgameMetrics::default();
    for o in v.own.iter().filter(|o| o.tb.is_some()) {
        e.tb_positions += 1;
        e.tb_errors += u32::from(o.tb.is_some_and(|(b, a)| a < b));
    }
    let Some((entry, w)) = v.endgame_entry() else {
        return e;
    };
    e.kind = endgame_type(&entry.rec.fen_before);
    e.entry_win = Some(w);
    e.outcome = v.outcome.and_then(|o| {
        if w >= ENDGAME_CONVERT_MIN_ENTRY {
            Some(if o == Outcome::Won {
                EndgameOutcome::Converted
            } else {
                EndgameOutcome::FailedConversion
            })
        } else if w <= ENDGAME_HOLD_MAX_ENTRY {
            Some(if o == Outcome::Lost {
                EndgameOutcome::FailedHold
            } else {
                EndgameOutcome::Held
            })
        } else {
            None
        }
    });
    e
}

/// Binomial rows that are consistent (`events ≤ opportunities`, at least one opportunity,
/// finite EPL), and the count of the others.
fn valid_binomial(
    rows: &BTreeMap<WeaknessKey, BinomialObs>,
) -> (BTreeMap<WeaknessKey, BinomialObs>, u32) {
    let mut bad = 0;
    let mut out = BTreeMap::new();
    for (k, b) in rows {
        if k.is_binomial()
            && b.opportunities > 0
            && b.events <= b.opportunities
            && b.epl.is_finite()
            && b.epl >= 0.0
        {
            out.insert(*k, b.clone());
        } else {
            bad += 1;
        }
    }
    (out, bad)
}

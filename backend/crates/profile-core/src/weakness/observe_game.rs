//! Per-game observations (R2 Task 8, TS M2 Task 7 `observeGame`).
//!
//! Composes the detectors of [`crate::weakness::detectors`]:
//! - a binomial key is present only with at least one opportunity (absent = no evidence);
//! - EPL (`ΔW / 100`) is attributed once per move by [`EPL_PRECEDENCE`]: the first key in that
//!   list that claims a move takes its ΔW, the later ones get 0 for it. Keys outside the list keep
//!   the ΔW of their own events (W9a, W9b, W9c, W11) or their game-level cost (W5, W6). Each is
//!   computed independently of every other key, so there is no order for the result to depend
//!   on. Their EPL overlaps the precedence keys' and each other's, so consumers must never sum
//!   EPL across keys outside `EPL_PRECEDENCE` (ADR-R2-4 §8);
//! - `exemplar_plies` are the event plies by ΔW descending (ties by ply);
//! - series: every binomial key, `motif.missed:*`, `motif.allowed:*`, `endgame:*`, `opening:*`.

use std::collections::{BTreeMap, BTreeSet};

use crate::constants::EPL_PRECEDENCE;
use crate::types::{AnalysedGame, BinomialObs, GameObservation, SeriesKey, SeriesObs, WeaknessKey};
use crate::weakness::detectors::{self as d, Detection, GameView};
use crate::weakness::endgame::endgame_type;
use crate::weakness::episodes::Outcome;

/// Fixed `T_win` until improvement-engine section 4.3 is fitted per band (ADR-R2-4).
pub const T_WIN: f64 = 85.0;

/// An observation plus what was left out.
#[derive(Clone, Debug, PartialEq)]
pub struct ObservationScan {
    pub observation: GameObservation,
    /// Moves (either colour) with a non-finite or out-of-range W, ΔW or score.
    pub skipped_moves: u32,
    pub invalid_clocks: u32,
    pub invalid_time_spent: u32,
    pub invalid_tablebase: u32,
    /// Difficulty that is not a valid Maia value (never substituted).
    pub invalid_difficulty: u32,
    /// Eligible own moves without a best line; any of them leaves W3 and W10 out for the game.
    pub moves_without_best_line: u32,
    /// Moves dropped because their ply appears more than once (all copies dropped).
    pub duplicate_plies: u32,
    /// `ctx.score` missing or not 1 / 0.5 / 0: no episode, endgame or opening evidence.
    pub invalid_score: bool,
    /// `t_win` not finite or outside (50, 100]: no winning episode.
    pub invalid_t_win: bool,
}

/// Player-POV W after the last opening ply (W7.prep input, also for Task 9).
pub fn win_at_opening_end(game: &AnalysedGame) -> Option<f64> {
    GameView::new(game).win_at_opening_end()
}

/// [`observe_game_counted`] without the counts.
pub fn observe_game(game: &AnalysedGame, t_win: f64) -> GameObservation {
    observe_game_counted(game, t_win).observation
}

pub fn observe_game_counted(game: &AnalysedGame, t_win: f64) -> ObservationScan {
    use WeaknessKey::*;
    let v = GameView::new(game);
    let valid_t_win = (t_win.is_finite() && t_win > 50.0 && t_win <= 100.0).then_some(t_win);

    let detections: Vec<(WeaknessKey, Detection)> = vec![
        (W1, d::w1(&v)),
        (W2, d::w2(&v)),
        (W3Shallow, d::w3_shallow(&v)),
        (W3Deep, d::w3_deep(&v)),
        (W4Missed, d::w4_missed(&v)),
        (W4Allowed, d::w4_allowed(&v)),
        (W5, d::w5(&v, valid_t_win)),
        (W6, d::w6(&v)),
        (W7Traps, d::w7_traps(&v)),
        (W7Prep, d::w7_prep(&v)),
        (W8Basics, d::w8_basics(&v)),
        (W8Advanced, d::w8_advanced(&v)),
        (W9a, d::w9a(&v)),
        (W9b, d::w9b(&v)),
        (W9c, d::w9c(&v)),
        (W10, d::w10(&v)),
        (W11, d::w11(&v)),
    ];
    let detections: BTreeMap<WeaknessKey, Detection> = detections
        .into_iter()
        .filter(|(_, det)| det.opportunities > 0)
        .collect();

    // EPL attribution.
    let mut epl: BTreeMap<WeaknessKey, f64> = BTreeMap::new();
    let mut claimed: BTreeSet<u32> = BTreeSet::new();
    for k in EPL_PRECEDENCE {
        if let Some(det) = detections.get(&k) {
            let mut sum = 0.0;
            for &(ply, dw) in &det.claims {
                if claimed.insert(ply) {
                    sum += dw / 100.0;
                }
            }
            epl.insert(k, sum);
        }
    }
    let mut binomial = BTreeMap::new();
    for (k, det) in &detections {
        let e = epl.get(k).copied().unwrap_or_else(|| {
            det.fixed_epl
                .unwrap_or_else(|| det.claims.iter().map(|(_, dw)| dw / 100.0).sum())
        });
        let mut ex = det.exemplars.clone();
        ex.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        let mut plies: Vec<u32> = Vec::with_capacity(ex.len());
        for (p, _) in ex {
            if !plies.contains(&p) {
                plies.push(p);
            }
        }
        binomial.insert(
            *k,
            BinomialObs {
                events: det.events,
                opportunities: det.opportunities,
                epl: finite(e),
                exemplar_plies: plies,
            },
        );
    }

    let series = build_series(&v, &binomial);
    let invalid_score = v.outcome.is_none();
    ObservationScan {
        observation: GameObservation {
            game_id: game.ctx.game_id,
            binomial,
            w9d_rho: d::w9d_rho(&v),
            series,
        },
        skipped_moves: v.counts.skipped_moves,
        invalid_clocks: v.counts.invalid_clocks,
        invalid_time_spent: v.counts.invalid_time_spent,
        invalid_tablebase: v.counts.invalid_tablebase,
        invalid_difficulty: v.counts.invalid_difficulty,
        moves_without_best_line: v.counts.moves_without_best_line,
        duplicate_plies: v.counts.duplicate_plies,
        invalid_score,
        invalid_t_win: valid_t_win.is_none(),
    }
}

fn finite(x: f64) -> f64 {
    if x.is_finite() { x } else { 0.0 }
}

fn row(series_key: SeriesKey, events: u32, opportunities: u32, epl: f64) -> SeriesObs {
    SeriesObs {
        series_key,
        events,
        opportunities,
        epl: finite(epl),
    }
}

fn build_series(v: &GameView, binomial: &BTreeMap<WeaknessKey, BinomialObs>) -> Vec<SeriesObs> {
    let mut out: Vec<SeriesObs> = binomial
        .iter()
        .map(|(k, b)| row(SeriesKey::Weakness(*k), b.events, b.opportunities, b.epl))
        .collect();
    for (t, (o, e, epl)) in d::motif_missed(v) {
        out.push(row(SeriesKey::MotifMissed(t), e, o, epl));
    }
    for (t, (o, e, epl)) in d::motif_allowed(v) {
        out.push(row(SeriesKey::MotifAllowed(t), e, o, epl));
    }
    let phase_loss = |phase| {
        v.own
            .iter()
            .filter(|o| o.m.rec.phase == phase)
            .map(|o| o.dw() / 100.0)
            .sum::<f64>()
    };
    if let (Some(outcome), Some((entry, w))) = (v.outcome, v.endgame_entry())
        && let Some(kind) = endgame_type(&entry.rec.fen_before)
        && w >= d::ENDGAME_MIN_ENTRY
    {
        let event = if w >= d::ENDGAME_WINNING_ENTRY {
            outcome != Outcome::Won
        } else {
            outcome == Outcome::Lost
        };
        let epl = if event {
            phase_loss(chess_core::Phase::Endgame)
        } else {
            0.0
        };
        out.push(row(SeriesKey::Endgame(kind), u32::from(event), 1, epl));
    }
    if let (Some(outcome), Some(line)) = (v.outcome, v.game.opening_line_key.as_deref())
        && !line.is_empty()
    {
        let lost = outcome == Outcome::Lost;
        let epl = if lost {
            phase_loss(chess_core::Phase::Opening)
        } else {
            0.0
        };
        out.push(row(
            SeriesKey::Opening(line.to_owned()),
            u32::from(lost),
            1,
            epl,
        ));
    }
    out
}

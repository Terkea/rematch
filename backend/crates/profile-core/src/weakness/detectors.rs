//! Weakness detectors W1-W12 (R2 Task 8, rules TS M2 Task 7 and improvement-engine section 6.2).
//!
//! One function per key. Each takes a [`GameView`] (the game's valid moves, sanitised) and
//! returns a [`Detection`]: opportunities, events and the moves whose ΔW the key may claim as EPL.
//! [`crate::weakness::observe_game`] composes them and attributes EPL by `EPL_PRECEDENCE`.
//!
//! Honesty rules (ADR-R2-4):
//! - A move is an opportunity only when its event can be decided. A missing line, threat answer,
//!   difficulty, clock or tablebase value is "unknown", never a non-event. When the missing value
//!   cannot change the answer (a non-decisive move is never a W1 event, whatever its refutation),
//!   the move stays a computable opportunity.
//! - W3 and W10 need the best line of every eligible own move: enrichment fills missing PVs only
//!   on critical plies, so a partial set would be selected on the outcome. With any eligible own
//!   move lacking its best line, W3, W10 and `motif.missed:*` are left out for the game.
//! - Difficulty is Maia only, re-checked here; nothing is substituted.
//! - Invalid inputs are skipped and counted by the caller; nothing here emits NaN or infinity.

use std::collections::BTreeMap;

use chess_core::{Color, EngineScore, Judgement, Phase, TimeClass};

use crate::board::material_signature;
use crate::features::{eval_in_range, maia_difficulty};
use crate::stats::correlation::spearman;
use crate::types::{AnalysedGame, Difficulty, EnrichedMove, MotifTag, Wdl};
use crate::weakness::endgame::{is_basic_signature, is_tablebase_eligible};
use crate::weakness::episodes::{EpisodeOpts, Outcome, extract_episodes_counted};
use crate::weakness::time_trouble::detect_time_trouble;

/// Decisive error: ΔW ≥ 15 (Lichess blunder).
pub const DECISIVE_DW: f64 = 15.0;
/// Inaccuracy-grade loss: ΔW ≥ 5 (W10 event).
pub const INACCURACY_DW: f64 = 5.0;
/// Eligible: `|W_before − 50| < 40`.
pub const ELIGIBLE_RADIUS: f64 = 40.0;
/// Material a line must win to count as a tactic, in pawns.
pub const TACTIC_MIN_GAIN: f64 = 2.0;
/// W1: the refutation realises its swing within this many plies.
pub const W1_MAX_DEPTH: u32 = 3;
/// W3: depth ≤ 3 is shallow, ≥ 4 deep (ADR-R2-4).
pub const W3_SHALLOW_MAX_DEPTH: u32 = 3;
/// W4.missed: mate in at most this many moves.
pub const W4_MAX_MATE: i32 = 4;
/// W6 event cost (ADR-R2-4).
pub const W6_EVENT_EPL: f64 = 0.5;
/// W7.traps: decisive errors up to this ply.
pub const TRAP_MAX_PLY: u32 = 20;
/// W7.prep: player W at the end of the opening at or below this is "out of the opening worse".
pub const PREP_MAX_WIN: f64 = 40.0;
/// W9b: own clock below this many seconds.
pub const LOW_CLOCK_SECONDS: f64 = 30.0;
/// W9c: time spent below this fraction of the player's median.
pub const RUSH_FRACTION: f64 = 0.5;
/// W9d: at least this many own moves with time spent and a MultiPV gap.
pub const W9D_MIN_MOVES: usize = 10;
/// W10: `|cp| ≤ 150` before the move.
pub const QUIET_MAX_CP: i32 = 150;
/// W11: Maia `p_ok` at least this.
pub const EASY_MIN_P_OK: f64 = 0.8;
/// Endgame series: entry W ≥ 30 opens an opportunity; ≥ 70 should be won.
pub const ENDGAME_MIN_ENTRY: f64 = 30.0;
pub const ENDGAME_WINNING_ENTRY: f64 = 70.0;

/// One of the player's valid moves, with its inputs sanitised.
#[derive(Clone, Debug)]
pub struct OwnMove<'a> {
    pub m: &'a EnrichedMove,
    /// `|W_before − 50| < 40`.
    pub eligible: bool,
    /// ΔW ≥ 15.
    pub decisive: bool,
    /// Finite, non-negative clock before the move.
    pub clock: Option<f64>,
    /// Finite, non-negative time spent.
    pub time_spent: Option<f64>,
    /// Maia difficulty (never at plies ≤ 10, never another model).
    pub difficulty: Option<Difficulty>,
    /// `(before, after)` WDL, both present, in −1..=1, on a ≤ 5-man position.
    pub tb: Option<(Wdl, Wdl)>,
}

impl OwnMove<'_> {
    pub fn ply(&self) -> u32 {
        self.m.rec.ply
    }

    pub fn dw(&self) -> f64 {
        self.m.rec.delta_w
    }

    fn at(&self) -> (u32, f64) {
        (self.ply(), self.dw())
    }
}

/// Counts of inputs left out while building a [`GameView`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ViewCounts {
    /// Moves (either colour) with a non-finite or out-of-range W, ΔW or score.
    pub skipped_moves: u32,
    pub invalid_clocks: u32,
    pub invalid_time_spent: u32,
    pub invalid_tablebase: u32,
    pub invalid_difficulty: u32,
    /// Eligible own moves without a best line (W3 and W10 are then left out).
    pub moves_without_best_line: u32,
    /// Moves dropped because their ply appears more than once (every copy is dropped, so the
    /// result does not depend on input order).
    pub duplicate_plies: u32,
}

/// The game as the detectors see it.
#[derive(Clone, Debug)]
pub struct GameView<'a> {
    pub game: &'a AnalysedGame,
    /// Every move, both colours, in ply order (valid or not).
    pub all: Vec<&'a EnrichedMove>,
    /// The player's valid moves, in ply order.
    pub own: Vec<OwnMove<'a>>,
    /// Plies of the player's moves that failed validation (unknown, not non-events).
    pub skipped_own_plies: Vec<u32>,
    pub player: Color,
    /// From `ctx.score` (1, 0.5 or 0); `None` when unknown or invalid.
    pub outcome: Option<Outcome>,
    /// Every eligible own move has a best line.
    pub best_lines_complete: bool,
    pub counts: ViewCounts,
}

/// Whether a move's W, ΔW and scores are usable.
pub fn move_is_valid(m: &EnrichedMove) -> bool {
    let w = |x: f64| x.is_finite() && (0.0..=100.0).contains(&x);
    w(m.rec.win_before)
        && w(m.rec.win_after)
        && w(m.rec.delta_w)
        && eval_in_range(m.score_before)
        && eval_in_range(m.score_after)
}

fn clean_seconds(v: Option<f64>, invalid: &mut u32) -> Option<f64> {
    match v {
        Some(x) if x.is_finite() && x >= 0.0 => Some(x),
        Some(_) => {
            *invalid += 1;
            None
        }
        None => None,
    }
}

impl<'a> GameView<'a> {
    pub fn new(game: &'a AnalysedGame) -> Self {
        let player = game.ctx.player_color;
        let mut all: Vec<&EnrichedMove> = game.moves.iter().collect();
        all.sort_by_key(|m| m.rec.ply);
        let mut counts = ViewCounts::default();
        let before = all.len();
        let mut per_ply: BTreeMap<u32, u32> = BTreeMap::new();
        for m in &all {
            *per_ply.entry(m.rec.ply).or_default() += 1;
        }
        all.retain(|m| per_ply[&m.rec.ply] == 1);
        counts.duplicate_plies = (before - all.len()) as u32;
        let mut own = Vec::new();
        let mut skipped_own_plies = Vec::new();
        for m in &all {
            if !move_is_valid(m) {
                counts.skipped_moves += 1;
                if m.rec.color == player {
                    skipped_own_plies.push(m.rec.ply);
                }
                continue;
            }
            if m.rec.color != player {
                continue;
            }
            let clock = clean_seconds(m.rec.clock_before, &mut counts.invalid_clocks);
            let time_spent = clean_seconds(m.rec.time_spent, &mut counts.invalid_time_spent);
            let raw = m.raw.as_ref();
            let difficulty =
                match maia_difficulty(m.rec.ply, raw.and_then(|r| r.difficulty.as_ref())) {
                    Ok(d) => d,
                    Err(_) => {
                        counts.invalid_difficulty += 1;
                        None
                    }
                };
            let tb = match raw.map(|r| (r.tb_wdl_before, r.tb_wdl_after)) {
                Some((Some(b), Some(a))) => {
                    let in_range = |v: Wdl| (-1..=1).contains(&v);
                    if in_range(b)
                        && in_range(a)
                        && is_tablebase_eligible(&m.rec.fen_before) == Some(true)
                    {
                        Some((b, a))
                    } else {
                        counts.invalid_tablebase += 1;
                        None
                    }
                }
                _ => None,
            };
            let eligible = (m.rec.win_before - 50.0).abs() < ELIGIBLE_RADIUS;
            if eligible && m.f.best.is_none() {
                counts.moves_without_best_line += 1;
            }
            own.push(OwnMove {
                m,
                eligible,
                decisive: m.rec.delta_w >= DECISIVE_DW,
                clock,
                time_spent,
                difficulty,
                tb,
            });
        }
        GameView {
            game,
            all,
            own,
            skipped_own_plies,
            player,
            outcome: game.ctx.score.and_then(Outcome::from_score),
            best_lines_complete: counts.moves_without_best_line == 0,
            counts,
        }
    }

    fn eligible(&self) -> impl Iterator<Item = &OwnMove<'a>> {
        self.own.iter().filter(|o| o.eligible)
    }

    /// The player's W from a move's own W.
    fn player_pov(&self, m: &EnrichedMove, w: f64) -> f64 {
        if m.rec.color == self.player {
            w
        } else {
            100.0 - w
        }
    }

    /// Player W after the last opening ply; `None` without an opening ply or when it is invalid.
    pub fn win_at_opening_end(&self) -> Option<f64> {
        self.opening_end().map(|(_, w)| w)
    }

    /// The last opening ply and the player's W after it.
    pub fn opening_end(&self) -> Option<(u32, f64)> {
        let last = self
            .all
            .iter()
            .rev()
            .find(|m| m.rec.phase == Phase::Opening)?;
        move_is_valid(last).then(|| (last.rec.ply, self.player_pov(last, last.rec.win_after)))
    }

    /// The first endgame ply and the player's W before it; `None` without one or when invalid.
    pub fn endgame_entry(&self) -> Option<(&'a EnrichedMove, f64)> {
        let first = *self.all.iter().find(|m| m.rec.phase == Phase::Endgame)?;
        move_is_valid(first).then(|| (first, self.player_pov(first, first.rec.win_before)))
    }

    fn time_class_is(&self, classes: &[TimeClass]) -> bool {
        classes.contains(&self.game.ctx.time_class)
    }
}

/// What one detector found in one game.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Detection {
    pub opportunities: u32,
    pub events: u32,
    /// `(ply, ΔW)` of the moves whose ΔW this key may claim as EPL.
    pub claims: Vec<(u32, f64)>,
    /// A game-level EPL (W5, W6) that replaces the claims.
    pub fixed_epl: Option<f64>,
    /// `(ply, ΔW)` of the exemplar plies.
    pub exemplars: Vec<(u32, f64)>,
}

impl Detection {
    /// Per-move opportunities, events where `event` holds.
    fn per_move<'b, 'a: 'b>(
        moves: impl Iterator<Item = &'b OwnMove<'a>>,
        event: impl Fn(&OwnMove<'a>) -> bool,
    ) -> Self {
        let mut d = Detection::default();
        for o in moves {
            d.opportunities += 1;
            if event(o) {
                d.events += 1;
                d.claims.push(o.at());
            }
        }
        d.exemplars = d.claims.clone();
        d
    }
}

fn tactic(l: &crate::types::LineAnalysis) -> bool {
    l.swing.gain >= TACTIC_MIN_GAIN || l.swing.mate_in.is_some()
}

/// W1 and `motif.allowed:*` opportunity: an eligible move whose refutation is known, or that is
/// not decisive (then no refutation can make it an event).
pub fn refutation_opportunity(o: &OwnMove) -> bool {
    o.eligible && (o.m.f.refutation.is_some() || !o.decisive)
}

/// W1 hung material: eligible moves; decisive with a refutation winning ≥ 2 within 3 plies and no
/// mate. A decisive move without a refutation line is not computable.
pub fn w1(v: &GameView) -> Detection {
    Detection::per_move(v.own.iter().filter(|o| refutation_opportunity(o)), |o| {
        o.decisive
            && o.m.f.refutation.as_ref().is_some_and(|r| {
                r.swing.gain >= TACTIC_MIN_GAIN
                    && r.swing.depth.is_some_and(|d| d <= W1_MAX_DEPTH)
                    && r.swing.mate_in.is_none()
            })
    })
}

/// W2 missed threat: eligible moves where a threat existed; decisive and not addressed.
pub fn w2(v: &GameView) -> Detection {
    Detection::per_move(
        v.eligible().filter(|o| {
            o.m.f.threat_existed == Some(true) && (o.m.f.threat_addressed.is_some() || !o.decisive)
        }),
        |o| o.decisive && o.m.f.threat_addressed == Some(false),
    )
}

/// W3 opportunities of one depth class: eligible moves whose best line is a tactic.
fn w3_moves<'b, 'a: 'b>(
    v: &'b GameView<'a>,
    shallow: bool,
) -> impl Iterator<Item = &'b OwnMove<'a>> {
    let complete = v.best_lines_complete;
    v.eligible().filter(move |o| {
        complete
            && o.m.f.best.as_ref().is_some_and(|b| {
                tactic(b)
                    && b.swing
                        .depth
                        .is_some_and(|d| (d <= W3_SHALLOW_MAX_DEPTH) == shallow)
            })
    })
}

fn w3_event(o: &OwnMove) -> bool {
    o.dw() >= DECISIVE_DW && o.m.f.in_top2 != Some(true)
}

/// W3.shallow missed own tactic, best line realised within 3 plies.
pub fn w3_shallow(v: &GameView) -> Detection {
    Detection::per_move(w3_moves(v, true), w3_event)
}

/// W3.deep missed own tactic, best line realised at 4 plies or later.
pub fn w3_deep(v: &GameView) -> Detection {
    Detection::per_move(w3_moves(v, false), w3_event)
}

/// W4.missed: own moves (decided allowed) with a mate in 1-4 before; the mate is gone after.
pub fn w4_missed(v: &GameView) -> Detection {
    Detection::per_move(
        v.own.iter().filter(
            |o| matches!(o.m.score_before, EngineScore::Mate(n) if (1..=W4_MAX_MATE).contains(&n)),
        ),
        |o| !matches!(o.m.score_after, EngineScore::Mate(n) if n > 0),
    )
}

/// W4.allowed: eligible moves; not already being mated before, being mated after.
pub fn w4_allowed(v: &GameView) -> Detection {
    let mated = |s: EngineScore| matches!(s, EngineScore::Mate(n) if n < 0);
    Detection::per_move(v.eligible(), |o| {
        !mated(o.m.score_before) && mated(o.m.score_after)
    })
}

/// Winning and losing episodes of the player (needs a known result and a valid `t_win`).
pub fn episodes(
    v: &GameView,
    t_win: Option<f64>,
) -> (
    Option<crate::weakness::episodes::Episode>,
    Option<crate::weakness::episodes::Episode>,
) {
    // Every own move, invalid ones with W = NaN: an invalid move breaks a run (T7); it never
    // silently drops out from between two run moves.
    let recs: Vec<_> = v
        .all
        .iter()
        .filter(|m| m.rec.color == v.player)
        .map(|m| {
            let mut r = m.rec.clone();
            if !move_is_valid(m) {
                r.win_before = f64::NAN;
                r.win_after = f64::NAN;
            }
            r
        })
        .collect();
    let opts = EpisodeOpts {
        t_win: t_win.unwrap_or(f64::INFINITY),
        ..EpisodeOpts::default()
    };
    let scan = extract_episodes_counted(&recs, v.player, v.game.ctx.score, &opts);
    (scan.winning.filter(|_| t_win.is_some()), scan.losing)
}

/// W5 conversion failure: a winning episode not won; EPL `1 − score`.
pub fn w5(v: &GameView, t_win: Option<f64>) -> Detection {
    let Some(e) = episodes(v, t_win).0 else {
        return Detection::default();
    };
    let failed = e.outcome != Outcome::Won;
    let score = match e.outcome {
        Outcome::Won => 1.0,
        Outcome::Drawn => 0.5,
        Outcome::Lost => 0.0,
    };
    Detection {
        opportunities: 1,
        events: u32::from(failed),
        claims: vec![],
        fixed_epl: Some(if failed { 1.0 - score } else { 0.0 }),
        exemplars: e
            .drop_ply
            .filter(|_| failed)
            .map(|p| (p, dw_at(v, p)))
            .into_iter()
            .collect(),
    }
}

/// W6 defence: a losing episode that was lost; event cost 0.5.
pub fn w6(v: &GameView) -> Detection {
    let Some(e) = episodes(v, None).1 else {
        return Detection::default();
    };
    let lost = e.outcome == Outcome::Lost;
    Detection {
        opportunities: 1,
        events: u32::from(lost),
        claims: vec![],
        fixed_epl: Some(if lost { W6_EVENT_EPL } else { 0.0 }),
        exemplars: vec![],
    }
}

fn dw_at(v: &GameView, ply: u32) -> f64 {
    v.own
        .iter()
        .find(|o| o.ply() == ply)
        .map_or(0.0, OwnMove::dw)
}

/// W7.traps: one opportunity per game with an own move by ply 20; event = a decisive error there.
pub fn w7_traps(v: &GameView) -> Detection {
    let early: Vec<&OwnMove> = v.own.iter().filter(|o| o.ply() <= TRAP_MAX_PLY).collect();
    if early.is_empty() {
        return Detection::default();
    }
    let claims: Vec<(u32, f64)> = early
        .iter()
        .filter(|o| o.decisive)
        .map(|o| o.at())
        .collect();
    // A skipped early own move could have been the trap: "no trap" is then unknown.
    if claims.is_empty() && v.skipped_own_plies.iter().any(|p| *p <= TRAP_MAX_PLY) {
        return Detection::default();
    }
    Detection {
        opportunities: 1,
        events: u32::from(!claims.is_empty()),
        exemplars: claims.clone(),
        claims,
        fixed_epl: None,
    }
}

/// W7.prep: one opportunity when the player's W at the opening end is known; event W ≤ 40. Its
/// EPL is the player's opening-phase ΔW (moves not claimed by a higher-precedence key).
pub fn w7_prep(v: &GameView) -> Detection {
    let Some((end_ply, w)) = v.opening_end() else {
        return Detection::default();
    };
    let worse = w <= PREP_MAX_WIN;
    Detection {
        opportunities: 1,
        events: u32::from(worse),
        claims: if worse {
            v.own
                .iter()
                .filter(|o| o.m.rec.phase == Phase::Opening)
                .map(|o| o.at())
                .collect()
        } else {
            vec![]
        },
        fixed_epl: None,
        // The position the player came out of the opening with.
        exemplars: if worse { vec![(end_ply, 0.0)] } else { vec![] },
    }
}

fn w8(v: &GameView, basics: bool) -> Detection {
    Detection::per_move(
        v.own.iter().filter(|o| {
            o.tb.is_some() && is_basic_signature(&material_signature(&o.m.rec.fen_before)) == basics
        }),
        |o| o.tb.is_some_and(|(b, a)| a < b),
    )
}

/// W8.basics: tablebase drops in the basic signatures.
pub fn w8_basics(v: &GameView) -> Detection {
    w8(v, true)
}

/// W8.advanced: tablebase drops in every other ≤ 5-man position.
pub fn w8_advanced(v: &GameView) -> Detection {
    w8(v, false)
}

/// W9a time trouble: one opportunity per clocked, non-daily game; EPL = own ΔW from the first
/// low-clock ply on.
pub fn w9a(v: &GameView) -> Detection {
    if v.game.ctx.time_class == TimeClass::Daily {
        return Detection::default();
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
    let t = detect_time_trouble(&recs, v.player, v.game.ctx.base_seconds);
    let Some(in_trouble) = t.in_trouble else {
        return Detection::default();
    };
    let first = t.first_ply.filter(|_| in_trouble);
    Detection {
        opportunities: 1,
        events: u32::from(in_trouble),
        claims: first.map_or_else(Vec::new, |p| {
            v.own
                .iter()
                .filter(|o| o.ply() >= p)
                .map(|o| o.at())
                .collect()
        }),
        fixed_epl: None,
        exemplars: first.map(|p| (p, dw_at(v, p))).into_iter().collect(),
    }
}

/// W9b time-pressure collapse: eligible moves under 30 s in bullet, blitz or rapid; decisive.
pub fn w9b(v: &GameView) -> Detection {
    if !v.time_class_is(&[TimeClass::Bullet, TimeClass::Blitz, TimeClass::Rapid]) {
        return Detection::default();
    }
    Detection::per_move(
        v.eligible()
            .filter(|o| o.clock.is_some_and(|c| c < LOW_CLOCK_SECONDS)),
        |o| o.decisive,
    )
}

/// Median of the player's valid time-spent values.
pub fn median_time_spent(v: &GameView) -> Option<f64> {
    let mut t: Vec<f64> = v.own.iter().filter_map(|o| o.time_spent).collect();
    if t.is_empty() {
        return None;
    }
    t.sort_by(f64::total_cmp);
    let n = t.len();
    Some(if n % 2 == 1 {
        t[n / 2]
    } else {
        (t[n / 2 - 1] + t[n / 2]) / 2.0
    })
}

/// W9c rushing critical moves: eligible critical moves with time spent; a mistake or blunder in
/// under half the player's median time.
pub fn w9c(v: &GameView) -> Detection {
    let Some(median) = median_time_spent(v) else {
        return Detection::default();
    };
    Detection::per_move(
        v.eligible()
            .filter(|o| o.m.f.is_critical == Some(true) && o.time_spent.is_some()),
        |o| {
            o.time_spent.is_some_and(|t| t < RUSH_FRACTION * median)
                && matches!(
                    o.m.rec.judgement,
                    Some(Judgement::Mistake | Judgement::Blunder)
                )
        },
    )
}

/// W9d: Spearman ρ of time spent vs MultiPV gap over ≥ 10 own moves that have both (the gap is
/// never substituted); `None` otherwise.
pub fn w9d_rho(v: &GameView) -> Option<f64> {
    let (t, g): (Vec<f64>, Vec<f64>) = v
        .own
        .iter()
        .filter_map(|o| {
            let gap = o.m.f.multipv_gap.filter(|x| x.is_finite())?;
            Some((o.time_spent?, gap))
        })
        .unzip();
    if t.len() < W9D_MIN_MOVES {
        return None;
    }
    spearman(&t, &g).filter(|r| r.is_finite())
}

/// W10 positional drift: eligible quiet middlegame moves; ΔW ≥ 5.
pub fn w10(v: &GameView) -> Detection {
    let complete = v.best_lines_complete;
    Detection::per_move(
        v.eligible().filter(|o| {
            complete
                && o.m.rec.phase == Phase::Middlegame
                && o.m.f.is_critical != Some(true)
                && o.m.f.best.as_ref().is_some_and(|b| b.swing.depth.is_none())
                && matches!(o.m.score_before, EngineScore::Cp(cp) if cp.abs() <= QUIET_MAX_CP)
        }),
        |o| o.dw() >= INACCURACY_DW,
    )
}

/// W11 easy-move errors: eligible moves with Maia `p_ok ≥ 0.8`; decisive.
pub fn w11(v: &GameView) -> Detection {
    Detection::per_move(
        v.eligible().filter(|o| {
            o.difficulty
                .as_ref()
                .is_some_and(|d| d.p_ok >= EASY_MIN_P_OK)
        }),
        |o| o.decisive,
    )
}

/// Per-motif `(opportunities, events, epl)`.
pub type MotifCounts = BTreeMap<MotifTag, (u32, u32, f64)>;

/// `motif.missed:<t>`: W3 opportunities whose best line has `t`; events are W3 events.
pub fn motif_missed(v: &GameView) -> MotifCounts {
    let mut out = MotifCounts::new();
    for o in w3_moves(v, true).chain(w3_moves(v, false)) {
        let event = w3_event(o);
        for t in o.m.f.best.iter().flat_map(|b| b.tags.iter()) {
            let e = out.entry(*t).or_insert((0, 0, 0.0));
            e.0 += 1;
            if event {
                e.1 += 1;
                e.2 += o.dw() / 100.0;
            }
        }
    }
    out
}

/// `motif.allowed:<t>`: every W1 opportunity, for every tag; events are decisive moves whose
/// refutation has `t`.
pub fn motif_allowed(v: &GameView) -> MotifCounts {
    let opps: Vec<&OwnMove> = v.own.iter().filter(|o| refutation_opportunity(o)).collect();
    let mut out = MotifCounts::new();
    if opps.is_empty() {
        return out;
    }
    for t in MotifTag::ALL {
        let mut c = (opps.len() as u32, 0, 0.0);
        for o in opps.iter().filter(|o| o.decisive) {
            if o.m
                .f
                .refutation
                .as_ref()
                .is_some_and(|r| r.tags.contains(&t))
            {
                c.1 += 1;
                c.2 += o.dw() / 100.0;
            }
        }
        out.insert(t, c);
    }
    out
}

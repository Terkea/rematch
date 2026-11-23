//! Flaw injectors: each one makes a synthetic player worse in one known way, at a known rate.
//!
//! An injector is a [`Flaw`]. For every own move it says whether the move is an opportunity of
//! the weakness it targets (as the detector of `profile-core::weakness::detectors` would count
//! it), whether the injector is armed there, and what it wants played when its coin comes up
//! ([`Goal`]). The generator ([`crate::generate`]) executes the goal with the engine and keeps
//! the tally ([`Tally`]) from which the ground-truth rate is computed ([`Tally::truth`]).
//!
//! The thresholds are the detectors' own constants (`DECISIVE_DW`, `ELIGIBLE_RADIUS`,
//! `LOW_CLOCK_SECONDS`, `TRAP_MAX_PLY`, `W4_MAX_MATE`, `T_WIN`), read from `profile-core`.

use profile_core::stats::rng::Rng;
use profile_core::weakness::detectors::{
    DECISIVE_DW, ELIGIBLE_RADIUS, LOW_CLOCK_SECONDS, TRAP_MAX_PLY, W4_MAX_MATE,
};
use serde::{Deserialize, Serialize};

/// The win-percentage drop an injected blunder must cost: the detectors' decisive threshold
/// plus a margin, so search noise between two searches cannot turn it into a non-event.
pub const INJECTED_DROP: f64 = DECISIVE_DW + 5.0;
/// `T_WIN` of `observe_game`: the winning threshold of W5's episodes.
pub const WINNING_W: f64 = 85.0;
/// Consecutive own moves at [`WINNING_W`] or above (from ply 21) that make a winning episode.
pub const WINNING_RUN: u32 = 3;
/// A spoiling W5 player stops blundering once its win percentage is at or below this.
pub const SPOILED_W: f64 = 50.0;
/// Opening blunders are injected from this ply (the first moves are book-like for everyone).
pub const TRAP_MIN_PLY: u32 = 7;
/// Hangs are injected only after the opening window of W7.traps, and only on a calm clock, so
/// the hanging player's flaw does not also read as early blunders or low-clock blunders.
pub const HANG_MIN_PLY: u32 = TRAP_MAX_PLY + 1;
pub const HANG_MIN_CLOCK: f64 = 60.0;

/// What an own move looks like to an injector.
#[derive(Clone, Debug, PartialEq)]
pub struct MoveCtx {
    pub ply: u32,
    /// The mover's win percentage before the move (the best line).
    pub w_before: f64,
    /// `Some(n)` when the best line mates in `n` for the mover.
    pub mate_in: Option<i32>,
    /// The mover's clock before the move, in seconds.
    pub clock_before: Option<f64>,
    /// Consecutive own moves, this one included, at [`WINNING_W`] or above from ply 21.
    pub winning_run: u32,
}

impl MoveCtx {
    /// The detectors' eligible move: the mover's W within `ELIGIBLE_RADIUS` of 50.
    pub fn eligible(&self) -> bool {
        (self.w_before - 50.0).abs() < ELIGIBLE_RADIUS
    }
}

/// What an injector wants played.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Goal {
    /// A move that costs at least [`INJECTED_DROP`]; with `hang`, one whose refutation captures a
    /// knight, bishop, rook or queen and is not a mate (W1's "settled swing" of 2 or more pawns).
    Blunder { hang: bool },
    /// A move that does not keep the forced mate.
    AvoidMate,
}

/// Whether an injector's opportunity is a move or a game.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Unit {
    Move,
    Game,
}

/// Per-game state of an injector.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FlawGame {
    /// The per-game coin (game-unit injectors).
    pub fire: bool,
    /// This game's opportunity and arming were counted (game-unit injectors).
    pub counted_opportunity: bool,
    pub counted_armed: bool,
    pub counted_attempt: bool,
    /// The injection happened in this game (game-unit injectors).
    pub succeeded: bool,
    /// W5: the player is spoiling a won game.
    pub spoiling: bool,
}

/// Counts from which the true rate of the targeted weakness is computed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Tally {
    /// Opportunities of the targeted weakness, counted as its detector counts them.
    pub opportunities: u64,
    /// Opportunities where the injector was armed.
    pub armed: u64,
    /// Armed opportunities whose coin came up.
    pub attempts: u64,
    /// Attempts where the engine found a move that does what the injector wants.
    pub successes: u64,
}

impl Tally {
    pub fn add(&mut self, o: &Tally) {
        self.opportunities += o.opportunities;
        self.armed += o.armed;
        self.attempts += o.attempts;
        self.successes += o.successes;
    }

    /// The injected rate per opportunity: `p * (successes / attempts) * (armed / opportunities)`.
    /// The expected number of injected events is `p * armed * success_rate`; dividing by the
    /// opportunities gives the rate the detector should measure on top of the clean noise.
    /// `None` without opportunities. With no attempt yet the success rate counts as 1.
    pub fn injected_rate(&self, p: f64) -> Option<f64> {
        if self.opportunities == 0 {
            return None;
        }
        let success = if self.attempts == 0 {
            1.0
        } else {
            self.successes as f64 / self.attempts as f64
        };
        Some(p * success * self.armed as f64 / self.opportunities as f64)
    }

    /// The true rate the pipeline should estimate: the injected events on top of the clean
    /// player's own rate `base` for the same weakness, `1 - (1 - injected) * (1 - base)`.
    pub fn truth(&self, p: f64, base: f64) -> Option<f64> {
        let i = self.injected_rate(p)?;
        let b = base.clamp(0.0, 1.0);
        Some(1.0 - (1.0 - i) * (1.0 - b))
    }
}

/// One way of being worse at chess, at rate [`Flaw::rate`].
pub trait Flaw: Send + Sync + std::fmt::Debug {
    /// Short stable id, as written in the results (`hang`, `miss_mate`, ...).
    fn id(&self) -> &'static str;
    fn rate(&self) -> f64;
    /// The series key of the weakness the flaw is (`W1`, `W4.missed`, ...).
    fn target_key(&self) -> &'static str;
    /// Drill types (`ai::output::drills`) that train this flaw; the plan hits when the top
    /// weakness's drills include one of them.
    fn expected_drills(&self) -> &'static [&'static str];
    fn unit(&self) -> Unit;
    fn start_game(&self, rng: &mut Rng) -> FlawGame {
        FlawGame {
            fire: rng.next_f64() < self.rate(),
            ..FlawGame::default()
        }
    }
    /// This move is an opportunity of the targeted weakness (move-unit), or makes the game one
    /// (game-unit).
    fn opportunity(&self, ctx: &MoveCtx, game: &FlawGame) -> bool;
    /// The injector may fire here.
    fn armed(&self, ctx: &MoveCtx, game: &FlawGame) -> bool;
    fn goal(&self) -> Goal;
}

/// W1: leaves a piece hanging on a calm clock after the opening.
#[derive(Clone, Copy, Debug)]
pub struct Hang(pub f64);

impl Flaw for Hang {
    fn id(&self) -> &'static str {
        "hang"
    }
    fn rate(&self) -> f64 {
        self.0
    }
    fn target_key(&self) -> &'static str {
        "W1"
    }
    fn expected_drills(&self) -> &'static [&'static str] {
        &["own_mistake_puzzles", "safe_unsafe"]
    }
    fn unit(&self) -> Unit {
        Unit::Move
    }
    fn opportunity(&self, ctx: &MoveCtx, _: &FlawGame) -> bool {
        ctx.eligible()
    }
    fn armed(&self, ctx: &MoveCtx, _: &FlawGame) -> bool {
        ctx.eligible()
            && ctx.ply >= HANG_MIN_PLY
            && ctx.clock_before.is_none_or(|c| c >= HANG_MIN_CLOCK)
    }
    fn goal(&self) -> Goal {
        Goal::Blunder { hang: true }
    }
}

/// W4.missed: plays something else when it has a mate in 1 to 4.
#[derive(Clone, Copy, Debug)]
pub struct MissMate(pub f64);

impl Flaw for MissMate {
    fn id(&self) -> &'static str {
        "miss_mate"
    }
    fn rate(&self) -> f64 {
        self.0
    }
    fn target_key(&self) -> &'static str {
        "W4.missed"
    }
    fn expected_drills(&self) -> &'static [&'static str] {
        &["mate_patterns"]
    }
    fn unit(&self) -> Unit {
        Unit::Move
    }
    fn opportunity(&self, ctx: &MoveCtx, _: &FlawGame) -> bool {
        ctx.mate_in.is_some_and(|n| (1..=W4_MAX_MATE).contains(&n))
    }
    fn armed(&self, ctx: &MoveCtx, g: &FlawGame) -> bool {
        self.opportunity(ctx, g)
    }
    fn goal(&self) -> Goal {
        Goal::AvoidMate
    }
}

/// W7.traps: one decisive blunder in the opening (plies 7 to 20) in a share of games.
#[derive(Clone, Copy, Debug)]
pub struct OpeningBlunder(pub f64);

impl Flaw for OpeningBlunder {
    fn id(&self) -> &'static str {
        "opening_blunder"
    }
    fn rate(&self) -> f64 {
        self.0
    }
    fn target_key(&self) -> &'static str {
        "W7.traps"
    }
    fn expected_drills(&self) -> &'static [&'static str] {
        &["opening_trainer", "opening_trap_puzzles"]
    }
    fn unit(&self) -> Unit {
        Unit::Game
    }
    fn opportunity(&self, ctx: &MoveCtx, _: &FlawGame) -> bool {
        ctx.ply <= TRAP_MAX_PLY
    }
    fn armed(&self, ctx: &MoveCtx, g: &FlawGame) -> bool {
        !g.succeeded && ctx.ply >= TRAP_MIN_PLY && ctx.ply <= TRAP_MAX_PLY && ctx.eligible()
    }
    fn goal(&self) -> Goal {
        Goal::Blunder { hang: false }
    }
}

/// W9b: blunders when its clock is under 30 seconds.
#[derive(Clone, Copy, Debug)]
pub struct LowClockBlunder(pub f64);

impl Flaw for LowClockBlunder {
    fn id(&self) -> &'static str {
        "low_clock_blunder"
    }
    fn rate(&self) -> f64 {
        self.0
    }
    fn target_key(&self) -> &'static str {
        "W9b"
    }
    fn expected_drills(&self) -> &'static [&'static str] {
        &["blitz_safety", "clock_pressure"]
    }
    fn unit(&self) -> Unit {
        Unit::Move
    }
    fn opportunity(&self, ctx: &MoveCtx, _: &FlawGame) -> bool {
        ctx.eligible() && ctx.clock_before.is_some_and(|c| c < LOW_CLOCK_SECONDS)
    }
    fn armed(&self, ctx: &MoveCtx, g: &FlawGame) -> bool {
        self.opportunity(ctx, g) && ctx.ply > TRAP_MAX_PLY
    }
    fn goal(&self) -> Goal {
        Goal::Blunder { hang: false }
    }
}

/// W5: in a share of won games, throws the advantage away once the winning episode starts.
#[derive(Clone, Copy, Debug)]
pub struct NoConvert(pub f64);

impl Flaw for NoConvert {
    fn id(&self) -> &'static str {
        "no_convert"
    }
    fn rate(&self) -> f64 {
        self.0
    }
    fn target_key(&self) -> &'static str {
        "W5"
    }
    fn expected_drills(&self) -> &'static [&'static str] {
        &["convert_vs_engine"]
    }
    fn unit(&self) -> Unit {
        Unit::Game
    }
    fn opportunity(&self, ctx: &MoveCtx, _: &FlawGame) -> bool {
        ctx.winning_run >= WINNING_RUN
    }
    fn armed(&self, ctx: &MoveCtx, g: &FlawGame) -> bool {
        (g.spoiling || ctx.winning_run >= WINNING_RUN) && ctx.w_before > SPOILED_W
    }
    fn goal(&self) -> Goal {
        Goal::Blunder { hang: false }
    }
}

/// A flaw by id and rate (the suites and the CLI's names).
pub fn flaw(id: &str, rate: f64) -> Option<std::sync::Arc<dyn Flaw>> {
    use std::sync::Arc;
    Some(match id {
        "hang" => Arc::new(Hang(rate)),
        "miss_mate" => Arc::new(MissMate(rate)),
        "opening_blunder" => Arc::new(OpeningBlunder(rate)),
        "low_clock_blunder" => Arc::new(LowClockBlunder(rate)),
        "no_convert" => Arc::new(NoConvert(rate)),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(ply: u32, w: f64, clock: Option<f64>) -> MoveCtx {
        MoveCtx {
            ply,
            w_before: w,
            mate_in: None,
            clock_before: clock,
            winning_run: 0,
        }
    }

    #[test]
    fn injected_rate_scales_by_success_and_arming() {
        let t = Tally {
            opportunities: 200,
            armed: 100,
            attempts: 10,
            successes: 8,
        };
        let r = t.injected_rate(0.1).unwrap();
        assert!((r - 0.1 * 0.8 * 0.5).abs() < 1e-12, "{r}");
        assert_eq!(Tally::default().injected_rate(0.1), None);
        let none_tried = Tally {
            opportunities: 10,
            armed: 10,
            ..Tally::default()
        };
        assert_eq!(none_tried.injected_rate(0.3), Some(0.3));
    }

    #[test]
    fn truth_adds_the_clean_base_rate() {
        let t = Tally {
            opportunities: 10,
            armed: 10,
            attempts: 5,
            successes: 5,
        };
        let truth = t.truth(0.5, 0.1).unwrap();
        assert!((truth - (1.0 - 0.5 * 0.9)).abs() < 1e-12, "{truth}");
    }

    #[test]
    fn hang_is_armed_only_after_the_opening_on_a_calm_clock() {
        let h = Hang(0.1);
        let g = FlawGame::default();
        assert!(h.opportunity(&ctx(10, 50.0, Some(300.0)), &g));
        assert!(!h.armed(&ctx(10, 50.0, Some(300.0)), &g));
        assert!(h.armed(&ctx(30, 50.0, Some(300.0)), &g));
        assert!(!h.armed(&ctx(30, 50.0, Some(20.0)), &g));
        assert!(!h.opportunity(&ctx(30, 95.0, Some(300.0)), &g), "not eligible");
    }

    #[test]
    fn low_clock_needs_under_thirty_seconds() {
        let f = LowClockBlunder(0.2);
        let g = FlawGame::default();
        assert!(f.opportunity(&ctx(40, 50.0, Some(29.9)), &g));
        assert!(!f.opportunity(&ctx(40, 50.0, Some(30.0)), &g));
        assert!(!f.opportunity(&ctx(40, 50.0, None), &g));
    }

    #[test]
    fn miss_mate_counts_mates_up_to_four() {
        let f = MissMate(0.5);
        let g = FlawGame::default();
        let mut c = ctx(40, 100.0, None);
        c.mate_in = Some(4);
        assert!(f.armed(&c, &g));
        c.mate_in = Some(5);
        assert!(!f.opportunity(&c, &g));
    }

    #[test]
    fn opening_blunder_fires_once_in_the_window() {
        let f = OpeningBlunder(1.0);
        let mut g = f.start_game(&mut Rng::new(1));
        assert!(g.fire);
        assert!(!f.armed(&ctx(5, 50.0, None), &g));
        assert!(f.armed(&ctx(9, 50.0, None), &g));
        g.succeeded = true;
        assert!(!f.armed(&ctx(11, 50.0, None), &g));
        assert!(!f.armed(&ctx(21, 50.0, None), &OpeningBlunder(1.0).start_game(&mut Rng::new(1))));
    }

    #[test]
    fn flaws_by_id() {
        for id in [
            "hang",
            "miss_mate",
            "opening_blunder",
            "low_clock_blunder",
            "no_convert",
        ] {
            assert_eq!(flaw(id, 0.1).unwrap().id(), id);
        }
        assert!(flaw("nope", 0.1).is_none());
    }
}

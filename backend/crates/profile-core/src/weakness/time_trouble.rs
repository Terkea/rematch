//! Time trouble and time losses (R2 Task 7, rules TS M2 Task 6; improvement-engine section 6.2
//! W9a).
//!
//! The player is in time trouble when some own move has `clock_before < 10% of base` and either
//! its move number is below 40 or at least 10 own moves remain (that move included: the clock is
//! read before it is played). Games without clocks or without a base are unknown (`None`), never
//! "no trouble".

use chess_core::{Color, MoveRecord};

use super::episodes::Outcome;
use crate::types::{GameContext, PlatformId};

/// Fraction of the base time below which the clock is low.
pub const TROUBLE_FRACTION: f64 = 0.1;
/// A low clock before this move number is trouble whatever remains.
pub const TROUBLE_MOVE_LIMIT: u32 = 40;
/// From move 40 on, a low clock is trouble only with at least this many own moves remaining.
pub const TROUBLE_MIN_REMAINING: usize = 10;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TimeTrouble {
    /// `None` = no clocks, no base, or no valid own clock reading.
    pub in_trouble: Option<bool>,
    pub first_ply: Option<u32>,
}

/// [`detect_time_trouble`] with its skip count.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TimeTroubleScan {
    pub trouble: TimeTrouble,
    /// Own moves whose clock is present but not a finite value `>= 0`, or whose ply is 0.
    pub skipped_clocks: u32,
    /// Own moves without a clock reading (a first move may legitimately have none).
    pub missing_clocks: u32,
}

pub fn detect_time_trouble(
    moves: &[MoveRecord],
    player: Color,
    base_seconds: Option<u32>,
) -> TimeTrouble {
    detect_time_trouble_counted(moves, player, base_seconds).trouble
}

pub fn detect_time_trouble_counted(
    moves: &[MoveRecord],
    player: Color,
    base_seconds: Option<u32>,
) -> TimeTroubleScan {
    let mut own: Vec<&MoveRecord> = moves.iter().filter(|m| m.color == player).collect();
    // Move numbers, remaining moves and the first ply follow ply order, whatever the input order.
    own.sort_by_key(|m| m.ply);
    let valid = |m: &MoveRecord| -> Option<f64> {
        m.clock_before
            .filter(|c| c.is_finite() && *c >= 0.0 && m.ply > 0)
    };
    let skipped_clocks = own
        .iter()
        .filter(|m| m.clock_before.is_some() && valid(m).is_none())
        .count() as u32;
    let missing_clocks = own.iter().filter(|m| m.clock_before.is_none()).count() as u32;
    let unknown = TimeTroubleScan {
        trouble: TimeTrouble::default(),
        skipped_clocks,
        missing_clocks,
    };
    let Some(base) = base_seconds.filter(|b| *b > 0) else {
        return unknown;
    };
    if !own.iter().any(|m| valid(m).is_some()) {
        return unknown;
    }
    let limit = TROUBLE_FRACTION * f64::from(base);
    let first_ply = own.iter().enumerate().find_map(|(i, m)| {
        let clock = valid(m)?;
        let move_number = m.ply.div_ceil(2);
        let remaining = own.len() - i;
        (clock < limit && (move_number < TROUBLE_MOVE_LIMIT || remaining >= TROUBLE_MIN_REMAINING))
            .then_some(m.ply)
    });
    TimeTroubleScan {
        trouble: TimeTrouble {
            in_trouble: Some(first_ply.is_some()),
            first_ply,
        },
        skipped_clocks,
        missing_clocks,
    }
}

/// The game result from the player's side, read from `ctx.result`; `None` for `"*"` or anything
/// unparsed.
pub fn player_outcome(ctx: &GameContext) -> Option<Outcome> {
    let white = match ctx.result.trim() {
        "1-0" => Outcome::Won,
        "0-1" => Outcome::Lost,
        "1/2-1/2" => Outcome::Drawn,
        _ => return None,
    };
    Some(match (ctx.player_color, white) {
        (Color::White, o) | (Color::Black, o @ Outcome::Drawn) => o,
        (Color::Black, Outcome::Won) => Outcome::Lost,
        (Color::Black, Outcome::Lost) => Outcome::Won,
    })
}

fn termination(ctx: &GameContext) -> Option<String> {
    ctx.termination
        .as_deref()
        .map(|t| t.trim().to_ascii_lowercase())
}

/// The player lost on time: Lichess status `outoftime` or `timeout`, or a chess.com termination
/// containing "on time" (any case), and the player lost the game.
///
/// TS M2 says "the player not winning"; R2 requires a known loss, because a drawn time-out
/// (timeout vs insufficient material) does not say who flagged, and `*` says nothing.
pub fn is_time_loss(ctx: &GameContext) -> bool {
    let Some(t) = termination(ctx) else {
        return false;
    };
    let on_time = match ctx.platform {
        PlatformId::Lichess => t == "outoftime" || t == "timeout",
        PlatformId::Chesscom => t.contains("on time"),
    };
    on_time && player_outcome(ctx) == Some(Outcome::Lost)
}

/// The game ended by resignation: Lichess status `resign`, or a chess.com termination containing
/// "resignation" (any case). Either side may have resigned; see [`player_resigned`].
pub fn is_resignation(ctx: &GameContext) -> bool {
    let Some(t) = termination(ctx) else {
        return false;
    };
    match ctx.platform {
        PlatformId::Lichess => t == "resign",
        PlatformId::Chesscom => t.contains("resignation"),
    }
}

/// The player resigned: a resignation the player lost.
pub fn player_resigned(ctx: &GameContext) -> bool {
    is_resignation(ctx) && player_outcome(ctx) == Some(Outcome::Lost)
}

//! Winning and losing episodes (R2 Task 7, rules TS M2 Task 6; improvement-engine section 6.2
//! W5 and W6).
//!
//! A winning episode is the first run of at least `min_moves` consecutive own moves with
//! `win_before >= t_win`, counting only own moves at ply `>= min_ply`. A losing episode is the
//! first run of at least `min_moves` consecutive own moves with `win_before <= t_lose` (no ply
//! floor). The outcome comes from the player's score; with no valid score there is no episode,
//! because a conversion or a save cannot be judged without one.

use chess_core::{Color, MoveRecord};

/// A winning move drops out of the won zone when its `win_after` falls below this.
pub const DROP_WIN: f64 = 65.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EpisodeKind {
    Winning,
    Losing,
}

/// Game result from the player's point of view.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Outcome {
    Won,
    Drawn,
    Lost,
}

impl Outcome {
    /// `1 -> Won`, `0.5 -> Drawn`, `0 -> Lost`; anything else (including NaN) is `None`.
    pub fn from_score(score: f64) -> Option<Outcome> {
        if score == 1.0 {
            Some(Outcome::Won)
        } else if score == 0.5 {
            Some(Outcome::Drawn)
        } else if score == 0.0 {
            Some(Outcome::Lost)
        } else {
            None
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Episode {
    pub kind: EpisodeKind,
    /// First own ply of the qualifying run.
    pub start_ply: u32,
    /// Last own ply of the qualifying run.
    pub end_ply: u32,
    pub outcome: Outcome,
    /// Winning episodes only: the first own ply from `start_ply` on with `win_after < 65`.
    pub drop_ply: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EpisodeOpts {
    pub t_win: f64,
    pub t_lose: f64,
    pub min_moves: u32,
    pub min_ply: u32,
}

impl Default for EpisodeOpts {
    fn default() -> Self {
        EpisodeOpts {
            t_win: 85.0,
            t_lose: 15.0,
            min_moves: 3,
            min_ply: 21,
        }
    }
}

/// [`extract_episodes`] with its skip counts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EpisodeScan {
    pub winning: Option<Episode>,
    pub losing: Option<Episode>,
    /// Own moves with a non-finite `win_before` or `win_after`. A non-finite `win_before` breaks
    /// a run; a non-finite `win_after` is never a drop.
    pub skipped_moves: u32,
    /// The score was missing or not 0, 0.5 or 1, so no episode was reported.
    pub invalid_score: bool,
}

/// `(winning, losing)` episodes of `player` in `moves` (both colours, in ply order).
pub fn extract_episodes(
    moves: &[MoveRecord],
    player: Color,
    score: Option<f64>,
    o: &EpisodeOpts,
) -> (Option<Episode>, Option<Episode>) {
    let scan = extract_episodes_counted(moves, player, score, o);
    (scan.winning, scan.losing)
}

/// [`extract_episodes`], also reporting skipped inputs.
pub fn extract_episodes_counted(
    moves: &[MoveRecord],
    player: Color,
    score: Option<f64>,
    o: &EpisodeOpts,
) -> EpisodeScan {
    let mut own: Vec<&MoveRecord> = moves.iter().filter(|m| m.color == player).collect();
    // Runs and the drop search follow ply order, whatever the input order.
    own.sort_by_key(|m| m.ply);
    let skipped_moves = own
        .iter()
        .filter(|m| !m.win_before.is_finite() || !m.win_after.is_finite())
        .count() as u32;
    let outcome = score.and_then(Outcome::from_score);
    let Some(outcome) = outcome else {
        return EpisodeScan {
            winning: None,
            losing: None,
            skipped_moves,
            invalid_score: true,
        };
    };
    let min_moves = o.min_moves.max(1) as usize;

    // Only own moves at ply >= min_ply count, so a run that starts earlier begins at min_ply.
    let winning = first_run(&own, min_moves, |m| {
        m.ply >= o.min_ply && m.win_before.is_finite() && m.win_before >= o.t_win
    })
    .map(|(start, end)| {
        let start_ply = own[start].ply;
        let drop_ply = own[start..]
            .iter()
            .find(|m| m.win_after.is_finite() && m.win_after < DROP_WIN)
            .map(|m| m.ply);
        Episode {
            kind: EpisodeKind::Winning,
            start_ply,
            end_ply: own[end].ply,
            outcome,
            drop_ply,
        }
    });

    let losing = first_run(&own, min_moves, |m| {
        m.win_before.is_finite() && m.win_before <= o.t_lose
    })
    .map(|(start, end)| Episode {
        kind: EpisodeKind::Losing,
        start_ply: own[start].ply,
        end_ply: own[end].ply,
        outcome,
        drop_ply: None,
    });

    EpisodeScan {
        winning,
        losing,
        skipped_moves,
        invalid_score: false,
    }
}

/// Index range `(start, end)` (inclusive) of the first maximal run of consecutive elements that
/// satisfy `pred` and is at least `min_len` long.
fn first_run(
    own: &[&MoveRecord],
    min_len: usize,
    pred: impl Fn(&MoveRecord) -> bool,
) -> Option<(usize, usize)> {
    let mut i = 0;
    while i < own.len() {
        if !pred(own[i]) {
            i += 1;
            continue;
        }
        let start = i;
        while i < own.len() && pred(own[i]) {
            i += 1;
        }
        if i - start >= min_len {
            return Some((start, i - 1));
        }
    }
    None
}

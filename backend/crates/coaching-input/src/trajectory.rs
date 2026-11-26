//! Whether the trajectory's rating verdict reaches a player or the coach (#136, ADR 0116).
//!
//! EXP-0015 ran 700 simulated players whose strength never changed through the production
//! trajectory. It told 44%, 62% and 78% of them that their rating was falling at 60, 120 and
//! 250 rated games, against a bound of 15%, and 44% and 30% that it was rising at 60 and 120
//! games. A rating carries over from game to game (lag-1 correlation 0.97), so the bootstrap of
//! 30 consecutive ratings is six to thirteen times too narrow, and the "then" window sits on a
//! selected peak. Its decision is to hide the verdict until a rule clears it (EXP-0030).
//!
//! The rule still runs: `profile::coaching::trajectory::compute` and `contract` compute `kind`,
//! the rating row's verdict and its intervals, and `cli lab trajectory` returns them, so the
//! arena and EXP-0030 keep measuring them. [`player_visible`] cuts them at the edges: the
//! CoachingInput the profile builds, and every stored input the coach reads. The accuracy,
//! blunders, score and weakness rows keep their verdicts (EXP-0015 found accuracy and blunders
//! within the bound). The peak, the current rating, the change and each window's mean rating
//! stay, as plain numbers.
//!
//! The switch is an algorithm component (`profile.trajectory_rating_verdict` in the manifest),
//! so turning it back on is one line here, `tests/trajectory.rs` moving to the experiment that
//! cleared it, and a major version. The web reads the same value from `algorithm/manifest.json`.

use crate::types::{ChangeVerdict, CoachingInput, Trajectory};

/// The experiment whose decision [`RATING_VERDICT_SHOWN`] is.
pub const RATING_VERDICT_SOURCE: &str = "EXP-0015";

/// Whether the trajectory's rating verdict (`kind`, `thenVsNow.rating.verdict` and its
/// intervals) is stated to a player or the coach. EXP-0015: hide it until fixed.
pub const RATING_VERDICT_SHOWN: bool = false;

/// The trajectory as a player and the coach may read it. With [`RATING_VERDICT_SHOWN`] false:
/// no `kind`; the rating row's verdict is `withheld` and its intervals (each window's and the
/// difference's) are null, since they are the rule's evidence and as narrow as its verdict is
/// wrong. Its means, its n and its difference stay. A rating row below the minimum keeps
/// `not_enough_games`, which says nothing about a change.
pub fn player_visible(mut t: Trajectory) -> Trajectory {
    if RATING_VERDICT_SHOWN {
        return t;
    }
    t.kind = None;
    if let Some(v) = t.then_vs_now.as_mut() {
        let r = &mut v.rating;
        if r.verdict != ChangeVerdict::NotEnoughGames {
            r.verdict = ChangeVerdict::Withheld;
        }
        r.diff_low = None;
        r.diff_high = None;
        for side in [&mut r.then, &mut r.now] {
            side.low = None;
            side.high = None;
        }
    }
    t
}

/// [`player_visible`] on an input's trajectory: for every CoachingInput the coach reads,
/// including one stored before `algo-5.0.0`.
pub fn player_visible_input(mut i: CoachingInput) -> CoachingInput {
    i.trajectory = i.trajectory.map(player_visible);
    i
}

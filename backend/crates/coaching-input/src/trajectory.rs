//! Whether the trajectory's rating verdict reaches a player or the coach (#136, ADR 0116; EXP-0030,
//! ADR 0117).
//!
//! EXP-0015 ran 700 simulated players whose strength never changed through the trajectory of
//! `algo-4.0.1`. It told 44%, 62% and 78% of them that their rating was falling at 60, 120 and
//! 250 rated games, against a bound of 15%: a rating carries over from game to game, so the
//! bootstrap of 30 consecutive ratings was six to thirteen times too narrow, and the "then"
//! window sat on a selected peak. `algo-5.0.0` hid the verdict (ADR 0116).
//!
//! EXP-0030 registered a rule that counts the rating's memory and compares the first games with
//! the last, `memory-halves` (`profile::coaching::trajectory::RatingRule`). On 2,000 fresh
//! constant-strength players it said "falling" or "rising" to 0.08 to 0.12 of them in each of
//! six cells, within the bound, and it passed its guards (detection of a 200-point fall, a world
//! with breaks and streaks, a chess.com-like world, real Lichess series). Since `algo-6.0.0` the
//! verdict is stated again, from that rule only.
//!
//! [`player_visible`] still cuts at the edges: a trajectory that does not name the cleared rule
//! (`ratingRule`), which is every input built before `algo-6.0.0`, has its verdict withheld as
//! ADR 0116 did. The switch is an algorithm component (`profile.trajectory_rating_verdict` in
//! the manifest); the web reads the same value from `algorithm/manifest.json`.

use crate::types::{ChangeVerdict, CoachingInput, Trajectory};

/// The experiment whose decision [`RATING_VERDICT_SHOWN`] is.
pub const RATING_VERDICT_SOURCE: &str = "EXP-0030";

/// Whether the trajectory's rating verdict (`kind`, `thenVsNow.rating.verdict` and its
/// intervals) is stated to a player or the coach. EXP-0030: shown, from [`CLEARED_RULE`] only.
pub const RATING_VERDICT_SHOWN: bool = true;

/// The rule whose verdict may be stated (`RatingRule::MemoryHalves`'s name).
pub const CLEARED_RULE: &str = "memory-halves";

/// Whether a trajectory's rating verdict may be stated as it is.
fn stated(t: &Trajectory) -> bool {
    RATING_VERDICT_SHOWN && t.rating_rule.as_deref() == Some(CLEARED_RULE)
}

/// The trajectory as a player and the coach may read it. A trajectory from the cleared rule
/// passes unchanged. Any other (an input built before `algo-6.0.0`): no `kind`; the rating row's
/// verdict is `withheld` and its intervals (each window's and the difference's) are null, since
/// they are the old rule's evidence and as narrow as its verdict was wrong. Its means, its n and
/// its difference stay. A rating row below the minimum keeps `not_enough_games`, which says
/// nothing about a change.
pub fn player_visible(mut t: Trajectory) -> Trajectory {
    if stated(&t) {
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
/// including one stored before `algo-6.0.0`.
pub fn player_visible_input(mut i: CoachingInput) -> CoachingInput {
    i.trajectory = i.trajectory.map(player_visible);
    i
}

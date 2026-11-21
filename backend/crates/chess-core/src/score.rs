//! Lichess win-percentage maths (improvement-engine section 4.1).

use crate::EngineScore;

const WIN_K: f64 = 0.00368208;
const CP_LIMIT: i32 = 1000;

/// Centipawns clamped to +-1000; any mate is +-1000 (`Mate(0)` maps to -1000, as in TS).
pub fn score_to_cp(s: EngineScore) -> i32 {
    match s {
        EngineScore::Mate(m) => {
            if m > 0 {
                CP_LIMIT
            } else {
                -CP_LIMIT
            }
        }
        EngineScore::Cp(cp) => cp.clamp(-CP_LIMIT, CP_LIMIT),
    }
}

pub fn flip_score(s: EngineScore) -> EngineScore {
    match s {
        EngineScore::Mate(m) => EngineScore::Mate(-m),
        EngineScore::Cp(cp) => EngineScore::Cp(-cp),
    }
}

/// Lichess winning chances in [-1, 1].
pub fn winning_chances(s: EngineScore) -> f64 {
    2.0 / (1.0 + (-WIN_K * f64::from(score_to_cp(s))).exp()) - 1.0
}

/// Lichess win percentage in [0, 100].
pub fn win_percent(s: EngineScore) -> f64 {
    50.0 + 50.0 * winning_chances(s)
}

/// Per-move accuracy from the mover's win% before and after the move.
pub fn move_accuracy(win_before: f64, win_after: f64) -> f64 {
    if win_after >= win_before {
        return 100.0;
    }
    let raw = 103.1668100711649 * (-0.04354415386753951 * (win_before - win_after)).exp()
        - 3.166924740191411
        + 1.0;
    raw.clamp(0.0, 100.0)
}

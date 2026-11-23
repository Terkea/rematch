//! Weakness constants (final in R2 Task 1).
//!
//! Values are exactly TS M2 Task 1 `constants.ts`: `N0` and `MIN_EVIDENCE` from the plan text,
//! `TRAINABILITY` verbatim from improvement-engine section 6.4 (W3, W7 and W8 sub-keys use their
//! own rows, W4.* the W4 row, W9a-d the W9 row, W12 all zero), areas and labels from section 6.2.

use chess_core::TimeClass;

use crate::types::{Area, Band, WeaknessKey};

use WeaknessKey::*;

/// Prior strength n0 of the band shrinkage, in opportunities (games for W9a, W9d and W12).
pub fn n0(k: WeaknessKey) -> f64 {
    match k {
        W1 => 300.0,
        W2 => 100.0,
        W3Shallow | W3Deep => 60.0,
        W4Missed | W4Allowed => 40.0,
        W5 | W6 => 30.0,
        W7Traps | W7Prep => 40.0,
        W8Basics | W8Advanced => 80.0,
        W9a => 30.0,
        W9b => 40.0,
        W9c => 60.0,
        W9d => 20.0,
        W10 => 600.0,
        W11 => 300.0,
        W12 => 60.0,
    }
}

/// Minimum opportunities before a weakness can be ranked; `n0(k)` where not listed.
pub fn min_evidence(k: WeaknessKey) -> f64 {
    match k {
        W1 | W2 | W11 => 1000.0,
        W3Shallow | W3Deep => 40.0,
        W5 | W6 => 15.0,
        W8Basics | W8Advanced => 50.0,
        W9a => 20.0,
        W10 => 1500.0,
        other => n0(other),
    }
}

/// Trainability rows per band B0..B5 (improvement-engine section 6.4).
const fn trainability_row(k: WeaknessKey) -> [f64; 6] {
    match k {
        W1 | W2 => [1.0, 1.0, 0.9, 0.8, 0.7, 0.6],
        W3Shallow => [0.8, 1.0, 0.9, 0.8, 0.6, 0.5],
        W3Deep => [0.2, 0.4, 0.7, 0.8, 0.8, 0.7],
        W4Missed | W4Allowed => [1.0, 0.9, 0.7, 0.5, 0.4, 0.3],
        W5 => [0.6, 0.8, 0.9, 0.9, 0.8, 0.7],
        W6 => [0.2, 0.4, 0.6, 0.7, 0.7, 0.7],
        W7Traps => [0.7, 0.7, 0.5, 0.3, 0.2, 0.2],
        W7Prep => [0.1, 0.2, 0.4, 0.7, 0.9, 1.0],
        W8Basics => [0.6, 0.8, 0.9, 0.8, 0.6, 0.5],
        W8Advanced => [0.0, 0.2, 0.4, 0.7, 0.9, 1.0],
        W9a | W9b | W9c | W9d => [0.5, 0.8, 0.9, 0.9, 0.8, 0.7],
        W10 => [0.0, 0.1, 0.3, 0.6, 0.8, 0.9],
        W11 => [1.0, 1.0, 0.8, 0.6, 0.5, 0.4],
        // Advice only (session caps, break rules); not a drill target.
        W12 => [0.0; 6],
    }
}

/// Trainability of a weakness at a band, 0..1.
pub fn trainability(k: WeaknessKey, band: Band) -> f64 {
    let i = match band {
        Band::B0 => 0,
        Band::B1 => 1,
        Band::B2 => 2,
        Band::B3 => 3,
        Band::B4 => 4,
        Band::B5 => 5,
    };
    trainability_row(k)[i]
}

/// Keys that can be ranked: every key except the continuous W9d and W12.
pub const RANKABLE: [WeaknessKey; 17] = [
    W1, W2, W3Shallow, W3Deep, W4Missed, W4Allowed, W5, W6, W7Traps, W7Prep, W8Basics, W8Advanced,
    W9a, W9b, W9c, W10, W11,
];

/// EPL attribution precedence when one move matches several weaknesses (section 6.2 notes:
/// W4 > W2 > W1 > W3 > W8 > W7 > W10).
pub const EPL_PRECEDENCE: [WeaknessKey; 11] = [
    W4Missed, W4Allowed, W2, W1, W3Shallow, W3Deep, W8Basics, W8Advanced, W7Traps, W7Prep, W10,
];

/// The coaching area a weakness belongs to.
pub fn weakness_area(k: WeaknessKey) -> Area {
    match k {
        W1 | W2 | W4Allowed | W11 => Area::Calculation,
        W3Shallow | W3Deep | W4Missed => Area::Tactics,
        W5 | W6 | W10 => Area::Middlegame,
        W7Traps | W7Prep => Area::Openings,
        W8Basics | W8Advanced => Area::Endgame,
        W9a | W9b | W9c | W9d => Area::Time,
        W12 => Area::Mindset,
    }
}

/// English label (improvement-engine section 6.2).
pub fn weakness_label(k: WeaknessKey) -> &'static str {
    match k {
        W1 => "Leaving pieces hanging",
        W2 => "Missing your opponent's threat",
        W3Shallow => "Missing a short tactic",
        W3Deep => "Missing a longer tactic",
        W4Missed => "Missing a mate",
        W4Allowed => "Allowing a mate",
        W5 => "Not winning won games",
        W6 => "Defending lost positions",
        W7Traps => "Early blunders",
        W7Prep => "Leaving the opening worse",
        W8Basics => "Basic endgames",
        W8Advanced => "Harder endgames",
        W9a => "Getting short of time",
        W9b => "Blunders on a low clock",
        W9c => "Rushing important moves",
        W9d => "How you spend your time",
        W10 => "Drifting in quiet positions",
        W11 => "Mistakes on easy moves",
        W12 => "Tilt and tiredness",
    }
}

/// chess.com to Lichess rating offset per time class (provisional, TS M2 / ADR-R2-1).
pub const CHESSCOM_TO_LICHESS_OFFSET: [(TimeClass, i32); 5] = [
    (TimeClass::Bullet, 250),
    (TimeClass::Blitz, 250),
    (TimeClass::Rapid, 350),
    (TimeClass::Classical, 300),
    (TimeClass::Daily, 300),
];

/// The [`CHESSCOM_TO_LICHESS_OFFSET`] of a time class.
pub fn chesscom_to_lichess_offset(tc: TimeClass) -> i32 {
    match tc {
        TimeClass::Bullet | TimeClass::Blitz => 250,
        TimeClass::Rapid => 350,
        TimeClass::Classical | TimeClass::Daily => 300,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_are_consistent() {
        for (tc, off) in CHESSCOM_TO_LICHESS_OFFSET {
            assert_eq!(chesscom_to_lichess_offset(tc), off);
        }
        for k in WeaknessKey::ALL {
            assert_eq!(RANKABLE.contains(&k), k.is_binomial());
            assert!(min_evidence(k) >= 15.0);
            for b in Band::ALL {
                assert!((0.0..=1.0).contains(&trainability(k, b)));
            }
        }
        assert_eq!(min_evidence(W7Traps), 40.0);
        assert_eq!(min_evidence(W9d), 20.0);
    }
}

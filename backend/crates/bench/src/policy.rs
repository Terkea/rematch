//! The clean player: how every synthetic player picks a move when no flaw fires.
//!
//! The player picks among the engine's top lines, never one that loses [`BASE_MAX_LOSS`] or more
//! win-percentage points against the best line, with weights `exp(-loss / BASE_TEMPERATURE)`. A
//! forced mate on the board is always played. A clean player therefore makes inaccuracies
//! (`INACCURACY_DW` is 5) but no mistakes or blunders (`DECISIVE_DW` is 15), so every decisive
//! error in a flawed player's games comes from its injector, up to the search noise between two
//! node-limited searches (the clean control measures that noise).

use chess_core::EngineScore;
use profile_core::stats::rng::Rng;

/// A candidate move loses less than this many win-percentage points, or is not played.
pub const BASE_MAX_LOSS: f64 = 8.0;
/// Softmax temperature over the loss, in win-percentage points.
pub const BASE_TEMPERATURE: f64 = 2.5;

/// One engine line from the mover's side.
#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    pub uci: String,
    /// The mover's win percentage after this line.
    pub w: f64,
    /// The line mates for the mover (`Mate(n)` with n > 0).
    pub mates: bool,
}

impl Candidate {
    pub fn new(uci: String, score: EngineScore) -> Candidate {
        Candidate {
            uci,
            w: chess_core::score::win_percent(score),
            mates: matches!(score, EngineScore::Mate(n) if n > 0),
        }
    }
}

/// The index of the clean player's move among `cands` (best line first), from one uniform draw
/// `u` in `[0, 1)`. `None` only when `cands` is empty.
pub fn base_choice(cands: &[Candidate], u: f64) -> Option<usize> {
    let best = cands.first()?;
    if best.mates {
        return Some(0);
    }
    let weights: Vec<f64> = cands
        .iter()
        .map(|c| {
            let loss = (best.w - c.w).max(0.0);
            if loss < BASE_MAX_LOSS {
                (-loss / BASE_TEMPERATURE).exp()
            } else {
                0.0
            }
        })
        .collect();
    let total: f64 = weights.iter().sum();
    let mut acc = 0.0;
    let target = u.clamp(0.0, 1.0) * total;
    for (i, w) in weights.iter().enumerate() {
        acc += w;
        if *w > 0.0 && target < acc {
            return Some(i);
        }
    }
    Some(0)
}

/// A uniform draw in `[lo, hi)`.
pub fn uniform(rng: &mut Rng, lo: f64, hi: f64) -> f64 {
    lo + (hi - lo) * rng.next_f64()
}

/// Fisher-Yates shuffle with the seeded generator.
pub fn shuffle<T>(rng: &mut Rng, v: &mut [T]) {
    for i in (1..v.len()).rev() {
        let j = (rng.next_f64() * (i + 1) as f64) as usize;
        v.swap(i, j.min(i));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(uci: &str, w: f64) -> Candidate {
        Candidate {
            uci: uci.to_owned(),
            w,
            mates: false,
        }
    }

    #[test]
    fn a_forced_mate_is_always_played() {
        let mut mate = c("d1h5", 100.0);
        mate.mates = true;
        let cands = [mate, c("a2a3", 99.0)];
        for u in [0.0, 0.5, 0.999] {
            assert_eq!(base_choice(&cands, u), Some(0));
        }
    }

    #[test]
    fn a_move_losing_eight_points_or_more_is_never_played() {
        let cands = [c("e2e4", 60.0), c("d2d4", 59.0), c("g2g4", 52.0)];
        for i in 0..1000 {
            let u = f64::from(i) / 1000.0;
            assert_ne!(base_choice(&cands, u), Some(2), "u = {u}");
        }
    }

    #[test]
    fn close_moves_are_both_played() {
        let cands = [c("e2e4", 60.0), c("d2d4", 59.5)];
        assert_eq!(base_choice(&cands, 0.1), Some(0));
        assert_eq!(base_choice(&cands, 0.9), Some(1));
        assert_eq!(base_choice(&[], 0.5), None);
    }

    #[test]
    fn shuffle_is_seeded() {
        let mut a: Vec<u32> = (0..20).collect();
        let mut b = a.clone();
        shuffle(&mut Rng::new(7), &mut a);
        shuffle(&mut Rng::new(7), &mut b);
        assert_eq!(a, b);
        assert_ne!(a, (0..20).collect::<Vec<_>>());
    }
}

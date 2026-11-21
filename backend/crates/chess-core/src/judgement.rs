use crate::score::{score_to_cp, winning_chances};
use crate::{EngineScore, Judgement};

/// Judge a move. Both scores are from the mover's point of view.
pub fn judge(before: EngineScore, after: EngineScore) -> Option<Judgement> {
    // Mate created against the mover.
    if let (EngineScore::Cp(_), EngineScore::Mate(m)) = (before, after)
        && m < 0
    {
        let cp = score_to_cp(before);
        return Some(if cp < -999 {
            Judgement::Inaccuracy
        } else if cp < -700 {
            Judgement::Mistake
        } else {
            Judgement::Blunder
        });
    }
    // Mate lost by the mover.
    if let EngineScore::Mate(b) = before
        && b > 0
        && !matches!(after, EngineScore::Mate(a) if a > 0)
    {
        let cp = score_to_cp(after);
        return Some(if cp > 999 {
            Judgement::Inaccuracy
        } else if cp > 700 {
            Judgement::Mistake
        } else {
            Judgement::Blunder
        });
    }
    let drop = winning_chances(before) - winning_chances(after);
    if drop >= 0.3 {
        Some(Judgement::Blunder)
    } else if drop >= 0.2 {
        Some(Judgement::Mistake)
    } else if drop >= 0.1 {
        Some(Judgement::Inaccuracy)
    } else {
        None
    }
}

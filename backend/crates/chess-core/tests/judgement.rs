use chess_core::judgement::judge;
use chess_core::{EngineScore::*, Judgement};

#[test]
fn uses_winning_chances_drop_thresholds() {
    assert_eq!(judge(Cp(0), Cp(-200)), Some(Judgement::Blunder));
    assert_eq!(judge(Cp(0), Cp(-120)), Some(Judgement::Mistake));
    assert_eq!(judge(Cp(0), Cp(-60)), Some(Judgement::Inaccuracy));
    assert_eq!(judge(Cp(0), Cp(-40)), None);
}

#[test]
fn applies_the_mate_created_rules() {
    assert_eq!(judge(Cp(-50), Mate(-2)), Some(Judgement::Blunder));
    assert_eq!(judge(Cp(-800), Mate(-2)), Some(Judgement::Mistake));
    assert_eq!(judge(Cp(-1000), Mate(-2)), Some(Judgement::Inaccuracy));
}

#[test]
fn applies_the_mate_lost_rules() {
    assert_eq!(judge(Mate(2), Cp(50)), Some(Judgement::Blunder));
    assert_eq!(judge(Mate(2), Cp(800)), Some(Judgement::Mistake));
    assert_eq!(judge(Mate(2), Cp(1000)), Some(Judgement::Inaccuracy));
    assert_eq!(judge(Mate(3), Mate(2)), None);
}

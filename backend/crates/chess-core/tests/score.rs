use chess_core::EngineScore;
use chess_core::score::*;

fn close(a: f64, x: f64, d: i32) -> bool {
    (a - x).abs() < 0.5 * 10f64.powi(-d)
}

#[test]
fn computes_win_percent() {
    assert_eq!(win_percent(EngineScore::Cp(0)), 50.0);
    assert!(close(win_percent(EngineScore::Cp(100)), 59.1, 1));
    assert!(close(win_percent(EngineScore::Cp(-100)), 40.9, 1));
    assert!(close(
        win_percent(EngineScore::Cp(2000)),
        win_percent(EngineScore::Cp(1000)),
        10
    ));
    assert!(close(win_percent(EngineScore::Mate(3)), 97.54, 1));
}

#[test]
fn computes_winning_chances_in_range() {
    assert_eq!(winning_chances(EngineScore::Cp(0)), 0.0);
    assert!(close(
        winning_chances(EngineScore::Mate(-1)),
        -winning_chances(EngineScore::Mate(1)),
        10
    ));
}

#[test]
fn clamps_cp_and_maps_mate_to_1000() {
    assert_eq!(score_to_cp(EngineScore::Cp(5000)), 1000);
    assert_eq!(score_to_cp(EngineScore::Cp(-5000)), -1000);
    assert_eq!(score_to_cp(EngineScore::Cp(37)), 37);
    assert_eq!(score_to_cp(EngineScore::Mate(4)), 1000);
    assert_eq!(score_to_cp(EngineScore::Mate(-4)), -1000);
}

#[test]
fn mate_zero_maps_to_minus_1000() {
    assert_eq!(score_to_cp(EngineScore::Mate(0)), -1000);
}

#[test]
fn flips_the_point_of_view() {
    assert_eq!(flip_score(EngineScore::Cp(30)), EngineScore::Cp(-30));
    assert_eq!(flip_score(EngineScore::Mate(2)), EngineScore::Mate(-2));
}

#[test]
fn computes_move_accuracy() {
    assert_eq!(move_accuracy(50.0, 50.0), 100.0);
    assert_eq!(move_accuracy(50.0, 60.0), 100.0);
    assert!(close(move_accuracy(60.0, 50.0), 64.58, 1));
    assert_eq!(move_accuracy(100.0, 0.0), 0.0);
}

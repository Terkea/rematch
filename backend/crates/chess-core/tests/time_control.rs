use chess_core::TimeClass::*;
use chess_core::fen::fen_key;
use chess_core::time_control::classify_time_control;

#[test]
fn classifies_by_estimated_duration_base_plus_40_increment() {
    assert_eq!(classify_time_control(60, 0), Bullet);
    assert_eq!(classify_time_control(180, 0), Blitz);
    assert_eq!(classify_time_control(180, 2), Blitz);
    assert_eq!(classify_time_control(300, 3), Blitz);
    assert_eq!(classify_time_control(480, 0), Rapid);
    assert_eq!(classify_time_control(600, 0), Rapid);
    assert_eq!(classify_time_control(900, 10), Rapid);
    assert_eq!(classify_time_control(1800, 0), Classical);
}

#[test]
fn uses_exclusive_upper_bounds() {
    assert_eq!(classify_time_control(179, 0), Bullet);
    assert_eq!(classify_time_control(479, 0), Blitz);
    assert_eq!(classify_time_control(1499, 0), Rapid);
    assert_eq!(classify_time_control(1500, 0), Classical);
}

#[test]
fn returns_the_first_four_fen_fields() {
    assert_eq!(
        fen_key("rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1"),
        "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq -"
    );
}

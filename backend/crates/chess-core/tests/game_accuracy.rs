use chess_core::Color;
use chess_core::game_accuracy::game_accuracy;

#[test]
fn is_100_when_the_evaluation_never_moves() {
    let series = vec![50.0; 21];
    assert_eq!(game_accuracy(&series, Color::White), 100.0);
    assert_eq!(game_accuracy(&series, Color::Black), 100.0);
}

#[test]
fn penalises_the_colour_that_made_the_big_drop() {
    let series = [
        50.0, 50.0, 50.0, 50.0, 50.0, 50.0, 90.0, 90.0, 90.0, 90.0, 90.0,
    ];
    let white = game_accuracy(&series, Color::White);
    let black = game_accuracy(&series, Color::Black);
    assert_eq!(white, 100.0);
    assert!(black < white);
    assert!(black > 0.0);
}

#[test]
fn returns_100_for_a_colour_with_no_moves() {
    assert_eq!(game_accuracy(&[50.0, 50.0], Color::Black), 100.0);
}

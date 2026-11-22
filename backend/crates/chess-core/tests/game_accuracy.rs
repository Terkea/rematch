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

// Audit regressions (A2, A4): real eval sequences, anonymised to white-POV centipawns, with the
// values an independent port of lila `AccuracyPercent.gameAccuracy` gives for them.
mod lila_examples {
    use chess_core::game_accuracy::{game_accuracy, game_accuracy_from};
    use chess_core::score::win_percent;
    use chess_core::{Color, EngineScore};
    use serde_json::Value;

    fn cases() -> Vec<Value> {
        let text = include_str!("../../../fixtures/accuracy/lila-examples.json");
        let v: Value = serde_json::from_str(text).unwrap();
        v["cases"].as_array().unwrap().clone()
    }

    fn case(name: &str) -> (Vec<f64>, f64, f64) {
        let c = cases()
            .into_iter()
            .find(|c| c["name"] == name)
            .expect("case");
        let wins = c["whiteCp"]
            .as_array()
            .unwrap()
            .iter()
            .map(|cp| win_percent(EngineScore::Cp(cp.as_i64().unwrap() as i32)))
            .collect();
        let lila = |k: &str| c["lila"][k].as_f64().unwrap();
        (wins, lila("white"), lila("black"))
    }

    fn close(got: f64, want: f64) {
        assert!((got - want).abs() < 1e-9, "got {got}, want {want}");
    }

    #[test]
    fn a_zero_accuracy_move_is_floored_at_1_in_the_harmonic_mean() {
        let (wins, white, black) = case("harmonic-floor");
        // Before the fix: 38.91 (the harmonic half was zeroed).
        close(game_accuracy(&wins, Color::Black), black);
        close(game_accuracy(&wins, Color::White), white);
    }

    #[test]
    fn the_window_is_sized_from_moves_not_positions() {
        let (wins, white, black) = case("window-from-moves");
        // Before the fix: 61.21 and 83.29 (window 3 from 30 positions).
        close(game_accuracy(&wins, Color::Black), black);
        close(game_accuracy(&wins, Color::White), white);
    }

    #[test]
    fn one_blundered_move_never_zeroes_the_harmonic_half() {
        // Ten perfect white moves and one at accuracy 0: the harmonic mean is
        // 11 / (10 / 100 + 1 / 1) = 10, not 0, so the game is 5 points above half the weighted
        // mean (14.71 before the fix).
        let mut wins = vec![50.0; 20];
        wins.extend([99.0, 1.0, 1.0]);
        close(game_accuracy(&wins, Color::White), 14.705882352941176 + 5.0);
    }

    #[test]
    fn from_a_black_start_index_0_is_a_black_move() {
        // Black drops 40 points at index 0 → 1; white then plays perfect moves.
        let wins = [50.0, 90.0, 90.0, 90.0, 90.0];
        let black = game_accuracy_from(&wins, Color::Black, Color::Black).unwrap();
        let white = game_accuracy_from(&wins, Color::Black, Color::White).unwrap();
        assert_eq!(white, 100.0);
        assert!(black < 50.0, "{black}");
        // From a white start the jump is a white gain.
        assert_eq!(game_accuracy_from(&wins, Color::White, Color::White), Some(100.0));
    }

    #[test]
    fn from_is_none_for_a_colour_without_moves() {
        assert_eq!(game_accuracy_from(&[50.0, 50.0], Color::White, Color::Black), None);
        assert_eq!(game_accuracy_from(&[50.0], Color::White, Color::White), None);
        assert_eq!(game_accuracy_from(&[], Color::White, Color::White), None);
    }

    #[test]
    fn from_a_white_start_matches_game_accuracy() {
        let (wins, _, black) = case("window-from-moves");
        close(game_accuracy_from(&wins, Color::White, Color::Black).unwrap(), black);
    }
}

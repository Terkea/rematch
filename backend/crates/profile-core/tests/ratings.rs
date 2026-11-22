//! Pre-game ratings for the rating-predicted score (audit A5, ADR-FIX-C-1).

use profile_core::types::PlatformId::{Chesscom, Lichess};
use profile_core::weakness::sessions::{PreGameInput, pre_game_ratings};

fn input(own: Option<i32>, opp: Option<i32>, previous: Option<i32>) -> PreGameInput {
    PreGameInput {
        rated: true,
        own_rating: own,
        opponent_rating: opp,
        previous_own_rating: previous,
    }
}

#[test]
fn chesscom_own_rating_is_the_previous_game_rating() {
    // Won 1500 -> 1510 against a 1600 who lost 10: both were rated before the game.
    let r = pre_game_ratings(Chesscom, input(Some(1510), Some(1590), Some(1500)));
    assert_eq!(r, (Some(1500), Some(1600)));
}

#[test]
fn chesscom_opponent_moves_by_the_mirrored_change() {
    // Lost 1500 -> 1492: the opponent's post-game rating is 8 above its pre-game one.
    let r = pre_game_ratings(Chesscom, input(Some(1492), Some(1408), Some(1500)));
    assert_eq!(r, (Some(1500), Some(1400)));
}

#[test]
fn chesscom_without_a_previous_game_keeps_the_post_game_ratings() {
    let r = pre_game_ratings(Chesscom, input(Some(1510), Some(1590), None));
    assert_eq!(r, (Some(1510), Some(1590)));
}

#[test]
fn chesscom_without_an_opponent_rating_has_none() {
    let r = pre_game_ratings(Chesscom, input(Some(1510), None, Some(1500)));
    assert_eq!(r, (Some(1500), None));
}

#[test]
fn chesscom_unrated_games_do_not_move_ratings() {
    let mut i = input(Some(1510), Some(1590), Some(1400));
    i.rated = false;
    assert_eq!(pre_game_ratings(Chesscom, i), (Some(1510), Some(1590)));
}

#[test]
fn lichess_ratings_are_already_pre_game() {
    // The Lichess API's `players.*.rating` is the rating at the start (`ratingDiff` is the change).
    let r = pre_game_ratings(Lichess, input(Some(1510), Some(1590), Some(1400)));
    assert_eq!(r, (Some(1510), Some(1590)));
}

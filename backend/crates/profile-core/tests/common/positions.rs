//! FEN/PV fixtures of TS M2 Task 3, verbatim (shared by R2 Tasks 4-6). Every fixture is White to
//! move; the expected swings are for pov White.

/// The standard starting position.
pub const START: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

/// `(name, fen, pv)`.
pub const FIXTURES: &[(&str, &str, &[&str])] = &[
    ("hanging", "4k3/8/8/3q4/8/8/8/3QK3 w - - 0 1", &["d1d5"]),
    (
        "fork",
        "r3k3/8/8/1N6/8/8/8/4K3 w - - 0 1",
        &["b5c7", "e8e7", "c7a8"],
    ),
    (
        "pin",
        "4k3/8/3p4/4n3/8/8/5P2/4R1K1 w - - 0 1",
        &["f2f4", "e8f7", "f4e5", "d6e5"],
    ),
    (
        "skewer",
        "4q3/8/8/4k3/8/8/8/R6K w - - 0 1",
        &["a1e1", "e5d6", "e1e8"],
    ),
    (
        "discovered",
        "7q/4k3/8/8/3N4/8/1B6/6K1 w - - 0 1",
        &["d4c6", "e7d6", "b2h8"],
    ),
    (
        "deflection",
        "1r4k1/1q3ppp/8/3Q4/8/8/5PPP/4R1K1 w - - 0 1",
        &["e1e8", "b8e8", "d5b7"],
    ),
    ("promotion", "8/4P3/8/8/8/k7/8/K7 w - - 0 1", &["e7e8q"]),
    (
        "backRank",
        "6k1/5ppp/8/8/8/8/5PPP/3R2K1 w - - 0 1",
        &["d1d8"],
    ),
    (
        "mateIn2",
        "r5k1/5ppp/8/8/8/8/3R1PPP/3R2K1 w - - 0 1",
        &["d2d8", "a8d8", "d1d8"],
    ),
    ("smothered", "6rk/6pp/8/6N1/8/8/8/6K1 w - - 0 1", &["g5f7"]),
    ("quiet", START, &["e2e4", "e7e5"]),
];

/// The FEN and PV of a named fixture.
pub fn fixture(name: &str) -> (&'static str, Vec<String>) {
    let (_, fen, pv) = FIXTURES
        .iter()
        .find(|(n, _, _)| *n == name)
        .unwrap_or_else(|| panic!("no fixture named {name}"));
    (fen, pv.iter().map(|m| (*m).to_owned()).collect())
}

/// Owned UCI moves, as the board functions take them.
pub fn line(moves: &[&str]) -> Vec<String> {
    moves.iter().map(|m| (*m).to_owned()).collect()
}

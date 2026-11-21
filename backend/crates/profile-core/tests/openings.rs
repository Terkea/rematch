//! R2 Task 16: opening classification from the CC0 lichess-org/chess-openings dataset.
//!
//! Ports TS M2 Task 12 (`classify.test.ts`) and adds the R-7 line-key bound, the dataset load
//! time, the duplicate rule and the table error cases.

use chess_core::Color;
use chess_core::board::Position;
use chess_core::fen::fen_key;
use profile_core::openings::classify::{
    Classified, LineKind, classify_opening, line_family, line_key, line_kind,
};
use profile_core::openings::dataset::{
    DatasetError, OpeningName, VENDORED_TSVS, build_openings, openings,
};
use profile_core::types::SeriesKey;
use sha2::{Digest, Sha256};
use std::time::{Duration, Instant};

const START_FEN: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

/// Replays SAN moves from the start and returns each ply's `(fen_after, mover)`.
fn plies(sans: &[&str]) -> Vec<(String, Color)> {
    let mut pos = Position::start();
    sans.iter()
        .map(|san| {
            let mover = pos.side_to_move();
            pos.play_san(san).unwrap_or_else(|e| panic!("{san}: {e}"));
            (pos.fen(), mover)
        })
        .collect()
}

fn classify(sans: &[&str]) -> Option<Classified> {
    classify_opening(&plies(sans))
}

fn hex6(s: &str) -> String {
    hex::encode(Sha256::digest(s.as_bytes()))[..6].to_string()
}

// --- TS M2 Task 12 ---------------------------------------------------------------------------

#[test]
fn caro_kann_advance_is_b12() {
    let c = classify(&["e4", "c6", "d4", "d5", "e5"]).expect("named");
    assert_eq!(c.eco, "B12");
    assert!(
        c.name.starts_with("Caro-Kann Defense: Advance Variation"),
        "{}",
        c.name
    );
    assert_eq!(c.ply, 5);
    assert_eq!(c.mover, Color::White);
}

#[test]
fn italian_game_is_c50() {
    let p = plies(&["e4", "e5", "Nf3", "Nc6", "Bc4"]);
    let c = classify_opening(&p).expect("named");
    assert_eq!(c.eco, "C50");
    assert_eq!(c.name, "Italian Game");
    assert_eq!(c.ply, 5);
    assert_eq!(c.mover, Color::White);
    assert_eq!(c.fen_key, fen_key(&p[4].0));
}

#[test]
fn transposition_gives_equal_results() {
    let a = classify(&["Nf3", "d5", "d4"]).expect("named");
    let b = classify(&["d4", "d5", "Nf3"]).expect("named");
    assert_eq!(a, b);
    assert_eq!(a.eco, "D02");
    assert_eq!(a.name, "Queen's Pawn Game: Zukertort Variation");
    assert_eq!(a.ply, 3);
}

#[test]
fn line_family_is_text_before_first_comma() {
    assert_eq!(
        line_family("Sicilian Defense: Najdorf Variation, English Attack"),
        "Sicilian Defense: Najdorf Variation"
    );
    assert_eq!(
        line_family("Queen's Indian Defense, with e3, Bb4+ Line"),
        "Queen's Indian Defense"
    );
    assert_eq!(line_family("Italian Game"), "Italian Game");
}

#[test]
fn line_kind_compares_entry_mover_and_player() {
    assert_eq!(line_kind(Color::Black, Color::White), LineKind::Faced);
    assert_eq!(line_kind(Color::White, Color::Black), LineKind::Faced);
    assert_eq!(line_kind(Color::White, Color::White), LineKind::Chosen);
    assert_eq!(line_kind(Color::Black, Color::Black), LineKind::Chosen);
}

#[test]
fn start_position_is_unclassified() {
    assert_eq!(classify_opening(&[]), None);
    assert_eq!(
        classify_opening(&[(START_FEN.to_string(), Color::White)]),
        None
    );
    assert!(!openings().contains_key(&fen_key(START_FEN)));
}

// --- Position-based classification ---------------------------------------------------------

#[test]
fn deepest_named_position_survives_leaving_the_book() {
    // 3...a6 4.c3 is in no table row, so the Italian Game (ply 5) stays the deepest name.
    let c = classify(&["e4", "e5", "Nf3", "Nc6", "Bc4", "a6", "c3"]).expect("named");
    assert_eq!(c.name, "Italian Game");
    assert_eq!(c.ply, 5);
}

#[test]
fn line_entered_by_black_reports_black_mover() {
    let c = classify(&["e4", "c6"]).expect("named");
    assert_eq!(
        (c.eco.as_str(), c.name.as_str()),
        ("B10", "Caro-Kann Defense")
    );
    assert_eq!((c.ply, c.mover), (2, Color::Black));
    assert_eq!(line_kind(c.mover, Color::White), LineKind::Faced);
}

#[test]
fn clocks_do_not_affect_classification() {
    let mut p = plies(&["e4", "e5", "Nf3", "Nc6", "Bc4"]);
    let fen = &mut p[4].0;
    let key = fen_key(fen);
    *fen = format!("{key} 17 42");
    assert_eq!(classify_opening(&p).expect("named").name, "Italian Game");
}

// --- R-7 line keys ---------------------------------------------------------------------------

#[test]
fn line_key_slugs_the_family() {
    assert_eq!(line_key(Color::White, "Italian Game"), "white:italian-game");
    assert_eq!(
        line_key(
            Color::Black,
            "Sicilian Defense: Najdorf Variation, English Attack"
        ),
        "black:sicilian-defense-najdorf-variation"
    );
    assert_eq!(
        line_key(Color::White, "Queen's Gambit Declined"),
        "white:queens-gambit-declined"
    );
    assert_eq!(
        line_key(Color::Black, "Grünfeld Defense: Kádas Gambit"),
        "black:grunfeld-defense-kadas-gambit"
    );
}

#[test]
fn line_key_fits_64_chars() {
    let name = "Hypothetical Opening: Extraordinarily Long Variation Name Used Only To Exercise Key Capped";
    assert_eq!(name.chars().count(), 90);
    let key = line_key(Color::White, name);
    assert!(key.len() <= 48, "{key} has {} chars", key.len());
    let (head, suffix) = key.rsplit_once('-').expect("hash suffix");
    assert_eq!(suffix.len(), 6, "{key}");
    assert!(
        suffix
            .chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
    );
    assert!(head.starts_with("white:hypothetical"), "{key}");
    let full_slug =
        "hypothetical-opening-extraordinarily-long-variation-name-used-only-to-exercise-key-capped";
    assert_eq!(
        key,
        format!("white:hypothetical-opening-{}", hex6(full_slug))
    );
    let series = format!("opening:{key}");
    assert!(series.len() <= 64);
    assert_eq!(
        series.parse::<SeriesKey>(),
        Ok(SeriesKey::Opening(key.clone()))
    );
    assert_eq!(line_key(Color::White, name), key, "stable");
    let sibling = name.replace("Capped", "Cut");
    assert_ne!(
        line_key(Color::White, &sibling),
        key,
        "hash separates names"
    );
}

#[test]
fn every_dataset_line_key_fits() {
    for o in openings().values() {
        for color in [Color::White, Color::Black] {
            let key = line_key(color, &o.name);
            let slug = key.split_once(':').expect("color prefix").1;
            assert!(key.len() <= 48, "{key}");
            assert!(!slug.is_empty() && !slug.starts_with('-') && !slug.ends_with('-'));
            assert!(
                slug.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
                "{key}"
            );
            assert!(!slug.contains("--"), "{key}");
        }
    }
}

// --- Dataset -----------------------------------------------------------------------------------

/// The load budget. An optimised build replays the tables in about 20 ms. Unoptimised test
/// builds run `Position::play_san` about 15 times slower (about 290 ms for the 8 801 distinct
/// moves), so a debug build gets 10 times the budget (still at least 3 times the measured time).
const LOAD_BUDGET: Duration = Duration::from_millis(200);
const DEBUG_BUILD_FACTOR: u32 = 10;

#[test]
fn dataset_loads_under_200ms() {
    let budget = if cfg!(debug_assertions) {
        LOAD_BUDGET * DEBUG_BUILD_FACTOR
    } else {
        LOAD_BUDGET
    };
    let t = Instant::now();
    let fresh = build_openings(&VENDORED_TSVS).expect("vendored tables replay");
    let took = t.elapsed();
    assert!(took < budget, "took {took:?}, budget {budget:?}");
    assert!(fresh.len() >= 3000, "{} entries", fresh.len());
    assert_eq!(openings().len(), fresh.len());
    assert!(std::ptr::eq(openings(), openings()), "built once");
}

#[test]
fn dataset_keys_are_epd_and_rows_are_well_formed() {
    for (key, o) in openings() {
        assert_eq!(key.split(' ').count(), 4, "{key}");
        let eco = o.eco.as_bytes();
        assert!(
            eco.len() == 3
                && (b'A'..=b'E').contains(&eco[0])
                && eco[1].is_ascii_digit()
                && eco[2].is_ascii_digit(),
            "{}",
            o.eco
        );
        assert!(!o.name.is_empty());
    }
}

#[test]
fn duplicates_keep_the_longest_name() {
    let rows = [
        "C40\tKing's Knight Opening\t1. e4 e5 2. Nf3",
        "A05\tZukertort Transposition: A Longer Name\t1. Nf3 e5 2. e4",
    ];
    let forward = format!("eco\tname\tpgn\n{}\n{}\n", rows[0], rows[1]);
    let backward = format!("eco\tname\tpgn\n{}\n{}\n", rows[1], rows[0]);
    for table in [forward, backward] {
        let map = build_openings(&[("x.tsv", table.as_str())]).expect("valid");
        assert_eq!(map.len(), 1);
        let only = map.values().next().expect("one entry");
        assert_eq!(
            only,
            &OpeningName {
                eco: "A05".into(),
                name: "Zukertort Transposition: A Longer Name".into()
            }
        );
    }
}

#[test]
fn bad_tables_are_rejected() {
    let header = build_openings(&[("x.tsv", "eco\tname\n")]);
    assert!(
        matches!(&header, Err(DatasetError::Header { file }) if file == "x.tsv"),
        "{header:?}"
    );
    let row = build_openings(&[("x.tsv", "eco\tname\tpgn\nA00\tAmar Opening\n")]);
    assert!(
        matches!(&row, Err(DatasetError::Row { file, line: 2 }) if file == "x.tsv"),
        "{row:?}"
    );
    let illegal = build_openings(&[("x.tsv", "eco\tname\tpgn\nA00\tBad\t1. e4 e5 2. Ke3\n")]);
    assert!(
        matches!(&illegal, Err(DatasetError::IllegalMove { file, line: 2, san }) if file == "x.tsv" && san == "Ke3"),
        "{illegal:?}"
    );
}

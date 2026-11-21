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
use std::collections::HashMap;

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
    assert_eq!((c.entry_ply, c.entry_mover), (2, Color::Black));
    assert_eq!(line_kind(c.entry_mover, Color::White), LineKind::Faced);
}

const NAJDORF: [&str; 10] = [
    "e4", "c5", "Nf3", "d6", "d4", "cxd4", "Nxd4", "Nf6", "Nc3", "a6",
];

fn najdorf_with(sixth: &str) -> Classified {
    let mut sans = NAJDORF.to_vec();
    sans.push(sixth);
    classify(&sans).expect("named")
}

#[test]
fn najdorf_english_attack_is_chosen_by_black_entered_at_5_a6() {
    let c = najdorf_with("Be3");
    assert_eq!(
        c.name,
        "Sicilian Defense: Najdorf Variation, English Attack"
    );
    assert_eq!((c.ply, c.mover), (11, Color::White));
    assert_eq!((c.entry_ply, c.entry_mover), (10, Color::Black));
    assert_eq!(line_kind(c.entry_mover, Color::Black), LineKind::Chosen);
}

#[test]
fn najdorf_6_bg5_is_also_chosen_by_black() {
    let c = najdorf_with("Bg5");
    assert_eq!(line_family(&c.name), "Sicilian Defense: Najdorf Variation");
    assert_eq!(c.ply, 11);
    assert_eq!((c.entry_ply, c.entry_mover), (10, Color::Black));
    assert_eq!(line_kind(c.entry_mover, Color::Black), LineKind::Chosen);
}

#[test]
fn white_facing_the_najdorf_is_faced() {
    for sixth in ["Be3", "Bg5"] {
        let c = najdorf_with(sixth);
        assert_eq!(
            line_kind(c.entry_mover, Color::White),
            LineKind::Faced,
            "{sixth}"
        );
    }
}

fn pgn_sans(pgn: &str) -> Vec<&str> {
    pgn.split_whitespace()
        .filter(|t| !t.ends_with('.'))
        .collect()
}

/// Along one move order, extending a line within its family never moves its entry: for every
/// row whose nearest shorter row (a pgn prefix) has the same family, both share `entry_ply` and
/// `entry_mover`. Across move orders the entry may differ: dozens of families are entered by White in
/// some rows and by Black in others (transpositions such as the Dragon), so `Chosen`/`Faced`
/// depends on the game's own move order, by design.
#[test]
fn entry_mover_is_stable_across_a_familys_continuations() {
    let mut rows: HashMap<String, Classified> = HashMap::new();
    let mut pgns: Vec<Vec<&str>> = Vec::new();
    for (_, table) in VENDORED_TSVS {
        for row in table.lines().skip(1).filter(|l| !l.trim().is_empty()) {
            let sans = pgn_sans(row.split('\t').nth(2).expect("pgn"));
            let c = classify(&sans).expect("every row is named");
            assert!(c.entry_ply <= c.ply, "{sans:?}");
            rows.insert(sans.join(" "), c);
            pgns.push(sans);
        }
    }
    let mut compared = 0;
    for sans in &pgns {
        let child = &rows[&sans.join(" ")];
        let parent = (1..sans.len())
            .rev()
            .find_map(|k| rows.get(&sans[..k].join(" ")));
        let Some(parent) = parent else { continue };
        if line_family(&parent.name) != line_family(&child.name) {
            continue;
        }
        compared += 1;
        assert_eq!(
            (child.entry_ply, child.entry_mover),
            (parent.entry_ply, parent.entry_mover),
            "{sans:?}"
        );
    }
    assert!(compared > 1000, "{compared} continuations compared");
}

#[test]
fn late_transposition_into_book() {
    let c = classify(&["d4", "c6", "e4", "d5", "e5"]).expect("named");
    assert_eq!(c.eco, "B12");
    assert!(c.name.starts_with("Caro-Kann Defense: Advance Variation"));
    assert_eq!(c, classify(&["e4", "c6", "d4", "d5", "e5"]).expect("named"));
}

#[test]
fn one_ply_game() {
    let c = classify(&["e4"]).expect("named");
    assert_eq!(
        (c.eco.as_str(), c.name.as_str()),
        ("B00", "King's Pawn Game")
    );
    assert_eq!(
        (c.ply, c.mover, c.entry_ply, c.entry_mover),
        (1, Color::White, 1, Color::White)
    );
}

#[test]
fn en_passant_square_is_normalised() {
    // After 1.e4 no capture on e3 is legal: a source that prints `e3` still matches.
    let printed = "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1";
    let c = classify_opening(&[(printed.to_string(), Color::White)]).expect("named");
    assert_eq!(c.name, "King's Pawn Game");
}

#[test]
fn legal_en_passant_square_is_part_of_the_key() {
    // Synthetic pair: after 1.e4 Nf6 2.e5 d5 the capture exd6 is legal, so the key keeps `d6`
    // and the same placement without the en-passant right is a different position.
    let table = "eco\tname\tpgn\nB02\tSynthetic: With En Passant\t1. e4 Nf6 2. e5 d5\n";
    let map = build_openings(&[("x.tsv", table)]).expect("valid");
    let with_ep = plies(&["e4", "Nf6", "e5", "d5"]).pop().expect("ply").0;
    assert_eq!(with_ep.split(' ').nth(3), Some("d6"));
    let without_ep = with_ep.replacen(" d6 ", " - ", 1);
    assert!(map.contains_key(&fen_key(&with_ep)));
    assert!(!map.contains_key(&fen_key(&without_ep)));
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
    assert!(key.len() <= 46, "{key} has {} chars", key.len());
    assert!(key.split_once(':').expect("color").1.len() <= 40, "{key}");
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
            assert!(key.len() <= 46, "{key}");
            assert!(slug.len() <= 40, "{key}");
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

#[test]
fn distinct_families_get_distinct_line_keys() {
    let mut by_key: HashMap<String, &str> = HashMap::new();
    for o in openings().values() {
        let family = line_family(&o.name);
        let key = line_key(Color::White, &o.name);
        let seen = by_key.entry(key.clone()).or_insert(family);
        assert_eq!(*seen, family, "{key} names two families");
    }
}

#[test]
fn openings_is_built_once() {
    assert!(openings().len() >= 3000);
    assert!(std::ptr::eq(openings(), openings()));
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

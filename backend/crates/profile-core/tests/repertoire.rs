//! R2 Task 18: repertoire tree, struggle detection and verdicts (TS M2 Task 13 cases), plus the
//! ownership rule (T16 ruling), the honesty rules (no claims from zero evidence, intervals never
//! widened: an unestimable or zero-spread interval is `None` and claims nothing) and the
//! boundaries of every threshold. TS cases 4 and 5 use fixtures with real spread (ADR-R2-8).

use std::collections::HashMap;

use chess_core::board::Position;
use chess_core::fen::fen_key;
use chess_core::{Color, Ply, TimeClass};
use coaching_input::{LineSide, Verdict};
use profile_core::openings::lines::{
    BandComparison, BandSource, LineSummary, classify_line, line_owner, line_side, perf_interval,
    resolve_band, summarise_lines,
};
use profile_core::openings::repertoire::{
    AnalysedOpening, ExplorerIndex, ExplorerMove, MAX_REPERTOIRE_PLIES, RepertoireGame, book_exit,
    build_repertoire_tree, in_theory, opening_plies,
};
use profile_core::openings::verdicts::{
    STRUGGLE_THRESHOLDS, StruggleReason, decide_verdict, detect_struggles,
};
use profile_core::types::{Band, PlatformId};
use uuid::Uuid;

// ---------------------------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------------------------

fn replay(sans: &[&str]) -> Vec<Ply> {
    let mut p = Position::start();
    let mut out = Vec::new();
    for (i, s) in sans.iter().enumerate() {
        let fen_before = p.fen();
        let color = p.side_to_move();
        let played = p.play_san(s).unwrap_or_else(|e| panic!("{s}: {e}"));
        out.push(Ply {
            ply: u32::try_from(i).unwrap() + 1,
            color,
            san: played.san,
            uci: played.uci,
            fen_before,
            fen_after: p.fen(),
            clock_seconds: None,
        });
    }
    out
}

fn key_after(sans: &[&str]) -> String {
    let mut p = Position::start();
    for s in sans {
        p.play_san(s).unwrap();
    }
    fen_key(&p.fen())
}

static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn game(color: Color, sans: &[&str], score: f64, expected: Option<f64>) -> RepertoireGame {
    let plies = replay(sans);
    let id = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    RepertoireGame {
        game_id: Uuid::from_u128(u128::from(id)),
        color,
        plies: opening_plies(&plies),
        score,
        expected_score: expected,
        line: classify_line(&plies, color),
        analysed: None,
    }
}

fn analysed(win: Option<f64>, delta_w: Vec<Option<f64>>) -> AnalysedOpening {
    let n = delta_w.len();
    AnalysedOpening {
        win_at_opening_end: win,
        opening_accuracy: Some(80.0),
        time_in_opening: Some(30.0),
        eval_at_opening_end_white_cp: Some(20),
        delta_w,
        best_uci: vec![None; n],
    }
}

fn mv(uci: &str, white: i64, draws: i64, black: i64) -> ExplorerMove {
    ExplorerMove {
        uci: uci.to_owned(),
        white,
        draws,
        black,
        total: white + draws + black,
    }
}

fn own_band() -> BandComparison {
    BandComparison {
        player_band: Band::B2,
        band_source: BandSource::Rating,
        explorer_band: Some(Band::B2),
        explorer_fallback: false,
    }
}

fn lines(games: &[RepertoireGame], index: &ExplorerIndex) -> Vec<LineSummary> {
    summarise_lines(games, index, &own_band(), TimeClass::Blitz)
}

fn line<'a>(ls: &'a [LineSummary], key: &str, kind: LineSide) -> &'a LineSummary {
    ls.iter()
        .find(|l| l.line_key == key && l.kind == kind)
        .unwrap_or_else(|| {
            let have: Vec<_> = ls.iter().map(|l| (&l.line_key, l.kind)).collect();
            panic!("no line {key} {kind:?} in {have:?}")
        })
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

const ITALIAN: &[&str] = &["e4", "e5", "Nf3", "Nc6", "Bc4"];
const BISHOPS: &[&str] = &["e4", "e5", "Bc4"];
const CARO_ADVANCE: &[&str] = &["e4", "c6", "d4", "d5", "e5", "Bf5"];

/// 6 wins, 2 draws, 2 losses, expected 0.5 each.
fn italian_games() -> Vec<RepertoireGame> {
    let scores = [1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.5, 0.5, 0.0, 0.0];
    scores
        .iter()
        .map(|s| game(Color::White, ITALIAN, *s, Some(0.5)))
        .collect()
}

// ---------------------------------------------------------------------------------------------
// TS M2 Task 13 cases
// ---------------------------------------------------------------------------------------------

#[test]
fn italian_ten_games_node_stats_and_keep() {
    let games = italian_games();
    let tree = build_repertoire_tree(&games, Color::White);
    let node = tree.get(&key_after(ITALIAN)).expect("italian node");
    assert_eq!(node.games, 10);
    assert_eq!((node.wins, node.draws, node.losses), (6, 2, 2));
    assert!(close(node.score, 0.7), "score {}", node.score);
    assert!(close(node.perf_vs_expected.unwrap(), 0.2));

    let ls = lines(&games, &ExplorerIndex::new());
    assert_eq!(ls.len(), 1);
    let l = line(&ls, "white:italian-game", LineSide::Chosen);
    assert_eq!(l.name, "Italian Game");
    assert_eq!(l.eco.as_deref(), Some("C50"));
    assert_eq!(l.games, 10);
    assert!(close(l.score, 0.7));
    assert!(l.struggles.is_empty());
    assert_eq!(l.verdict, Some(Verdict::Keep), "{}", l.reason);
    assert_eq!(l.moves_san, ["e4", "e5", "Nf3", "Nc6", "Bc4"]);
}

#[test]
fn transposition_merges_into_one_node_with_two_parents() {
    let games = vec![
        game(Color::White, &["Nf3", "d5", "d4"], 1.0, None),
        game(Color::White, &["d4", "d5", "Nf3"], 0.0, None),
    ];
    let tree = build_repertoire_tree(&games, Color::White);
    let node = tree.get(&key_after(&["d4", "d5", "Nf3"])).unwrap();
    assert_eq!(node.games, 2);
    let mut want = vec![key_after(&["Nf3", "d5"]), key_after(&["d4", "d5"])];
    want.sort();
    assert_eq!(node.parents, want);
    assert!(close(node.score, 0.5));
    // No game has an expected score: no performance claim.
    assert_eq!(node.perf_vs_expected, None);
    // The start node has two children with one game each.
    let root = tree.get(&fen_key(&Position::start().fen())).unwrap();
    assert_eq!(root.games, 2);
    assert_eq!(root.children.len(), 2);
    assert!(root.children.iter().all(|c| c.games == 1));
    assert!(root.parents.is_empty());
}

/// 8 Caro-Kann (Advance) games for Black: 3...Bf5 loses ΔW 12 in 3 of them.
fn caro_games(errors: usize, total: usize, delta: f64) -> Vec<RepertoireGame> {
    (0..total)
        .map(|i| {
            let mut g = game(Color::Black, CARO_ADVANCE, 0.5, Some(0.5));
            let bf5 = if i < errors { delta } else { 1.0 };
            g.analysed = Some(analysed(
                Some(50.0),
                vec![
                    Some(0.0),
                    Some(1.0),
                    Some(0.0),
                    Some(1.0),
                    Some(0.0),
                    Some(bf5),
                ],
            ));
            g
        })
        .collect()
}

#[test]
fn caro_kann_repeated_error_is_fix_with_its_spot() {
    let games = caro_games(3, 8, 12.0);
    let ls = lines(&games, &ExplorerIndex::new());
    let l = line(
        &ls,
        "black:caro-kann-defense-advance-variation",
        LineSide::Chosen,
    );
    assert_eq!(l.games, 8);
    assert!(
        l.struggles
            .contains(&StruggleReason::RepeatedDeviationError)
    );
    assert_eq!(l.verdict, Some(Verdict::Fix), "{}", l.reason);
    let spot = l.fix_spot.as_ref().expect("fix spot");
    assert_eq!(spot.games, 3);
    assert_eq!(spot.uci, "c8f5");
    assert_eq!(spot.san, "Bf5");
    assert_eq!(spot.fen_key, key_after(&CARO_ADVANCE[..5]));
}

/// The position after 1.e4 e5 with Nf3 (500 games), Bc4 (the current move) and Nc3 (90).
fn bishops_index() -> ExplorerIndex {
    let mut index = ExplorerIndex::new();
    index.insert(
        key_after(&["e4", "e5"]),
        vec![
            mv("g1f3", 300, 100, 100),
            mv("f1c4", 150, 100, 150),
            mv("b1c3", 40, 10, 40),
        ],
    );
    index
}

/// `n` Bishop's Opening games with real spread (TS cases 4 and 5 amended, ADR-R2-8): two draws,
/// the rest losses, expected scores alternating ±0.05 around a mean chosen so the performance
/// is `perf`.
fn spread_games(n: usize, color: Color, perf: f64) -> Vec<RepertoireGame> {
    let scores: Vec<f64> = (0..n).map(|i| if i < 2 { 0.5 } else { 0.0 }).collect();
    let mean_score = scores.iter().sum::<f64>() / n as f64;
    let mean_expected = mean_score - perf;
    scores
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let d = if n % 2 == 1 && i == n - 1 {
                0.0
            } else if i % 2 == 0 {
                -0.05
            } else {
                0.05
            };
            game(color, BISHOPS, *s, Some(mean_expected + d))
        })
        .collect()
}

/// `n` games whose residuals are all `score - expected` = -0.2 (zero spread), with `noise`
/// added to every other expected score.
fn flat_games(n: usize, color: Color, noise: f64) -> Vec<RepertoireGame> {
    (0..n)
        .map(|i| {
            let e = if i % 2 == 0 { 0.2 } else { 0.2 + noise };
            game(color, BISHOPS, 0.0, Some(e))
        })
        .collect()
}

#[test]
fn zero_spread_gives_no_interval_and_no_replace() {
    // TS case 4 as written: 15 losses at expected 0.2. The residuals have no spread, so the
    // interval cannot be estimated and nothing that depends on it is claimed (binding ruling).
    for noise in [0.0, 1e-15, 1e-12] {
        let ls = lines(&flat_games(15, Color::White, noise), &bishops_index());
        let l = line(&ls, "white:bishops-opening", LineSide::Chosen);
        assert!(close(l.perf_vs_expected.unwrap(), -0.2));
        assert_eq!(l.perf_interval, None, "noise {noise}");
        assert!(!l.struggles.contains(&StruggleReason::ScoreBelowExpected));
        assert_ne!(l.verdict, Some(Verdict::Replace), "{}", l.reason);
        assert_eq!(l.verdict, Some(Verdict::Keep), "{}", l.reason);
        // TS case 5 as written (faced): no Learn from an unestimable interval either.
        let ls = lines(&flat_games(15, Color::Black, noise), &bishops_index());
        let l = line(&ls, "black:bishops-opening", LineSide::Faced);
        assert_eq!(l.perf_interval, None);
        assert_eq!(l.verdict, Some(Verdict::Keep), "{}", l.reason);
    }
}

#[test]
fn fifteen_losing_chosen_games_are_replace_with_alternatives() {
    let games = spread_games(15, Color::White, -0.2);
    let ls = lines(&games, &bishops_index());
    let l = line(&ls, "white:bishops-opening", LineSide::Chosen);
    assert!(close(l.perf_vs_expected.unwrap(), -0.2));
    assert!(l.struggles.contains(&StruggleReason::ScoreBelowExpected));
    assert_eq!(l.verdict, Some(Verdict::Replace), "{}", l.reason);
    assert_eq!(
        l.choice_fen_key.as_deref(),
        Some(&*key_after(&["e4", "e5"]))
    );
    let ucis: Vec<_> = l.alternatives.iter().map(|a| a.uci.as_str()).collect();
    assert_eq!(
        ucis,
        ["g1f3"],
        "b1c3 has 90 games, f1c4 is the current move"
    );
    let a = &l.alternatives[0];
    assert_eq!(a.san, "Nf3");
    assert_eq!(a.games, 500);
    assert!(close(a.band_score, 0.7), "white to move: (300 + 50) / 500");
    assert_eq!(a.name.as_deref(), Some("King's Knight Opening"));
    // The interval comes from the observed spread alone.
    let iv = l.perf_interval.unwrap();
    assert!(iv.low < iv.high && iv.high < 0.0, "{iv:?}");
}

#[test]
fn the_same_stats_faced_are_learn_never_replace() {
    let games = spread_games(15, Color::Black, -0.2);
    let ls = lines(&games, &bishops_index());
    let l = line(&ls, "black:bishops-opening", LineSide::Faced);
    assert!(l.struggles.contains(&StruggleReason::ScoreBelowExpected));
    assert_eq!(l.verdict, Some(Verdict::Learn), "{}", l.reason);
    assert!(
        l.alternatives.is_empty(),
        "no alternatives for a faced line"
    );
}

#[test]
fn seven_games_give_no_verdict_and_eight_do() {
    let seven: Vec<_> = italian_games().into_iter().take(7).collect();
    let l = &lines(&seven, &ExplorerIndex::new())[0];
    assert_eq!(l.verdict, None);
    assert!(l.struggles.is_empty(), "no struggle claims under 8 games");
    assert!(
        l.reason.contains('7') && l.reason.contains('8'),
        "{}",
        l.reason
    );

    let eight: Vec<_> = italian_games().into_iter().take(8).collect();
    assert_eq!(
        lines(&eight, &ExplorerIndex::new())[0].verdict,
        Some(Verdict::Keep)
    );
}

#[test]
fn seven_losing_games_claim_nothing() {
    // Zero evidence of enough games: no struggle and no verdict, however bad the score.
    let games: Vec<_> = (0..7)
        .map(|_| game(Color::White, BISHOPS, 0.0, Some(0.8)))
        .collect();
    let l = &lines(&games, &bishops_index())[0];
    assert!(l.struggles.is_empty());
    assert_eq!(l.verdict, None);
    assert_eq!(l.fix_spot, None);
    // The same move losing ΔW 20 in all 7 games of a line: still no struggle and no spot.
    let l = &lines(&caro_games(7, 7, 20.0), &ExplorerIndex::new())[0];
    assert_eq!(l.error_spot.as_ref().map(|s| s.games), Some(7));
    assert!(l.struggles.is_empty());
    assert_eq!(l.fix_spot, None);
    assert_eq!(l.verdict, None);
}

// ---------------------------------------------------------------------------------------------
// Ownership rule (T16 ruling)
// ---------------------------------------------------------------------------------------------

#[test]
fn ownership_rule_on_the_five_named_openings() {
    let cases: [(&[&str], &str, Color); 5] = [
        (&["d4", "d6", "c4"], "Old Indian Defense", Color::Black),
        (
            &["Nf3", "d5", "Nc3"],
            "Zukertort Opening: Reversed Mexican Defense",
            Color::White,
        ),
        (
            &[
                "e4", "c5", "Nf3", "d6", "d4", "cxd4", "Nxd4", "Nf6", "Nc3", "a6",
            ],
            "Sicilian Defense: Najdorf Variation",
            Color::Black,
        ),
        (ITALIAN, "Italian Game", Color::White),
        (
            &["d4", "d5", "c4", "e6"],
            "Queen's Gambit Declined",
            Color::Black,
        ),
    ];
    for (sans, family, owner) in cases {
        let plies = replay(sans);
        for player in [Color::White, Color::Black] {
            let l = classify_line(&plies, player).expect(family);
            assert_eq!(l.name, family);
            let want = if player == owner {
                LineSide::Chosen
            } else {
                LineSide::Faced
            };
            assert_eq!(l.kind, want, "{family} for {player:?}");
            assert!(l.key.starts_with(player.as_str()));
        }
    }
    // Old Indian is first named on White's 2.c4, yet Black owns it.
    let old = classify_line(&replay(&["d4", "d6", "c4"]), Color::Black).unwrap();
    assert_eq!(old.entry_ply, 3);
}

#[test]
fn ownership_words_and_reversed_precedence() {
    assert_eq!(
        line_owner("Queen's Gambit Accepted", Color::White),
        Color::Black
    );
    assert_eq!(
        line_owner("Englund Gambit Declined: Reversed French", Color::Black),
        Color::White
    );
    assert_eq!(
        line_owner("Latvian Countergambit", Color::White),
        Color::Black
    );
    assert_eq!(line_owner("Alekhine Defence", Color::White), Color::Black);
    assert_eq!(line_owner("Italian Game", Color::Black), Color::Black);
    assert_eq!(line_owner("Italian Game", Color::White), Color::White);
    // Whole words only.
    assert_eq!(line_owner("Defenseless Gambit", Color::White), Color::White);
    assert_eq!(
        line_side("Old Indian Defense", Color::White, Color::Black),
        LineSide::Chosen
    );
}

// ---------------------------------------------------------------------------------------------
// Book exit and deviations
// ---------------------------------------------------------------------------------------------

fn theory_index() -> ExplorerIndex {
    let mut index = ExplorerIndex::new();
    index.insert(
        key_after(&[]),
        vec![mv("e2e4", 400, 100, 300), mv("d2d4", 300, 100, 200)],
    );
    index.insert(
        key_after(&["e4"]),
        vec![
            mv("e7e5", 30, 10, 20),
            mv("c7c5", 20, 5, 20),
            mv("c7c6", 2, 1, 1),
        ],
    );
    index
}

#[test]
fn book_exit_attributes_the_deviation_to_its_mover() {
    let plies = opening_plies(&replay(&["e4", "c6", "d4"]));
    let index = theory_index();
    // 1...c6 has 4 of 110 games (< 5%) in a theory position: Black left the book at ply 2.
    assert!(!in_theory(&index, &key_after(&["e4"]), "c7c6"));
    assert_eq!(book_exit(&plies, &index, Color::Black), (Some(2), Some(2)));
    assert_eq!(book_exit(&plies, &index, Color::White), (Some(2), None));
    // 1...e5 is theory; the position after it is unknown, so theory runs out at ply 3 and
    // nobody deviated.
    let e5 = opening_plies(&replay(&["e4", "e5", "Nf3"]));
    assert_eq!(book_exit(&e5, &index, Color::White), (Some(3), None));
}

#[test]
fn book_exit_without_theory_evidence_claims_nothing() {
    let plies = opening_plies(&replay(&["e4", "c6"]));
    assert_eq!(
        book_exit(&plies, &ExplorerIndex::new(), Color::Black),
        (None, None)
    );
    // A start position with 49 games is not theory.
    let mut thin = ExplorerIndex::new();
    thin.insert(key_after(&[]), vec![mv("e2e4", 20, 9, 20)]);
    assert_eq!(book_exit(&plies, &thin, Color::White), (None, None));
    // 50 games is.
    thin.insert(key_after(&[]), vec![mv("e2e4", 20, 10, 20)]);
    assert_eq!(book_exit(&plies, &thin, Color::White), (Some(2), None));
    assert_eq!(book_exit(&[], &thin, Color::White), (None, None));
}

#[test]
fn in_theory_boundaries() {
    let mut index = ExplorerIndex::new();
    index.insert("p".into(), vec![mv("a", 45, 0, 0), mv("b", 5, 0, 0)]);
    assert!(in_theory(&index, "p", "b"), "5 of 50 is exactly 5%");
    index.insert("p".into(), vec![mv("a", 96, 0, 0), mv("b", 4, 0, 0)]);
    assert!(!in_theory(&index, "p", "b"), "4 of 100");
    index.insert("p".into(), vec![mv("a", 95, 0, 0), mv("b", 5, 0, 0)]);
    assert!(in_theory(&index, "p", "b"), "5 of 100");
    index.insert("p".into(), vec![mv("a", 44, 0, 0), mv("b", 5, 0, 0)]);
    assert!(!in_theory(&index, "p", "a"), "49 games");
    assert!(!in_theory(&index, "q", "a"));
}

/// The index after 1.e4 e5 where 2.Bc4 has 40 of 990 games (4%).
fn rare_bishop_index() -> ExplorerIndex {
    let mut index = ExplorerIndex::new();
    index.insert(key_after(&[]), vec![mv("e2e4", 500, 100, 400)]);
    index.insert(key_after(&["e4"]), vec![mv("e7e5", 500, 100, 400)]);
    index.insert(
        key_after(&["e4", "e5"]),
        vec![mv("g1f3", 450, 100, 400), mv("f1c4", 20, 0, 20)],
    );
    index
}

fn d4_games(n: usize) -> Vec<RepertoireGame> {
    (0..n)
        .map(|_| game(Color::White, &["d4", "d5", "c4", "e6"], 0.5, Some(0.5)))
        .collect()
}

#[test]
fn early_deviation_in_a_frequent_chosen_line_is_learn() {
    let games: Vec<_> = (0..10)
        .map(|_| game(Color::White, BISHOPS, 0.5, Some(0.5)))
        .collect();
    let ls = lines(&games, &rare_bishop_index());
    let l = line(&ls, "white:bishops-opening", LineSide::Chosen);
    assert_eq!(l.deviation_ply, Some(3));
    assert_eq!(l.deviation_move_san.as_deref(), Some("Bc4"));
    assert_eq!(l.median_deviation_ply, Some(3.0));
    assert!(l.struggles.is_empty());
    assert_eq!(l.verdict, Some(Verdict::Learn), "{}", l.reason);
}

#[test]
fn line_share_boundary_for_learn() {
    let bishops = || -> Vec<RepertoireGame> {
        (0..10)
            .map(|_| game(Color::White, BISHOPS, 0.5, Some(0.5)))
            .collect()
    };
    // 10 of 100 white games: exactly 10%.
    let mut at = bishops();
    at.extend(d4_games(90));
    let l = lines(&at, &rare_bishop_index());
    assert!(close(
        line(&l, "white:bishops-opening", LineSide::Chosen).share,
        0.1
    ));
    assert_eq!(
        line(&l, "white:bishops-opening", LineSide::Chosen).verdict,
        Some(Verdict::Learn)
    );
    // 10 of 101: below 10%.
    let mut below = bishops();
    below.extend(d4_games(91));
    let l = lines(&below, &rare_bishop_index());
    assert_eq!(
        line(&l, "white:bishops-opening", LineSide::Chosen).verdict,
        Some(Verdict::Keep)
    );
}

#[test]
fn deviation_in_a_minority_of_games_is_not_typical() {
    // 4 of 10 games deviate at 2.Bc4; 6 reach 2.Bc4 via theory (counted as staying in book).
    let mut index = rare_bishop_index();
    let mut games: Vec<_> = (0..4)
        .map(|_| game(Color::White, BISHOPS, 0.5, Some(0.5)))
        .collect();
    // The other 6 play 1.e4 e5 2.Nf3 Nc6 3.Bc4: all in theory, then theory runs out.
    index.insert(
        key_after(&["e4", "e5"]),
        vec![mv("g1f3", 450, 100, 400), mv("f1c4", 20, 0, 20)],
    );
    games.extend((0..6).map(|_| game(Color::White, ITALIAN, 0.5, Some(0.5))));
    let ls = lines(&games, &index);
    let l = line(&ls, "white:bishops-opening", LineSide::Chosen);
    assert_eq!(l.games, 4);
    // 4 games is too few for a deviation claim.
    assert_eq!(l.median_deviation_ply, None);
    assert_eq!(l.deviation_ply, None);
}

#[test]
fn median_deviation_censors_games_that_stayed_in_book() {
    // 8 Bishop's Opening games. With 2.Bc4 rare, all 8 deviate at ply 3.
    let mut index = rare_bishop_index();
    let games: Vec<_> = (0..8)
        .map(|_| game(Color::White, BISHOPS, 0.5, Some(0.5)))
        .collect();
    let ls = lines(&games, &index);
    assert_eq!(
        line(&ls, "white:bishops-opening", LineSide::Chosen).median_deviation_ply,
        Some(3.0)
    );
    // With Bc4 common (theory), no game deviates: the median is censored, no deviation claim.
    index.insert(
        key_after(&["e4", "e5"]),
        vec![mv("g1f3", 450, 100, 400), mv("f1c4", 200, 0, 200)],
    );
    let ls = lines(&games, &index);
    let l = line(&ls, "white:bishops-opening", LineSide::Chosen);
    assert_eq!(l.median_deviation_ply, None);
    assert_eq!(l.deviation_ply, None);
    assert_eq!(l.deviation_evidence_games, 8);
    assert_eq!(l.verdict, Some(Verdict::Keep));
}

#[test]
fn no_explorer_data_means_no_deviation_claim() {
    let l = &lines(&italian_games(), &ExplorerIndex::new())[0];
    assert_eq!(l.deviation_ply, None);
    assert_eq!(l.deviation_move_san, None);
    assert_eq!(l.median_deviation_ply, None);
    assert_eq!(l.deviation_evidence_games, 0);
    assert!(l.alternatives.is_empty());
}

// ---------------------------------------------------------------------------------------------
// Struggle and verdict boundaries
// ---------------------------------------------------------------------------------------------

fn with_win(mut games: Vec<RepertoireGame>, wins: &[Option<f64>]) -> Vec<RepertoireGame> {
    for (g, w) in games.iter_mut().zip(wins) {
        g.analysed = Some(analysed(*w, vec![None; g.plies.len()]));
    }
    games
}

#[test]
fn worse_after_opening_needs_five_analysed_games_at_40_or_less() {
    let five_at_40 = with_win(italian_games(), &[Some(40.0); 5]);
    let l = &lines(&five_at_40, &ExplorerIndex::new())[0];
    assert_eq!(l.analysed_games, 5);
    assert!(l.struggles.contains(&StruggleReason::WorseAfterOpening));
    assert_eq!(l.verdict, Some(Verdict::Learn));

    let four = with_win(italian_games(), &[Some(10.0); 4]);
    let l = &lines(&four, &ExplorerIndex::new())[0];
    assert!(!l.struggles.contains(&StruggleReason::WorseAfterOpening));
    assert_eq!(l.verdict, Some(Verdict::Keep));

    // An analysed game without an opening-end win% is not evidence.
    let mut five_one_missing = vec![Some(10.0); 4];
    five_one_missing.push(None);
    let l = &lines(
        &with_win(italian_games(), &five_one_missing),
        &ExplorerIndex::new(),
    )[0];
    assert_eq!(l.analysed_games, 4);
    assert!(l.struggles.is_empty());

    let above = with_win(italian_games(), &[Some(40.5); 5]);
    assert!(lines(&above, &ExplorerIndex::new())[0].struggles.is_empty());
}

#[test]
fn repeated_error_boundaries() {
    // ΔW exactly 10 counts.
    let l = &lines(&caro_games(3, 8, 10.0), &ExplorerIndex::new())[0];
    assert_eq!(l.verdict, Some(Verdict::Fix));
    // Below 10 does not.
    let l = &lines(&caro_games(3, 8, 9.99), &ExplorerIndex::new())[0];
    assert!(l.struggles.is_empty());
    assert_eq!(l.fix_spot, None);
    assert_eq!(l.verdict, Some(Verdict::Keep));
    // Two games do not.
    let l = &lines(&caro_games(2, 8, 20.0), &ExplorerIndex::new())[0];
    assert_eq!(l.verdict, Some(Verdict::Keep));
    // 3 of 10 is exactly 30%; 3 of 11 is below.
    let l = &lines(&caro_games(3, 10, 20.0), &ExplorerIndex::new())[0];
    assert_eq!(l.verdict, Some(Verdict::Fix));
    let l = &lines(&caro_games(3, 11, 20.0), &ExplorerIndex::new())[0];
    assert_eq!(l.verdict, Some(Verdict::Keep));
}

#[test]
fn opponent_errors_are_not_the_players_fix() {
    // ΔW 30 on White's 3.e5 (the opponent's move) in every game: no fix for Black.
    let games: Vec<_> = (0..8)
        .map(|_| {
            let mut g = game(Color::Black, CARO_ADVANCE, 0.5, Some(0.5));
            g.analysed = Some(analysed(
                Some(50.0),
                vec![None, None, None, None, Some(30.0), Some(0.0)],
            ));
            g
        })
        .collect();
    let l = &lines(&games, &ExplorerIndex::new())[0];
    assert!(l.struggles.is_empty());
    assert_eq!(l.fix_spot, None);
}

#[test]
fn replace_boundaries() {
    // 14 games: score_below holds, but replace needs 15, so learn.
    let games = spread_games(14, Color::White, -0.2);
    let l = &lines(&games, &bishops_index())[0];
    assert!(l.struggles.contains(&StruggleReason::ScoreBelowExpected));
    assert_eq!(l.verdict, Some(Verdict::Learn));
    // 15 games at perf -0.14: score_below, but not -0.15, so learn.
    let games = spread_games(15, Color::White, -0.14);
    let l = &lines(&games, &bishops_index())[0];
    assert!(l.struggles.contains(&StruggleReason::ScoreBelowExpected));
    assert_eq!(l.verdict, Some(Verdict::Learn));
    // perf exactly -0.15 is replace.
    let games = spread_games(15, Color::White, -0.15);
    assert_eq!(
        lines(&games, &bishops_index())[0].verdict,
        Some(Verdict::Replace)
    );
}

#[test]
fn score_below_needs_the_interval_below_zero() {
    // perf -0.125 over 8 games: below -0.10, but the 80% interval reaches above 0.
    let scores = [1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0];
    let games: Vec<_> = scores
        .iter()
        .map(|s| game(Color::White, BISHOPS, *s, Some(0.5)))
        .collect();
    let l = &lines(&games, &ExplorerIndex::new())[0];
    assert!(close(l.perf_vs_expected.unwrap(), -0.125));
    assert!(l.perf_interval.unwrap().high > 0.0);
    assert!(l.struggles.is_empty());
    assert_eq!(l.verdict, Some(Verdict::Keep));
}

#[test]
fn score_below_needs_eight_games_with_an_expected_score() {
    // 10 games, but only 7 have ratings: no performance-based struggle.
    let games: Vec<_> = (0..10)
        .map(|i| {
            let exp = (i < 7).then_some(if i % 2 == 0 { 0.75 } else { 0.85 });
            game(Color::White, BISHOPS, 0.0, exp)
        })
        .collect();
    let l = &lines(&games, &ExplorerIndex::new())[0];
    assert_eq!(l.perf_games, 7);
    assert!(l.perf_vs_expected.is_some());
    assert!(!l.struggles.contains(&StruggleReason::ScoreBelowExpected));
    assert_eq!(l.verdict, Some(Verdict::Keep));
}

#[test]
fn decide_verdict_rule_order() {
    let games = spread_games(15, Color::White, -0.2);
    let l = lines(&games, &bishops_index()).remove(0);
    // Fix outranks replace.
    let both = [
        StruggleReason::ScoreBelowExpected,
        StruggleReason::RepeatedDeviationError,
    ];
    assert_eq!(
        decide_verdict(&l, &both, 15, &l.alternatives).0,
        Some(Verdict::Fix)
    );
    assert_eq!(
        decide_verdict(
            &l,
            &[StruggleReason::ScoreBelowExpected],
            15,
            &l.alternatives
        )
        .0,
        Some(Verdict::Replace)
    );
    assert_eq!(decide_verdict(&l, &[], 15, &[]).0, Some(Verdict::Keep));
    assert_eq!(
        detect_struggles(&l),
        vec![StruggleReason::ScoreBelowExpected]
    );
    assert_eq!(STRUGGLE_THRESHOLDS.min_games, 8);
    assert_eq!(STRUGGLE_THRESHOLDS.deviation_min_games, 3);
}

#[test]
fn fallback_band_is_reported_in_the_reason() {
    let games = spread_games(15, Color::White, -0.2);
    let fallback = BandComparison {
        player_band: Band::B3,
        band_source: BandSource::Rating,
        explorer_band: Some(Band::B2),
        explorer_fallback: true,
    };
    let ls = summarise_lines(&games, &bishops_index(), &fallback, TimeClass::Blitz);
    let l = &ls[0];
    assert_eq!(l.verdict, Some(Verdict::Replace));
    assert!(
        l.reason.contains("B2") && l.reason.contains("fallback"),
        "{}",
        l.reason
    );
    assert_eq!(l.comparison, fallback);
    // Own band: no fallback wording.
    let own = &lines(&games, &bishops_index())[0];
    assert!(!own.reason.contains("fallback"), "{}", own.reason);
}

// ---------------------------------------------------------------------------------------------
// Band resolution (R-4, ruling Q2) and misc
// ---------------------------------------------------------------------------------------------

#[test]
fn band_resolution_rating_then_opponents_then_b1() {
    assert_eq!(
        resolve_band(PlatformId::Lichess, TimeClass::Blitz, Some(1500), &[2500]),
        (Band::B2, BandSource::Rating)
    );
    assert_eq!(
        resolve_band(PlatformId::Lichess, TimeClass::Blitz, None, &[1000, 1200]),
        (Band::B1, BandSource::OpponentMean)
    );
    assert_eq!(
        resolve_band(PlatformId::Lichess, TimeClass::Blitz, None, &[]),
        (Band::B1, BandSource::Default)
    );
    // chess.com ratings are mapped to the Lichess-equivalent scale first.
    let offset = profile_core::constants::chesscom_to_lichess_offset(TimeClass::Blitz);
    let (band, _) = resolve_band(
        PlatformId::Chesscom,
        TimeClass::Blitz,
        Some(1699 - offset),
        &[],
    );
    assert_eq!(band, Band::B2);
    let (band, _) = resolve_band(
        PlatformId::Chesscom,
        TimeClass::Blitz,
        Some(1700 - offset),
        &[],
    );
    assert_eq!(band, Band::B3);
}

#[test]
fn opening_plies_are_capped_at_30() {
    let sans = [
        "Nf3", "Nf6", "Ng1", "Ng8", "Nf3", "Nf6", "Ng1", "Ng8", "Nf3", "Nf6", "Ng1", "Ng8", "Nf3",
        "Nf6", "Ng1", "Ng8", "Nf3", "Nf6", "Ng1", "Ng8", "Nf3", "Nf6", "Ng1", "Ng8", "Nf3", "Nf6",
        "Ng1", "Ng8", "Nf3", "Nf6", "Ng1", "Ng8",
    ];
    let g = game(Color::White, &sans, 1.0, None);
    assert_eq!(g.plies.len(), MAX_REPERTOIRE_PLIES);
    // A repeated position counts once per game.
    let tree = build_repertoire_tree(&[g], Color::White);
    let root = tree.get(&key_after(&[])).unwrap();
    assert_eq!(root.games, 1);
    assert_eq!(root.children.len(), 1);
    assert_eq!(root.children[0].games, 1);
}

#[test]
fn tree_is_per_colour_and_averages_only_known_values() {
    let mut games = italian_games();
    games[0].analysed = Some(analysed(Some(60.0), vec![None; 5]));
    games[1].analysed = Some(analysed(None, vec![None; 5]));
    games.push(game(Color::Black, ITALIAN, 1.0, Some(0.5)));
    let tree = build_repertoire_tree(&games, Color::White);
    let node = tree.get(&key_after(ITALIAN)).unwrap();
    assert_eq!(node.games, 10);
    assert_eq!(node.analysed_games, 2);
    assert_eq!(node.avg_win_at_opening_end, Some(60.0));
    assert_eq!(node.avg_opening_accuracy, Some(80.0));
    let black = build_repertoire_tree(&games, Color::Black);
    assert_eq!(black.get(&key_after(ITALIAN)).unwrap().games, 1);
    let empty: HashMap<_, _> = build_repertoire_tree(&[], Color::White);
    assert!(empty.is_empty());
}

#[test]
fn avg_eval_is_the_rounded_white_pov_mean() {
    let mut games = italian_games();
    for (g, cp) in games.iter_mut().zip([10, 11, -1000]) {
        let mut a = analysed(Some(50.0), vec![None; 5]);
        a.eval_at_opening_end_white_cp = Some(cp);
        g.analysed = Some(a);
    }
    let l = &lines(&games, &ExplorerIndex::new())[0];
    // (10 + 11 - 1000) / 3 = -326.33 → -326.
    assert_eq!(l.avg_eval_white_cp, Some(-326));
    assert_eq!(l.eval_games, 3);
    let none = &lines(&italian_games(), &ExplorerIndex::new())[0];
    assert_eq!(none.avg_eval_white_cp, None);
}

#[test]
fn summary_round_trips_through_json() {
    let games = spread_games(15, Color::White, -0.2);
    let l = lines(&games, &bishops_index()).remove(0);
    let json = serde_json::to_value(&l).unwrap();
    assert_eq!(json["kind"], "chosen");
    assert_eq!(json["verdict"], "replace");
    assert_eq!(json["comparison"]["explorerBand"], "B2");
    let back: LineSummary = serde_json::from_value(json).unwrap();
    assert_eq!(back, l);
}

#[test]
fn perf_interval_is_never_widened_and_none_when_unestimable() {
    // One game, or identical residuals (exact or within float noise): not estimable.
    assert_eq!(perf_interval(&[(0.0, 0.2)]), None);
    assert_eq!(perf_interval(&[]), None);
    assert_eq!(perf_interval(&[(0.0, 0.2); 15]), None);
    assert_eq!(perf_interval(&[(0.0, 0.2), (0.0, 0.2 + 1e-12)]), None);
    assert_eq!(perf_interval(&[(f64::NAN, 0.2), (0.0, 0.2)]), None);
    // Two different residuals: the plain sample interval, mean ± Z80·s/√n.
    let iv = perf_interval(&[(1.0, 0.5), (0.0, 0.5)]).unwrap();
    let half = 1.2815515655446004 * (0.5f64 / 2.0).sqrt();
    assert!(close(iv.low, -half) && close(iv.high, half), "{iv:?}");
}

#[test]
fn replace_without_explorer_data_says_so_plainly() {
    let games = spread_games(15, Color::White, -0.2);
    let none = BandComparison {
        player_band: Band::B1,
        band_source: BandSource::Default,
        explorer_band: None,
        explorer_fallback: false,
    };
    let l = &summarise_lines(&games, &ExplorerIndex::new(), &none, TimeClass::Blitz)[0];
    assert_eq!(l.verdict, Some(Verdict::Replace));
    assert!(
        l.reason
            .contains("no explorer data within 2 bands of B1 to suggest alternatives"),
        "{}",
        l.reason
    );
    assert!(!l.reason.contains("among no explorer"), "{}", l.reason);
}

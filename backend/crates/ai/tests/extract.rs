//! Port of `.worktrees/m3/packages/ai/test/validator/extract.test.ts` (the `pvToSan` block moves to
//! `pv.rs`, Task 7). Each `describe` is a module and each `it` title is snake_case.

use ai::validator::extract::{
    ExtractedDate, ExtractedMoves, ExtractedNumber, NumberKind, extract_dates, extract_moves,
    extract_numbers,
};
use ai::validator::normalize::normalize_text;

fn values(ns: &[ExtractedNumber]) -> Vec<f64> {
    ns.iter().map(|n| n.value).collect()
}

fn san(text: &str) -> Vec<String> {
    extract_moves(text).san
}

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| (*x).to_owned()).collect()
}

fn no_moves() -> ExtractedMoves {
    ExtractedMoves {
        san: vec![],
        uci: vec![],
    }
}

mod normalize_text {
    use super::*;

    #[test]
    fn applies_nfkc_full_width_digits_and_letters() {
        assert_eq!(
            normalize_text("\u{ff11}\u{ff12}. \u{ff2e}\u{ff46}\u{ff13}"),
            "12. Nf3"
        );
    }

    #[test]
    fn strips_zero_width_characters() {
        assert_eq!(
            normalize_text("N\u{200b}f\u{200c}3\u{200d}\u{feff}\u{2060}"),
            "Nf3"
        );
    }

    #[test]
    fn maps_confusable_minus_signs_before_digits() {
        assert_eq!(
            normalize_text("\u{2212}4.2 \u{2013}3 \u{2014} x"),
            "-4.2 -3 \u{2014} x"
        );
    }

    #[test]
    fn maps_figurines_to_letters() {
        assert_eq!(
            normalize_text(
                "\u{2654}\u{2655}\u{2656}\u{2657}\u{2658} \u{265a}\u{265b}\u{265c}\u{265d}\u{265e}"
            ),
            "KQRBN KQRBN"
        );
    }

    #[test]
    fn maps_0_0_castling_to_o_o() {
        assert_eq!(normalize_text("0-0 and 0-0-0"), "O-O and O-O-O");
    }

    #[test]
    fn is_idempotent() {
        let s = "\u{2212}\u{ff14}.2 \u{2658}f3 0-0";
        assert_eq!(normalize_text(&normalize_text(s)), normalize_text(s));
    }
}

mod extract_moves {
    use super::*;

    #[test]
    fn finds_san_with_move_numbers_suffixes_and_castling() {
        assert_eq!(
            san("After 12. Nxe5 dxe5 13.Qh5+ you missed O-O-O"),
            s(&["Nxe5", "dxe5", "Qh5", "O-O-O"])
        );
    }

    #[test]
    fn does_not_treat_squares_as_moves() {
        assert_eq!(san("the knight on f3 guards e5"), s(&[]));
    }

    #[test]
    fn counts_a_bare_pawn_move_after_a_move_number() {
        assert_eq!(san("12...e5 was the problem"), s(&["e5"]));
        assert_eq!(san("12. e4 e5"), s(&["e4", "e5"]));
        assert_eq!(san("12. ... e5"), s(&["e5"]));
    }

    #[test]
    fn counts_a_bare_pawn_move_after_play_push_words() {
        assert_eq!(san("you played h4 too early"), s(&["h4"]));
        assert_eq!(san("pushed e5 and then advance d4"), s(&["e5", "d4"]));
    }

    #[test]
    fn keeps_promotions_without_suffix() {
        assert_eq!(san("e8=Q+ wins"), s(&["e8=Q"]));
        assert_eq!(san("exd8=N#"), s(&["exd8=N"]));
    }

    #[test]
    fn handles_o_and_0_castling() {
        assert_eq!(san(&normalize_text("castle with 0-0")), s(&["O-O"]));
        assert_eq!(san("0-0-0 and O-O+"), s(&["O-O-O", "O-O"]));
    }

    #[test]
    fn handles_figurine_notation() {
        assert_eq!(san(&normalize_text("\u{2658}f3")), s(&["Nf3"]));
        assert_eq!(san("\u{265e}xe5"), s(&["Nxe5"]));
    }

    #[test]
    fn strips_every_annotation_glyph() {
        assert_eq!(san("Rxe6?? lost and exd5 was fine"), s(&["Rxe6", "exd5"]));
        for g in ["!", "?", "!!", "??", "!?", "?!"] {
            assert_eq!(
                san(&format!("Nf3{g} and Qh5+{g} and Qxf7#{g}")),
                s(&["Nf3", "Qh5", "Qxf7"]),
                "glyph {g}"
            );
        }
    }

    #[test]
    fn finds_disambiguated_san() {
        assert_eq!(san("Nbd7 R1e2"), s(&["Nbd7", "R1e2"]));
        assert_eq!(extract_moves("Qh4xe1").uci, s(&["h4e1"]));
    }

    #[test]
    fn finds_uci() {
        assert_eq!(
            extract_moves("the engine line g1f3 d7d5").uci,
            s(&["g1f3", "d7d5"])
        );
        assert_eq!(extract_moves("a7a8q").uci, s(&["a7a8q"]));
    }

    #[test]
    fn is_not_fooled_by_zero_width_characters_inside_a_move() {
        assert_eq!(san("N\u{200b}f3 and Q\u{2060}h5"), s(&["Nf3", "Qh5"]));
    }

    #[test]
    fn ignores_ids_time_controls_dates() {
        assert_eq!(
            extract_moves("W3 at B2 in 3+0 blitz on 2026-10-14"),
            no_moves()
        );
    }

    #[test]
    fn does_not_match_san_inside_longer_words() {
        assert_eq!(extract_moves("Bad Rd1ng Knights"), no_moves());
    }
}

mod extract_numbers {
    use super::*;

    #[test]
    fn handles_thousands_separators() {
        assert_eq!(
            values(&extract_numbers(
                "over your last 214 analysed rapid games (3,912 imported)"
            )),
            vec![214.0, 3912.0]
        );
    }

    #[test]
    fn normalises_unicode_minus_and_skips_move_numbers_and_san() {
        let r = extract_numbers(&normalize_text("12. Nxe5 \u{2212}4.2"));
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].value, -4.2);
        assert_eq!(r[0].decimals, 1);
        assert_eq!(extract_numbers("12...e5 and 13.Qh5+ g1f3"), vec![]);
    }

    #[test]
    fn normalises_full_width_digits_and_zero_width_splits() {
        assert_eq!(
            values(&extract_numbers("\u{ff12}\u{ff11}\u{ff14} games")),
            vec![214.0]
        );
        assert_eq!(
            values(&extract_numbers("2\u{200b}1\u{200b}4 games")),
            vec![214.0]
        );
    }

    #[test]
    fn flags_percent() {
        let r = extract_numbers("31% of games");
        assert_eq!(r.len(), 1);
        assert!(r[0].value == 31.0 && r[0].percent);
        let r = extract_numbers("31 percent of games");
        assert_eq!(r.len(), 1);
        assert!(r[0].value == 31.0 && r[0].percent);
        assert!(!extract_numbers("31 games")[0].percent);
    }

    #[test]
    fn handles_decimals_and_signs() {
        let r = extract_numbers("+0.35 and -1.5 and 12.25");
        let got: Vec<(f64, u32)> = r.iter().map(|n| (n.value, n.decimals)).collect();
        assert_eq!(got, vec![(0.35, 2), (-1.5, 1), (12.25, 2)]);
    }

    #[test]
    fn splits_ranges_into_unsigned_values() {
        assert_eq!(values(&extract_numbers("10-12 games")), vec![10.0, 12.0]);
        assert_eq!(
            values(&extract_numbers("10\u{2013}12 games")),
            vec![10.0, 12.0]
        );
    }

    #[test]
    fn handles_plus_minus() {
        assert_eq!(values(&extract_numbers("45% \u{b1} 6")), vec![45.0, 6.0]);
    }

    #[test]
    fn sets_hedged() {
        let r = extract_numbers("about 40 games");
        assert_eq!(r.len(), 1);
        assert!(r[0].value == 40.0 && r[0].hedged);
        assert!(extract_numbers("at least 5 games")[0].hedged);
        assert!(extract_numbers("more than 5 games")[0].hedged);
        assert!(extract_numbers("~12 games")[0].hedged);
        assert!(!extract_numbers("exactly 5 games")[0].hedged);
    }

    #[test]
    fn handles_ratio_words() {
        for (text, v) in [
            ("twice as often", 2.0),
            ("double the rate", 2.0),
            ("half of them", 0.5),
            ("three times as often", 3.0),
        ] {
            let r = extract_numbers(text);
            assert_eq!(r.len(), 1, "{text}");
            assert_eq!(r[0].value, v, "{text}");
            assert_eq!(r[0].kind, NumberKind::Ratio, "{text}");
        }
    }

    #[test]
    fn counts_number_words_only_with_a_unit() {
        assert_eq!(values(&extract_numbers("three games")), vec![3.0]);
        assert_eq!(values(&extract_numbers("three rapid games")), vec![3.0]);
        assert_eq!(extract_numbers("one of your games"), vec![]);
        assert_eq!(extract_numbers("one thing to try"), vec![]);
        let r = extract_numbers("ten percent");
        assert_eq!(r.len(), 1);
        assert!(r[0].value == 10.0 && r[0].percent);
    }

    #[test]
    fn excludes_weakness_ids_band_ids_and_citations() {
        assert_eq!(extract_numbers("W12 and B3 [[g:abc:24]]"), vec![]);
        assert_eq!(extract_numbers("W3a and W10b"), vec![]);
    }

    #[test]
    fn excludes_dates_but_keeps_nearby_numbers() {
        assert_eq!(
            values(&extract_numbers("since 2026-10-14 you lost 5 games")),
            vec![5.0]
        );
        assert_eq!(extract_numbers("since about 14 Oct").len(), 0);
        assert_eq!(extract_numbers("on Oct 14, 2026").len(), 0);
    }

    #[test]
    fn excludes_game_results() {
        assert_eq!(extract_numbers("lost 0-1 and won 1-0"), vec![]);
    }

    #[test]
    fn excludes_ordinals_and_letter_glued_digits() {
        assert_eq!(extract_numbers("your 3rd game, square e5"), vec![]);
    }
}

mod extract_dates {
    use super::*;

    fn d(year: Option<i32>, month: u32, day: u32) -> ExtractedDate {
        ExtractedDate { year, month, day }
    }

    #[test]
    fn parses_day_month_and_month_day_forms() {
        assert_eq!(extract_dates("since about 14 Oct"), vec![d(None, 10, 14)]);
        assert_eq!(extract_dates("on Oct 14th"), vec![d(None, 10, 14)]);
        assert_eq!(
            extract_dates("on 3 September 2026"),
            vec![d(Some(2026), 9, 3)]
        );
    }

    #[test]
    fn parses_iso_dates() {
        assert_eq!(extract_dates("2026-10-14"), vec![d(Some(2026), 10, 14)]);
    }

    #[test]
    fn ignores_invalid_and_non_dates() {
        assert_eq!(extract_dates("2026-13-45"), vec![]);
        assert_eq!(extract_dates("you may 5 times"), vec![]);
        assert_eq!(extract_dates("31 games"), vec![]);
    }
}

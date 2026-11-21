//! `CoachingInput::validate`: the zod bounds and refinements, with zod's issue paths.

use coaching_input::{CoachingInput, Issue, LIMITS, MetricUnit};
use time::macros::date;

fn sample() -> CoachingInput {
    let path = format!(
        "{}/../../fixtures/coaching-input/valid-sample.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    serde_json::from_str(&text).expect("valid-sample.json deserialises")
}

fn issues(input: &CoachingInput) -> Vec<Issue> {
    input.validate().expect_err("expected validation issues")
}

/// Asserts that `validate` reports an issue at `path` and returns its message.
fn issue_at(input: &CoachingInput, path: &str) -> String {
    let all = issues(input);
    all.iter()
        .find(|i| i.path == path)
        .map(|i| i.message.clone())
        .unwrap_or_else(|| panic!("no issue at {path:?}; got {all:#?}"))
}

#[test]
fn valid_sample_passes() {
    assert_eq!(sample().validate(), Ok(()));
}

#[test]
fn fen_side_to_move_disagrees_with_ply() {
    let mut input = sample();
    let ex = &mut input.weaknesses[0].exemplars[0];
    assert!(ex.fen.contains(" w "), "fixture exemplar is White to move");
    ex.ply += 1; // even ply = Black to move
    let msg = issue_at(&input, "weaknesses.0.exemplars.0.fen");
    assert!(msg.contains("disagrees with ply 6"), "{msg}");
    // Same rule on change examples (flattened exemplar fields).
    let mut input = sample();
    input.changes[0].examples[0].position.ply += 1;
    issue_at(&input, "changes.0.examples.0.fen");
}

#[test]
fn invalid_fen() {
    let mut input = sample();
    input.weaknesses[0].exemplars[0].fen = "not a fen".into();
    assert_eq!(
        issue_at(&input, "weaknesses.0.exemplars.0.fen"),
        "invalid FEN"
    );
    let mut input = sample();
    input.strengths[0].exemplars[1].fen = "x".repeat(101);
    issue_at(&input, "strengths.0.exemplars.1.fen");

    // Structural rules `Position::from_fen` alone does not enforce: six fields, eight ranks, no
    // two digits in a row (as chess.js). Single-space separation is deliberately stricter than
    // chess.js/zod (which accept runs of whitespace).
    let fen = sample().weaknesses[0].exemplars[0].fen.clone();
    assert_eq!(fen.split('/').nth(4), Some("4P3"), "fixture's fifth rank");
    // White to move, like the fixture exemplar (ply 5), so only the shape differs.
    let start = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
    let mut input = sample();
    input.weaknesses[0].exemplars[0].fen = start.into();
    assert_eq!(input.validate(), Ok(()));
    let shapes = [
        ("leading space", format!(" {fen}")),
        ("trailing space", format!("{fen} ")),
        ("trailing newline", format!("{fen}\n")),
        (
            "double space (stricter than chess.js/zod)",
            fen.replacen(' ', "  ", 1),
        ),
        (
            "tab separator (stricter than chess.js/zod)",
            fen.replacen(' ', "\t", 1),
        ),
        ("adjacent digits 12", fen.replacen("4P3", "4P12", 1)),
        ("invalid piece", fen.replacen("4P3", "4X3", 1)),
        ("adjacent digits 44", start.replacen("/8/", "/44/", 1)),
        ("nine ranks", start.replacen("/8/", "/8/8/", 1)),
    ];
    for (case, bad) in shapes {
        let mut input = sample();
        input.weaknesses[0].exemplars[0].fen = bad.clone();
        assert_eq!(
            issue_at(&input, "weaknesses.0.exemplars.0.fen"),
            "invalid FEN",
            "{case}: {bad:?}"
        );
    }
}

#[test]
fn metric_low_above_high() {
    let mut input = sample();
    input.metrics[0].low = Some(1.0);
    input.metrics[0].high = Some(0.8);
    assert_eq!(issue_at(&input, "metrics.0.low"), "low must be <= high");
}

#[test]
fn validate_rejects_value_outside_unit_range() {
    let mut input = sample();
    let m = &mut input.metrics[0];
    m.unit = MetricUnit::Rate;
    m.value = 1.2;
    m.low = None;
    m.high = None;
    m.band_median = None;
    let msg = issue_at(&input, "metrics.0.value");
    assert_eq!(msg, r#"value 1.2 is outside 0..1 for unit "rate""#);
    // A signed metric (eval_trend_15_35 = -8) is out of range for any unit.
    let mut input = sample();
    input.metrics[0].value = -8.0;
    issue_at(&input, "metrics.0.value");
    // low, high and bandMedian are checked against the unit too.
    let mut input = sample();
    input.metrics[2].band_median = Some(100_001.0); // seconds
    issue_at(&input, "metrics.2.bandMedian");
    assert_eq!(MetricUnit::Rate.max(), 1.0);
    assert_eq!(MetricUnit::PerGame.max(), 1000.0);
    assert_eq!(MetricUnit::Pct.max(), 100.0);
    assert_eq!(MetricUnit::Seconds.max(), 100_000.0);
    assert_eq!(MetricUnit::Elo.max(), 4000.0);
}

#[test]
fn posterior_outside_interval() {
    let mut input = sample();
    input.weaknesses[0].posterior_rate = 0.5; // interval 0.1..0.28
    assert_eq!(
        issue_at(&input, "weaknesses.0.posteriorRate"),
        "posteriorRate must lie within [intervalLow, intervalHigh]"
    );
    let mut input = sample();
    input.weaknesses[0].interval_low = 0.3;
    issue_at(&input, "weaknesses.0.intervalLow");
}

#[test]
fn events_above_opportunities() {
    let mut input = sample();
    let w = &mut input.weaknesses[0];
    w.events = w.opportunities + 1;
    assert_eq!(
        issue_at(&input, "weaknesses.0.events"),
        "events must be <= opportunities"
    );
}

#[test]
fn duplicate_rank() {
    let mut input = sample();
    input.weaknesses[1].rank = input.weaknesses[0].rank;
    assert_eq!(issue_at(&input, "weaknesses.1.rank"), "duplicate rank 1");
    assert!(issues(&input).iter().all(|i| i.path != "weaknesses.0.rank"));
}

#[test]
fn area_missing_or_duplicated() {
    let mut input = sample();
    input.area_scores.remove(0);
    let msg = issue_at(&input, "areaScores");
    assert!(msg.contains("exactly once"), "{msg}");
    let mut input = sample();
    input.area_scores[0] = input.area_scores[1].clone();
    issue_at(&input, "areaScores");
}

#[test]
fn onset_order() {
    let mut input = sample();
    let c = &mut input.changes[0];
    c.onset_date = c.onset_to.next_day().unwrap();
    assert_eq!(
        issue_at(&input, "changes.0.onsetDate"),
        "must satisfy onsetFrom <= onsetDate <= onsetTo"
    );
    let mut input = sample();
    let c = &mut input.changes[0];
    c.onset_from = c.onset_date.next_day().unwrap();
    issue_at(&input, "changes.0.onsetDate");
}

#[test]
fn analysed_above_imported() {
    let mut input = sample();
    input.basis.analysed_games = input.basis.imported_games + 1;
    assert_eq!(
        issue_at(&input, "basis.analysedGames"),
        "analysedGames must be <= importedGames"
    );
}

#[test]
fn window_start_after_end() {
    let mut input = sample();
    input.basis.window_start = date!(2026 - 11 - 20);
    input.basis.window_end = date!(2026 - 11 - 19);
    assert_eq!(
        issue_at(&input, "basis.windowStart"),
        "windowStart must be <= windowEnd"
    );
    // R-5: an empty window (start = end) is valid.
    let mut input = sample();
    input.basis.window_start = input.basis.window_end;
    assert_eq!(input.validate(), Ok(()));
}

#[test]
fn limits_exceeded() {
    let mut input = sample();
    let template = input.weaknesses[0].clone();
    input.weaknesses = (1..=25)
        .map(|rank| {
            let mut w = template.clone();
            w.rank = rank;
            w
        })
        .collect();
    assert_eq!(input.weaknesses.len(), LIMITS.weaknesses + 1);
    let msg = issue_at(&input, "weaknesses");
    assert!(msg.contains("<=24"), "{msg}");
    // The 25th rank is out of range as well; validate reports every issue, not the first.
    issue_at(&input, "weaknesses.24.rank");

    let mut input = sample();
    let ex = input.weaknesses[0].exemplars[0].clone();
    input.weaknesses[0].exemplars = vec![ex; LIMITS.exemplars_per_item + 1];
    issue_at(&input, "weaknesses.0.exemplars");

    let mut input = sample();
    input.weaknesses[0].exemplars[0].best_pv = vec!["e2e4".into(); LIMITS.pv_plies + 1];
    issue_at(&input, "weaknesses.0.exemplars.0.bestPv");
}

#[test]
fn reports_every_issue_not_just_the_first() {
    let mut input = sample();
    input.basis.analysed_games = input.basis.imported_games + 1;
    input.metrics[0].label = String::new();
    input.weaknesses[0].confidence = 1.2;
    input.changes[1].posterior_prob = -0.1;
    let paths: Vec<String> = issues(&input).into_iter().map(|i| i.path).collect();
    for p in [
        "basis.analysedGames",
        "metrics.0.label",
        "weaknesses.0.confidence",
        "changes.1.posteriorProb",
    ] {
        assert!(paths.iter().any(|x| x == p), "missing {p} in {paths:?}");
    }
}

#[test]
fn uci_pattern() {
    let mut input = sample();
    input.weaknesses[0].exemplars[0].played.uci = "e2e9".into();
    let msg = issue_at(&input, "weaknesses.0.exemplars.0.played.uci");
    assert!(msg.contains("pattern"), "{msg}");
    let mut input = sample();
    input.weaknesses[0].exemplars[0].refutation_pv[1] = "e7e8k".into();
    issue_at(&input, "weaknesses.0.exemplars.0.refutationPv.1");
    // Promotion to a lowercase piece is valid.
    let mut input = sample();
    input.weaknesses[0].exemplars[0].best_pv[0] = "e7e8q".into();
    assert_eq!(input.validate(), Ok(()));
}

#[test]
fn string_too_long() {
    let mut input = sample();
    input.weaknesses[0].label = "x".repeat(LIMITS.label_chars + 1);
    let msg = issue_at(&input, "weaknesses.0.label");
    assert!(msg.contains("<=120"), "{msg}");
    let mut input = sample();
    input.weaknesses[0].label = "x".repeat(LIMITS.label_chars);
    assert_eq!(input.validate(), Ok(()));
    // zod 4.6 counts Unicode code points (`util.codePointLength`), not UTF-16 units: 120
    // astral characters (240 UTF-16 units) fit, 121 do not.
    let mut input = sample();
    input.weaknesses[0].label = "\u{1F600}".repeat(LIMITS.label_chars);
    assert_eq!(input.validate(), Ok(()));
    let mut input = sample();
    input.weaknesses[0].label = "\u{1F600}".repeat(LIMITS.label_chars + 1);
    issue_at(&input, "weaknesses.0.label");
}

#[test]
fn other_bounds() {
    let mut input = sample();
    input.weaknesses[0].id = "W13".into();
    issue_at(&input, "weaknesses.0.id");
    let mut input = sample();
    input.rating = Some(4001);
    issue_at(&input, "rating");
    let mut input = sample();
    input.weaknesses[0].exemplars[0].eval_after = coaching_input::Score::Mate(501);
    issue_at(&input, "weaknesses.0.exemplars.0.evalAfter.mate");
    let mut input = sample();
    input.weaknesses[0].exemplars[0].win_before = f64::NAN;
    issue_at(&input, "weaknesses.0.exemplars.0.winBefore");
    let mut input = sample();
    input.metrics[0].value = f64::INFINITY;
    issue_at(&input, "metrics.0.value");
    let mut input = sample();
    input.changes[0].candidate_causes[0].before =
        coaching_input::CauseValue::Text("x".repeat(LIMITS.label_chars + 1));
    issue_at(&input, "changes.0.candidateCauses.0.before");
    let mut input = sample();
    input.account_id = uuid::Uuid::from_u128(1); // version 0: zod's uuid pattern rejects it
    issue_at(&input, "accountId");
    let mut input = sample();
    input.account_id = uuid::Uuid::nil();
    assert_eq!(input.validate(), Ok(()));
}

//! The trajectory's rating verdict is EXP-0030's decision (ADR 0117): stated again, from the rule
//! that cleared it only. EXP-0015 hid it (#136, ADR 0116); an input built before `algo-6.0.0`
//! names no rule, so its verdict stays withheld.

#[path = "support/v2.rs"]
mod v2;
#[path = "support/v3.rs"]
mod v3;

use coaching_input::trajectory::{
    CLEARED_RULE, RATING_VERDICT_SHOWN, RATING_VERDICT_SOURCE, player_visible, player_visible_input,
};
use coaching_input::{ChangeVerdict, CoachingInput, InputVersion};
use serde_json::Value;

fn exp_0030() -> Value {
    let path = format!(
        "{}/../../../lab/experiments/EXP-0030/results.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn v3_input() -> CoachingInput {
    let path = format!(
        "{}/../../fixtures/coaching-input/valid-sample.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let raw: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let mut i: CoachingInput = serde_json::from_value(raw).unwrap();
    i.input_version = InputVersion::V3;
    i.by_color = Some(v2::by_color());
    i.benchmark = Some(v2::benchmark());
    i.trajectory = Some(v3::trajectory());
    i
}

#[test]
fn the_switch_is_exp_0030s_decision() {
    let r = exp_0030();
    assert_eq!(RATING_VERDICT_SOURCE, "EXP-0030");
    assert_eq!(r["experiment"], "EXP-0030");
    let decision = r["decision"].as_str().expect("a decision");
    let shipped = decision.strip_prefix("ship ");
    assert!(
        shipped.is_some()
            || decision == "keep the verdict hidden"
            || decision.starts_with("need more data"),
        "unknown decision {decision:?}"
    );
    assert_eq!(
        RATING_VERDICT_SHOWN,
        shipped.is_some(),
        "EXP-0030 decided {decision:?}"
    );
    assert_eq!(
        shipped,
        Some(CLEARED_RULE),
        "the rule that ships is the one that cleared"
    );
}

#[test]
fn the_cleared_rule_is_stated_and_any_other_is_cut() {
    let mut cleared = v3::trajectory();
    cleared.rating_rule = Some(CLEARED_RULE.to_owned());
    assert_eq!(player_visible(cleared.clone()), cleared);
    for other in [None, Some("peak-bootstrap".to_owned())] {
        let mut t = v3::trajectory();
        t.rating_rule = other;
        let cut = player_visible(t);
        assert_eq!(cut.kind, None);
        assert_eq!(
            cut.then_vs_now.unwrap().rating.verdict,
            ChangeVerdict::Withheld
        );
    }
}

#[test]
fn a_hidden_verdict_keeps_the_numbers_and_drops_the_call() {
    let full = v3::trajectory();
    let t = player_visible(full.clone());
    assert_eq!(t.kind, None);
    let (a, b) = (full.then_vs_now.unwrap(), t.then_vs_now.unwrap());
    assert_eq!(b.rating.verdict, ChangeVerdict::Withheld);
    assert_eq!(b.rating.diff, a.rating.diff, "the difference is a fact");
    assert_eq!((b.rating.diff_low, b.rating.diff_high), (None, None));
    for (side, was) in [
        (&b.rating.then, &a.rating.then),
        (&b.rating.now, &a.rating.now),
    ] {
        assert_eq!(side.value, was.value);
        assert_eq!(side.n, was.n);
        assert_eq!((side.low, side.high), (None, None));
    }
    // Accuracy, blunders, score and the weaknesses keep their verdicts and ranges.
    assert_eq!(b.accuracy, a.accuracy);
    assert_eq!(b.blunders_per_game, a.blunders_per_game);
    assert_eq!(b.score, a.score);
    assert_eq!(b.weaknesses, a.weaknesses);
    assert_eq!(
        (t.peak, t.current, t.change, t.months),
        (full.peak, full.current, full.change, full.months)
    );
}

#[test]
fn a_hidden_input_still_validates_and_an_old_one_is_cut() {
    let stored = v3_input();
    assert_eq!(stored.validate(), Ok(()), "an input stored before 5.0.0");
    let cut = player_visible_input(stored);
    assert_eq!(cut.validate(), Ok(()));
    let t = cut.trajectory.as_ref().unwrap();
    assert_eq!(t.kind, None);
    let json = cut.to_json();
    assert_eq!(
        json["trajectory"]["thenVsNow"]["rating"]["verdict"],
        "withheld"
    );
    assert!(json["trajectory"]["thenVsNow"]["rating"]["diffLow"].is_null());
    let errors: Vec<String> = jsonschema::options()
        .should_validate_formats(true)
        .build(&coaching_input::json_schema())
        .unwrap()
        .iter_errors(&json)
        .map(|e| e.to_string())
        .collect();
    assert!(errors.is_empty(), "{errors:#?}");
    let short = player_visible(v3::short());
    assert_eq!(short, v3::short(), "nothing to cut below the minimum");
}

#[test]
fn only_the_rating_can_be_withheld_and_it_drops_its_interval() {
    let paths = |i: &CoachingInput| -> Vec<String> {
        i.validate()
            .err()
            .unwrap_or_default()
            .into_iter()
            .map(|x| x.path)
            .collect()
    };
    let mut i = player_visible_input(v3_input());
    let v = i.trajectory.as_mut().unwrap().then_vs_now.as_mut().unwrap();
    v.accuracy.verdict = ChangeVerdict::Withheld;
    v.rating.diff_low = Some(-10.0);
    let p = paths(&i);
    assert!(
        p.contains(&"trajectory.thenVsNow.accuracy.verdict".to_owned()),
        "{p:?}"
    );
    assert!(
        p.contains(&"trajectory.thenVsNow.rating.diffLow".to_owned()),
        "{p:?}"
    );
    let mut i = player_visible_input(v3_input());
    let v = i.trajectory.as_mut().unwrap().then_vs_now.as_mut().unwrap();
    v.rating.diff = None;
    assert_eq!(
        paths(&i),
        vec!["trajectory.thenVsNow.rating.diff".to_owned()]
    );
    let mut i = player_visible_input(v3_input());
    let v = i.trajectory.as_mut().unwrap().then_vs_now.as_mut().unwrap();
    v.weaknesses[0].verdict = ChangeVerdict::Withheld;
    (v.rating.now.low, v.rating.now.high) = (Some(830.0), Some(850.0));
    assert_eq!(
        paths(&i),
        vec![
            "trajectory.thenVsNow.rating.now.low".to_owned(),
            "trajectory.thenVsNow.rating.now.high".to_owned(),
            "trajectory.thenVsNow.weaknesses.0.verdict".to_owned(),
        ]
    );
}

#[test]
fn the_cut_is_all_or_nothing() {
    let paths = |i: &CoachingInput| -> Vec<String> {
        i.validate()
            .err()
            .unwrap_or_default()
            .into_iter()
            .map(|x| x.path)
            .collect()
    };
    // A withheld rating with a `kind` left on.
    let mut i = player_visible_input(v3_input());
    i.trajectory.as_mut().unwrap().kind = v3::trajectory().kind;
    assert_eq!(paths(&i), vec!["trajectory.kind".to_owned()]);
    // No `kind` with a called rating.
    let mut i = v3_input();
    i.trajectory.as_mut().unwrap().kind = None;
    assert_eq!(paths(&i), vec!["trajectory.kind".to_owned()]);
}

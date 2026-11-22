//! Port of TS plan B4's `weekly.test.ts` (R3 Task 15): the weekly review on the lighter model,
//! validated item by item and repaired once. Fake client only.

#[path = "support/generate.rs"]
mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use ai::client::fake::{FakeAiClient, FakeScript};
use ai::client::model::{Effort, Models};
use ai::client::port::{AiError, AiUsage, UnavailableReason};
use ai::generate::request::rules_block;
use ai::generate::weekly::{
    WEEKLY_MAX_TOKENS, build_weekly_evidence, build_weekly_request, generate_weekly_review,
    validate_weekly,
};
use ai::output::codes::ValidationCode;
use ai::output::weekly::{WeeklyReviewInput, WeeklyReviewOutput};
use ai::prompts::load_prompt;
use ai::testing::load_fixture;
use serde_json::{Value, json};
use support::{no_budget, out, stop, structured_requests};

/// A week for the b2-rapid player: one focus (W1, `pImp` 0.6, so "unclear yet"), one metric
/// delta, one exemplar and b2-rapid's change.
fn weekly_json() -> Value {
    let b2 = load_fixture("b2-rapid");
    json!({
        "inputVersion": "weekly-input-v1",
        "timeClass": b2["timeClass"],
        "band": b2["band"],
        "weekStart": "2026-11-16",
        "weekEnd": "2026-11-22",
        "gamesThisWeek": 14,
        "basis": b2["basis"],
        "adherence": { "minutesDone": 90, "minutesPlanned": 120 },
        "metricDeltas": [{
            "key": "blunder_rate", "area": "tactics", "label": "Blunders",
            "before": { "value": 0.2, "n": 40, "low": 0.15, "high": 0.25 },
            "after": { "value": 0.1, "n": 14, "low": null, "high": null },
            "verdict": "no_clear_change"
        }],
        "focusUpdates": [{ "weaknessId": "W1", "label": "Hanging pieces", "pImp": 0.6, "state": "active" }],
        "exemplars": [b2["weaknesses"][0]["exemplars"][0]],
        "changes": b2["changes"],
    })
}

fn weekly() -> WeeklyReviewInput {
    serde_json::from_value(weekly_json()).unwrap()
}

fn item(text: &str) -> Value {
    json!({ "text": text, "citations": [] })
}

const GOOD_OUTCOME: &str =
    "Whether the work on hanging pieces shows in your results is unclear yet.";
const WRONG_OUTCOME: &str =
    "It is very likely that the work on hanging pieces shows in your results.";

/// A weekly review where every item passes, with `outcome` as its only outcome.
fn review(outcome: &str) -> Value {
    json!({
        "headline": item("A steady week of practice."),
        "process": item("You put in {{num:adherence.minutesDone|int}} minutes of practice this week."),
        "outcomes": [item(outcome)],
        "exemplars": [{
            "text": "In this game {{move:li-w1-0:5}} left a piece loose.",
            "citations": [{ "gameId": "li-w1-0", "ply": 5 }],
        }],
        "nextWeek": item("Next week, look for loose pieces before you move."),
    })
}

#[tokio::test]
async fn weekly_uses_haiku_with_a_cached_system_block() {
    let input = weekly();
    let req = build_weekly_request(&input);
    assert_eq!(req.model, Models::WEEKLY);
    assert_eq!(req.model.as_str(), "claude-haiku-5-5");
    assert_eq!(req.effort, Effort::Low);
    assert_eq!(req.max_tokens, WEEKLY_MAX_TOKENS);
    assert_eq!(WEEKLY_MAX_TOKENS, 4000);
    assert_eq!(req.schema.id, "weekly-review");

    // system[0] is the shared rules block, byte for byte; system[1] the weekly prompt, cached.
    assert_eq!(req.system.len(), 2);
    assert_eq!(req.system[0], rules_block());
    assert!(req.system[1].cache);
    let weekly_system = load_prompt("weekly-review/system", 1).unwrap().text;
    assert!(req.system[1].text.starts_with(weekly_system));
    assert!(
        !req.system[1]
            .text
            .contains(load_prompt("core/tokens", 1).unwrap().text),
        "the token grammar is never restated"
    );

    // The user message: the cached weekly input, then the uncached task.
    assert_eq!(req.messages.len(), 1);
    let blocks = &req.messages[0].content;
    assert_eq!(blocks.len(), 2);
    let data = blocks[0]["text"].as_str().unwrap();
    assert!(data.starts_with("<weekly_input>") && data.ends_with("</weekly_input>"));
    assert_eq!(blocks[0]["cache_control"], json!({"type": "ephemeral"}));
    assert!(blocks[1].get("cache_control").is_none());
    assert_eq!(
        blocks[1]["text"].as_str().unwrap(),
        load_prompt("weekly-review/task", 1).unwrap().text
    );
    // The focus carries its likelihood phrase, and the refs list the weekly paths.
    assert!(
        data.contains("\"likelihoodPhrase\":\"unclear yet\""),
        "{data}"
    );
    assert!(
        data.contains("metricDeltas.blunder_rate.after.value"),
        "{data}"
    );
    assert!(data.contains("adherence.minutesDone"), "{data}");

    // The fake sees the haiku request; a valid review is ready on the first call.
    let ai = FakeAiClient::new(FakeScript {
        structured: Some(Box::new(|_, _| Ok(out(review(GOOD_OUTCOME))))),
        ..FakeScript::default()
    });
    let budget = no_budget();
    let o = generate_weekly_review(&ai, &budget, &input).await.unwrap();
    let reqs = structured_requests(&ai);
    assert_eq!(reqs.len(), 1);
    assert_eq!(reqs[0]["model"], "claude-haiku-5-5");
    assert_eq!(reqs[0]["effort"], "low");
    assert_eq!(reqs[0]["max_tokens"], 4000);
    assert_eq!(
        reqs[0]["system"][0]["cache_control"],
        json!({"type": "ephemeral"})
    );
    assert!(o.dropped.is_empty(), "{:?}", o.dropped);
    assert!(o.content.headline.is_some());
    assert!(o.content.process.is_some());
    assert_eq!(o.content.outcomes.len(), 1);
    assert_eq!(o.content.exemplars.len(), 1);
    assert!(o.content.next_week.is_some());
    assert_eq!(o.calls.len(), 1);
    assert_eq!(o.calls[0].model, "claude-haiku-5-5");
}

#[tokio::test]
async fn outcome_with_wrong_likelihood_is_repaired_once_then_dropped() {
    let input = weekly();
    let ai = FakeAiClient::new(FakeScript {
        structured: Some(Box::new(|_, _| Ok(out(review(WRONG_OUTCOME))))),
        ..FakeScript::default()
    });
    let budget = no_budget();
    let o = generate_weekly_review(&ai, &budget, &input).await.unwrap();
    let reqs = structured_requests(&ai);
    assert_eq!(reqs.len(), 2, "one repair, no more");
    // The repair replays the answer and lists the error with its path.
    let msgs = reqs[1]["messages"].as_array().unwrap();
    assert_eq!(msgs.len(), 3);
    assert_eq!(msgs[1]["role"], "assistant");
    let repair = msgs[2]["content"][0]["text"].as_str().unwrap();
    assert!(
        repair.contains("- outcomes[0].text: likelihood_mismatch: very likely"),
        "{repair}"
    );
    // The outcome is dropped; everything else stays.
    assert!(o.content.outcomes.is_empty());
    assert!(o.content.headline.is_some() && o.content.process.is_some());
    assert_eq!(o.content.exemplars.len(), 1);
    assert_eq!(o.dropped.len(), 1);
    assert_eq!(o.dropped[0].section, "weekly");
    assert_eq!(o.dropped[0].path, "outcomes[0]");
    assert!(
        o.dropped[0]
            .errors
            .iter()
            .any(|e| e.code == ValidationCode::LikelihoodMismatch && e.detail == "very likely"),
        "{:?}",
        o.dropped
    );
    // Both calls are metered.
    assert_eq!(o.calls.len(), 2);
}

#[test]
fn weekly_process_quoting_an_outcome_number_is_rejected() {
    let input = weekly();
    let ev = build_weekly_evidence(&input);
    let quoted = "{{num:metricDeltas.blunder_rate.after.value|dec1}}";
    let mut v = review(GOOD_OUTCOME);
    v["process"] = item(&format!(
        "You practised well, and the rate is now {quoted}."
    ));
    v["outcomes"] = json!([item(&format!(
        "The rate is now {quoted}, and whether it holds is unclear yet."
    ))]);
    let out: WeeklyReviewOutput = serde_json::from_value(v).unwrap();
    let (content, dropped) = validate_weekly(&out, &input, &ev);
    assert!(content.process.is_none());
    assert_eq!(dropped.len(), 1, "{dropped:?}");
    assert_eq!(dropped[0].path, "process");
    assert_eq!(dropped[0].errors.len(), 1, "{dropped:?}");
    let e = &dropped[0].errors[0];
    assert_eq!(e.code, ValidationCode::LikelihoodMismatch);
    assert_eq!(e.path, "process.text");
    assert_eq!(e.detail, "process quotes an outcome number");
    // The same number is fine in an outcome.
    assert_eq!(content.outcomes.len(), 1);
    // A process number that is not an outcome (the practice minutes) is fine.
    let ok: WeeklyReviewOutput = serde_json::from_value(review(GOOD_OUTCOME)).unwrap();
    let (content, dropped) = validate_weekly(&ok, &input, &ev);
    assert!(dropped.is_empty(), "{dropped:?}");
    assert!(content.process.is_some());
}

#[test]
fn every_weekly_item_gets_the_banned_phrase_and_marketing_rules() {
    let input = weekly();
    let ev = build_weekly_evidence(&input);
    let mut v = review(GOOD_OUTCOME);
    v["headline"] = item("This plan is guaranteed to work.");
    v["nextWeek"] = item("Upgrade for more drills next week.");
    v["exemplars"][0]["text"] = json!("In this game {{move:li-w1-0:5}} you definitely slipped.");
    let out: WeeklyReviewOutput = serde_json::from_value(v).unwrap();
    let (content, dropped) = validate_weekly(&out, &input, &ev);
    assert!(content.headline.is_none() && content.next_week.is_none());
    assert!(content.exemplars.is_empty());
    let codes: Vec<(String, ValidationCode)> = dropped
        .iter()
        .flat_map(|d| d.errors.iter().map(|e| (d.path.clone(), e.code)))
        .collect();
    assert!(codes.contains(&("headline".into(), ValidationCode::BannedPhrase)));
    assert!(codes.contains(&("nextWeek".into(), ValidationCode::Marketing)));
    assert!(codes.contains(&("exemplars[0]".into(), ValidationCode::BannedPhrase)));
}

#[test]
fn an_outcome_past_the_focuses_has_no_likelihood_to_use() {
    let input = weekly();
    let ev = build_weekly_evidence(&input);
    let mut v = review(GOOD_OUTCOME);
    // outcomes[1] has no matching focus: any likelihood phrase is a mismatch.
    v["outcomes"] = json!([item(GOOD_OUTCOME), item(GOOD_OUTCOME)]);
    let out: WeeklyReviewOutput = serde_json::from_value(v).unwrap();
    let (content, dropped) = validate_weekly(&out, &input, &ev);
    assert_eq!(content.outcomes.len(), 1);
    assert_eq!(dropped.len(), 1);
    assert_eq!(dropped[0].path, "outcomes[1]");
    assert_eq!(
        dropped[0].errors[0].code,
        ValidationCode::LikelihoodMismatch
    );
}

#[test]
fn an_exemplar_item_cites_only_the_weekly_exemplars() {
    let input = weekly();
    let ev = build_weekly_evidence(&input);
    let mut v = review(GOOD_OUTCOME);
    // li-c1-0#21 is a change example: known, but not a weekly exemplar.
    v["exemplars"] = json!([{
        "text": "In this game {{move:li-c1-0:21}} left a piece loose.",
        "citations": [{ "gameId": "li-c1-0", "ply": 21 }],
    }, {
        "text": "This game shows a loose piece.",
        "citations": [],
    }]);
    let out: WeeklyReviewOutput = serde_json::from_value(v).unwrap();
    let (content, dropped) = validate_weekly(&out, &input, &ev);
    assert!(content.exemplars.is_empty());
    let codes: Vec<ValidationCode> = dropped
        .iter()
        .flat_map(|d| d.errors.iter().map(|e| e.code))
        .collect();
    assert!(
        codes.contains(&ValidationCode::UnrelatedCitation),
        "{dropped:?}"
    );
    assert!(
        codes.contains(&ValidationCode::MissingCitation),
        "{dropped:?}"
    );
}

#[test]
fn weekly_evidence_holds_the_exemplars_changes_and_paths() {
    let input = weekly();
    let ev = build_weekly_evidence(&input);
    assert!(ev.positions.contains_key("li-w1-0#5"));
    assert!(ev.positions.contains_key("li-c1-0#21"));
    assert!(
        !ev.positions.contains_key("li-w3-0#16"),
        "only the week's positions"
    );
    for p in [
        "gamesThisWeek",
        "adherence.minutesDone",
        "metricDeltas.blunder_rate.after.value",
        "focusUpdates.W1.pImp",
        "changes.tactics_blunder_rate.ratioAfterBefore",
        "basis.analysedGames",
    ] {
        assert!(ev.number_paths.contains_key(p), "{p}");
    }
    assert!(ev.date_paths.contains_key("weekStart"));
    assert!(
        ev.date_paths
            .contains_key("changes.tactics_blunder_rate.onsetDate")
    );
    assert!(ev.rate_paths.contains("focusUpdates.W1.pImp"));
}

#[tokio::test]
async fn a_refusal_twice_gives_an_empty_review_with_both_calls_metered() {
    let input = weekly();
    let ai = FakeAiClient::new(FakeScript {
        structured: Some(Box::new(|_, _| Ok(stop("refusal")))),
        ..FakeScript::default()
    });
    let budget = no_budget();
    let o = generate_weekly_review(&ai, &budget, &input).await.unwrap();
    assert_eq!(structured_requests(&ai).len(), 2);
    assert!(o.content.headline.is_none() && o.content.process.is_none());
    assert!(o.content.outcomes.is_empty() && o.content.exemplars.is_empty());
    assert!(o.content.next_week.is_none());
    assert_eq!(o.dropped.len(), 1);
    assert_eq!(o.dropped[0].path, "");
    assert_eq!(o.dropped[0].errors[0].code, ValidationCode::Refusal);
    assert_eq!(o.calls.len(), 2);
}

#[tokio::test]
async fn a_failed_repair_hands_the_paid_first_call_back() {
    let input = weekly();
    let n = Arc::new(AtomicUsize::new(0));
    let seen = n.clone();
    let ai = FakeAiClient::new(FakeScript {
        structured: Some(Box::new(move |_, _| {
            if seen.fetch_add(1, Ordering::SeqCst) == 0 {
                let mut o = out(review(WRONG_OUTCOME));
                o.usage = AiUsage {
                    input_tokens: 100,
                    output_tokens: 50,
                    ..AiUsage::default()
                };
                Ok(o)
            } else {
                Err(AiError::Unavailable {
                    reason: UnavailableReason::Overloaded,
                    retry_after: None,
                })
            }
        })),
        ..FakeScript::default()
    });
    let budget = no_budget();
    let e = generate_weekly_review(&ai, &budget, &input)
        .await
        .unwrap_err();
    assert!(e.error.is_unavailable());
    assert_eq!(e.calls.len(), 1, "attempt 1 was paid for");
    assert_eq!(e.calls[0].usage.input_tokens, 100);
}

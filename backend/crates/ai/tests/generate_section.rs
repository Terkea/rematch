//! Port of TS plan B3's `section.test.ts` (R3 Task 14): one section, attempt, validate, repair
//! once, drop. Fake client only.

#[path = "support/generate.rs"]
mod support;

use ai::client::port::{AiError, UnavailableReason};
use ai::generate::computed::build_computed_report;
use ai::generate::section::{SectionArgs, SectionStatus, generate_section};
use ai::input::types::CoachingInput;
use ai::output::codes::{SectionKey, ValidationCode};
use ai::testing::input;
use ai::validator::evidence::{Evidence, build_evidence};
use ai::validator::focus::select_focuses;
use serde_json::{Value, json};
use support::*;

struct World {
    input: CoachingInput,
    evidence: Evidence,
    focuses: Vec<String>,
}

fn world(name: &str) -> World {
    let input = input(name);
    let evidence = build_evidence(&input);
    let focuses = select_focuses(&input, true);
    World {
        input,
        evidence,
        focuses,
    }
}

impl World {
    fn args(&self, section: SectionKey) -> SectionArgs<'_> {
        SectionArgs {
            input: &self.input,
            evidence: &self.evidence,
            focuses: &self.focuses,
            section,
            is_first_report: true,
        }
    }
}

fn mixed_weaknesses() -> Value {
    envelope(
        SectionKey::Weaknesses,
        json!({"items": [
            w1_item(&good_w1_text()),
            {"text": LEAKING_TEXT, "citations": [{"gameId": "li-w3-1", "ply": 18}], "weaknessId": "W3", "title": "Forks"},
        ]}),
    )
}

#[tokio::test]
async fn valid_first_attempt() {
    let w = world("b2-rapid");
    let ai = per_section(&w.input, |k, _| Ok(out(valid(k))));
    let budget = no_budget();
    let o = generate_section(&ai, &budget, w.args(SectionKey::Weaknesses))
        .await
        .unwrap();
    assert_eq!(structured_requests(&ai).len(), 1);
    assert_eq!(o.section, SectionKey::Weaknesses);
    assert_eq!(o.status, SectionStatus::Ready);
    assert_eq!(o.attempts, 1);
    assert!(o.dropped.is_empty());
    let content = o.content.expect("content");
    assert_eq!(content.key(), SectionKey::Weaknesses);
    assert_eq!(content.weaknesses().unwrap().items.len(), 1);
    assert_eq!(o.calls.len(), 1);
    assert_eq!(o.calls[0].model, "claude-sonnet-5-5");
    assert!(!o.calls[0].batch);
}

#[tokio::test]
async fn bad_then_good() {
    let w = world("b2-rapid");
    let bad = envelope(
        SectionKey::Weaknesses,
        json!({"items": [{"text": LEAKING_TEXT, "citations": [{"gameId": "li-w1-0", "ply": 5}], "weaknessId": "W1", "title": "t"}]}),
    );
    let bad_text = bad.to_string();
    let ai = per_section(&w.input, move |k, n| {
        Ok(out(if n == 1 { bad.clone() } else { valid(k) }))
    });
    let budget = no_budget();
    let o = generate_section(&ai, &budget, w.args(SectionKey::Weaknesses))
        .await
        .unwrap();
    assert_eq!(o.status, SectionStatus::Ready);
    assert_eq!(o.attempts, 2);
    assert!(o.dropped.is_empty());
    assert_eq!(o.calls.len(), 2);

    let reqs = structured_requests(&ai);
    assert_eq!(reqs.len(), 2);
    let first = reqs[0]["messages"].as_array().unwrap();
    let second = reqs[1]["messages"].as_array().unwrap();
    // The repair request is the first request plus the raw assistant content and a user turn.
    assert_eq!(second.len(), first.len() + 2);
    assert_eq!(&second[..first.len()], &first[..]);
    let assistant = &second[first.len()];
    assert_eq!(assistant["role"], "assistant");
    assert_eq!(
        assistant["content"],
        json!([{"type": "text", "text": bad_text}])
    );
    let repair = &second[first.len() + 1];
    assert_eq!(repair["role"], "user");
    let repair_text = repair["content"][0]["text"].as_str().unwrap();
    assert!(repair_text.contains("free_text_fact"), "{repair_text}");
    assert!(
        repair_text.contains("- weaknesses.items[0].text: free_text_fact:"),
        "{repair_text}"
    );
}

#[tokio::test]
async fn bad_then_bad_keeps_valid_items_and_lists_dropped_paths() {
    let w = world("b2-rapid");
    let ai = per_section(&w.input, |_, _| Ok(out(mixed_weaknesses())));
    let budget = no_budget();
    let o = generate_section(&ai, &budget, w.args(SectionKey::Weaknesses))
        .await
        .unwrap();
    assert_eq!(structured_requests(&ai).len(), 2);
    assert_eq!(o.status, SectionStatus::Ready);
    assert_eq!(o.attempts, 2);
    let content = o.content.expect("content");
    let items = &content.weaknesses().unwrap().items;
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].weakness_id, "W1");
    assert_eq!(o.dropped.len(), 1);
    assert_eq!(o.dropped[0].section, "weaknesses");
    assert_eq!(o.dropped[0].path, "weaknesses.items[1]");
    assert!(
        o.dropped[0]
            .errors
            .iter()
            .any(|e| e.code == ValidationCode::FreeTextFact)
    );
}

#[tokio::test]
async fn refusal_then_truncation_drops_section_without_throwing() {
    let w = world("b2-rapid");
    let ai = per_section(&w.input, |_, n| {
        Ok(stop(if n == 1 { "refusal" } else { "max_tokens" }))
    });
    let budget = no_budget();
    let o = generate_section(&ai, &budget, w.args(SectionKey::Overview))
        .await
        .unwrap();
    assert_eq!(structured_requests(&ai).len(), 2);
    assert_eq!(o.status, SectionStatus::Dropped);
    assert_eq!(o.content, None);
    assert_eq!(o.attempts, 2);
    assert_eq!(o.calls.len(), 2);
    assert_eq!(o.dropped.len(), 1);
    assert_eq!(o.dropped[0].section, "overview");
    assert_eq!(o.dropped[0].path, "");
    assert_eq!(o.dropped[0].errors[0].code, ValidationCode::Truncated);
    // A refusal has no assistant content to replay: attempt 2 resends the section request
    // instead of an empty assistant turn.
    let reqs = structured_requests(&ai);
    assert_eq!(reqs[0]["messages"], reqs[1]["messages"]);
}

#[tokio::test]
async fn schema_mismatch_is_repaired_with_the_issue_paths() {
    let w = world("b2-rapid");
    let ai = per_section(&w.input, |k, n| {
        Ok(if n == 1 {
            out(json!({"section": "overview"}))
        } else {
            out(valid(k))
        })
    });
    let budget = no_budget();
    let o = generate_section(&ai, &budget, w.args(SectionKey::Overview))
        .await
        .unwrap();
    assert_eq!(o.status, SectionStatus::Ready);
    assert_eq!(o.attempts, 2);
    let reqs = structured_requests(&ai);
    let msgs = reqs[1]["messages"].as_array().unwrap();
    let repair = msgs.last().unwrap()["content"][0]["text"].as_str().unwrap();
    assert!(repair.contains("schema_invalid"), "{repair}");
}

#[tokio::test]
async fn wrong_section_twice_is_dropped() {
    let w = world("b2-rapid");
    let ai = per_section(&w.input, |_, _| Ok(out(valid(SectionKey::Weaknesses))));
    let budget = no_budget();
    let o = generate_section(&ai, &budget, w.args(SectionKey::Overview))
        .await
        .unwrap();
    assert_eq!(structured_requests(&ai).len(), 2);
    assert_eq!(o.status, SectionStatus::Dropped);
    assert_eq!(o.content, None);
    assert_eq!(o.dropped.len(), 1);
    assert_eq!(o.dropped[0].errors[0].code, ValidationCode::WrongSection);
    let reqs = structured_requests(&ai);
    let repair = reqs[1]["messages"].as_array().unwrap().last().unwrap()["content"][0]["text"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(repair.contains("- section: wrong_section:"), "{repair}");
}

fn early_areas(with_token: bool) -> Value {
    let insufficient = |area: &str| {
        if with_token {
            format!(
                "We need {{{{num:areaScores.{area}.needMore|int}}}} more games here to judge this."
            )
        } else {
            "We need more games here to judge this.".to_owned()
        }
    };
    let ok = |area: &str| json!({"text": "This part of your play looks sound.", "citations": [], "area": area});
    let thin = |area: &str| json!({"text": insufficient(area), "citations": [], "area": area});
    envelope(
        SectionKey::Areas,
        json!({"items": [
            ok("openings"), ok("middlegame"), thin("endgame"), ok("tactics"),
            thin("calculation"), ok("time"), thin("mindset"),
        ]}),
    )
}

#[tokio::test]
async fn early_read_input_yields_general_focus_and_need_more_area_texts() {
    let w = world("early-read");
    assert!(build_computed_report(&w.input, true).focuses.is_empty());

    // The climb-plan request context names the general focus.
    let ai = per_section(&w.input, |_, _| Ok(stop("refusal")));
    let budget = no_budget();
    generate_section(&ai, &budget, w.args(SectionKey::ClimbPlan))
        .await
        .unwrap();
    let reqs = structured_requests(&ai);
    let ctx = reqs[0]["messages"][0]["content"][0]["text"]
        .as_str()
        .unwrap();
    assert!(ctx.contains(r#""focuses":["general"]"#), "{ctx}");

    // Insufficient areas without the needMore token are dropped (after the repair) ...
    let ai = per_section(&w.input, |_, _| Ok(out(early_areas(false))));
    let o = generate_section(&ai, &budget, w.args(SectionKey::Areas))
        .await
        .unwrap();
    assert_eq!(o.status, SectionStatus::Ready);
    assert_eq!(o.attempts, 2);
    let paths: Vec<&str> = o.dropped.iter().map(|d| d.path.as_str()).collect();
    assert_eq!(
        paths,
        ["areas.items[2]", "areas.items[4]", "areas.items[6]"]
    );
    assert!(
        o.dropped
            .iter()
            .all(|d| d.errors[0].code == ValidationCode::InsufficientDataNotStated)
    );
    assert_eq!(o.content.unwrap().areas().unwrap().items.len(), 4);

    // ... and kept with it.
    let ai = per_section(&w.input, |_, _| Ok(out(early_areas(true))));
    let o = generate_section(&ai, &budget, w.args(SectionKey::Areas))
        .await
        .unwrap();
    assert_eq!(o.status, SectionStatus::Ready);
    assert_eq!(o.attempts, 1, "{:?}", o.dropped);
    assert_eq!(o.content.unwrap().areas().unwrap().items.len(), 7);
}

#[tokio::test]
async fn unavailable_and_budget_propagate() {
    let w = world("b2-rapid");
    let unavailable = AiError::Unavailable {
        reason: UnavailableReason::Overloaded,
        retry_after: None,
    };
    let e = unavailable.clone();
    let ai = per_section(&w.input, move |_, _| Err(e.clone()));
    let budget = no_budget();
    let r = generate_section(&ai, &budget, w.args(SectionKey::Overview)).await;
    assert_eq!(r.unwrap_err(), unavailable);

    // Unavailable on the repair call propagates too (nothing is dropped on an outage).
    let e = unavailable.clone();
    let ai = per_section(&w.input, move |_, n| {
        if n == 1 {
            Ok(out(mixed_weaknesses()))
        } else {
            Err(e.clone())
        }
    });
    let r = generate_section(&ai, &budget, w.args(SectionKey::Weaknesses)).await;
    assert_eq!(r.unwrap_err(), unavailable);

    // The budget hook runs before the call: no call is made.
    let ai = per_section(&w.input, |k, _| Ok(out(valid(k))));
    let over: BudgetFn = Box::new(|| Box::pin(async { Err(AiError::Budget) }));
    let r = generate_section(&ai, &over, w.args(SectionKey::Overview)).await;
    assert_eq!(r.unwrap_err(), AiError::Budget);
    assert!(structured_requests(&ai).is_empty());
}

//! Port of TS plan B4's `weakness-change.test.ts` (R3 Task 15): a short, hedged note on one
//! timeline change, on the lighter model, validated against the change's own evidence only.
//! Fake client only.

#[path = "support/generate.rs"]
mod support;

use ai::client::fake::{FakeAiClient, FakeScript};
use ai::client::model::{Effort, Models};
use ai::generate::change::{
    CHANGE_MAX_TOKENS, build_change_evidence, build_change_request, explain_weakness_change,
};
use ai::generate::request::rules_block;
use ai::generate::section::SectionStatus;
use ai::output::change::WeaknessChangeInput;
use ai::output::codes::{Citation, ValidationCode};
use ai::prompts::load_prompt;
use ai::segments::Segment;
use ai::testing::load_fixture;
use serde_json::{Value, json};
use support::{no_budget, out, structured_requests};

/// b2-rapid's change: `tactics.blunder_rate` (key `tactics_blunder_rate`), 0.22 → 0.14, with
/// examples li-c1-0#21, li-c1-1#23 (before) and li-c1-2#25, li-c1-3#27 (after).
fn change_input() -> WeaknessChangeInput {
    let b2 = load_fixture("b2-rapid");
    serde_json::from_value(json!({
        "change": b2["changes"][0],
        "band": b2["band"],
        "timeClass": b2["timeClass"],
        "basis": b2["basis"],
    }))
    .unwrap()
}

const K: &str = "tactics_blunder_rate";

fn hedged_text() -> String {
    format!(
        "Since about {{{{date:changes.{K}.onsetDate}}}} you may be missing back-rank threats about {{{{num:changes.{K}.ratioAfterBefore|dec1}}}} times as often, around the time you switched to blitz."
    )
}

fn note(text: &str, cites: &[(&str, u32)]) -> Value {
    let citations: Vec<Value> = cites
        .iter()
        .map(|(g, p)| json!({"gameId": g, "ply": p}))
        .collect();
    json!({ "text": text, "citations": citations })
}

fn two_cites() -> Vec<(&'static str, u32)> {
    vec![("li-c1-0", 21), ("li-c1-2", 25)]
}

fn fake(f: impl Fn(usize) -> Value + Send + Sync + 'static) -> FakeAiClient {
    let n = std::sync::atomic::AtomicUsize::new(0);
    FakeAiClient::new(FakeScript {
        structured: Some(Box::new(move |_, _| {
            Ok(out(f(
                n.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1
            )))
        })),
        ..FakeScript::default()
    })
}

#[test]
fn change_request_uses_haiku_low_effort_and_the_shared_rules_block() {
    let input = change_input();
    let req = build_change_request(&input);
    assert_eq!(req.model, Models::EXPLAIN);
    assert_eq!(req.model.as_str(), "claude-haiku-5-5");
    assert_eq!(req.effort, Effort::Low);
    assert_eq!(req.max_tokens, CHANGE_MAX_TOKENS);
    assert_eq!(CHANGE_MAX_TOKENS, 2000);
    assert_eq!(req.schema.id, "weakness-change");
    assert_eq!(req.system[0], rules_block());
    assert!(req.system[1].cache);
    let system = load_prompt("weakness-change/system", 1).unwrap().text;
    assert!(req.system[1].text.starts_with(system));
    // The change prompt names correlated changes as possible causes, never proven ones.
    assert!(system.contains("possible causes"), "{system}");
    let blocks = &req.messages[0].content;
    let data = blocks[0]["text"].as_str().unwrap();
    assert!(data.starts_with("<change_input>") && data.ends_with("</change_input>"));
    assert_eq!(blocks[0]["cache_control"], json!({"type": "ephemeral"}));
    assert!(
        data.contains(&format!("changes.{K}.ratioAfterBefore")),
        "{data}"
    );
    // Only the change's own paths are offered: no basis number can be cited.
    assert!(!data.contains("\"basis.analysedGames\""), "{data}");
}

#[test]
fn change_evidence_is_the_change_only() {
    let ev = build_change_evidence(&change_input());
    let mut keys: Vec<&String> = ev.positions.keys().collect();
    keys.sort();
    assert_eq!(
        keys,
        ["li-c1-0#21", "li-c1-1#23", "li-c1-2#25", "li-c1-3#27"]
    );
    assert!(
        ev.number_paths
            .contains_key(&format!("changes.{K}.ratioAfterBefore"))
    );
    assert!(
        ev.date_paths
            .contains_key(&format!("changes.{K}.onsetDate"))
    );
    assert!(!ev.number_paths.keys().any(|p| p.starts_with("basis.")));
    assert!(ev.rate_paths.contains(&format!("changes.{K}.rateAfter")));
}

#[tokio::test]
async fn change_note_with_hedge_and_two_citations_is_ready() {
    let input = change_input();
    let ai = fake(|_| note(&hedged_text(), &two_cites()));
    let budget = no_budget();
    let o = explain_weakness_change(&ai, &budget, &input).await.unwrap();
    assert_eq!(structured_requests(&ai).len(), 1);
    assert_eq!(o.status, SectionStatus::Ready, "{:?}", o.dropped);
    assert_eq!(o.text.as_deref(), Some(hedged_text().as_str()));
    assert_eq!(
        o.citations,
        vec![
            Citation {
                game_id: "li-c1-0".into(),
                ply: 21
            },
            Citation {
                game_id: "li-c1-2".into(),
                ply: 25
            },
        ]
    );
    let segments = o.segments.expect("segments");
    let rendered: String = segments
        .iter()
        .map(|s| match s {
            Segment::Text { text } => text.clone(),
            Segment::Date { date, .. } => date.clone(),
            Segment::Num { display, .. } => display.clone(),
            other => panic!("unexpected segment {other:?}"),
        })
        .collect();
    assert!(rendered.contains("0.6 times as often"), "{rendered}");
    assert!(!rendered.contains("{{"), "{rendered}");
    assert_eq!(o.calls.len(), 1);
    assert_eq!(o.calls[0].model, "claude-haiku-5-5");
}

#[tokio::test]
async fn causal_text_twice_is_dropped_with_none_text() {
    let input = change_input();
    let causal = format!(
        "Since {{{{date:changes.{K}.onsetDate}}}} you miss back-rank threats because you switched to blitz."
    );
    let ai = fake(move |_| note(&causal, &two_cites()));
    let budget = no_budget();
    let o = explain_weakness_change(&ai, &budget, &input).await.unwrap();
    assert_eq!(structured_requests(&ai).len(), 2);
    assert_eq!(o.status, SectionStatus::Dropped);
    assert_eq!(o.text, None);
    assert_eq!(o.segments, None);
    assert!(o.citations.is_empty());
    let codes: Vec<ValidationCode> = o
        .dropped
        .iter()
        .flat_map(|d| d.errors.iter().map(|e| e.code))
        .collect();
    assert!(codes.contains(&ValidationCode::CausalClaim), "{codes:?}");
    assert!(codes.contains(&ValidationCode::MissingHedge), "{codes:?}");
    assert_eq!(o.dropped[0].section, "change");
    assert_eq!(o.calls.len(), 2, "both paid calls are metered");
}

#[tokio::test]
async fn citing_a_game_outside_the_change_examples_is_repaired() {
    let input = change_input();
    // li-w1-0#5 is a weakness exemplar of b2-rapid, not an example of this change.
    let ai = fake(|n| {
        if n == 1 {
            note(&hedged_text(), &[("li-w1-0", 5)])
        } else {
            note(&hedged_text(), &two_cites())
        }
    });
    let budget = no_budget();
    let o = explain_weakness_change(&ai, &budget, &input).await.unwrap();
    let reqs = structured_requests(&ai);
    assert_eq!(reqs.len(), 2);
    let msgs = reqs[1]["messages"].as_array().unwrap();
    assert_eq!(msgs.len(), 3);
    let repair = msgs[2]["content"][0]["text"].as_str().unwrap();
    assert!(
        repair.contains("- text: unknown_citation: li-w1-0#5"),
        "{repair}"
    );
    assert_eq!(o.status, SectionStatus::Ready);
    assert_eq!(o.citations.len(), 2);
    assert!(o.dropped.is_empty());
    assert_eq!(o.calls.len(), 2);
}

#[tokio::test]
async fn a_note_without_a_citation_or_with_a_contrary_direction_is_rejected() {
    let input = change_input();
    // The rate went down (0.22 → 0.14): "more often" claims a change R2 did not detect.
    let contrary = format!(
        "Since about {{{{date:changes.{K}.onsetDate}}}} you may be missing back-rank threats more often."
    );
    let ai = fake(move |n| {
        if n == 1 {
            note(&hedged_text(), &[])
        } else {
            note(&contrary, &two_cites())
        }
    });
    let budget = no_budget();
    let o = explain_weakness_change(&ai, &budget, &input).await.unwrap();
    assert_eq!(o.status, SectionStatus::Dropped);
    let reqs = structured_requests(&ai);
    let repair = reqs[1]["messages"][2]["content"][0]["text"]
        .as_str()
        .unwrap();
    assert!(repair.contains("missing_citation"), "{repair}");
    let codes: Vec<ValidationCode> = o
        .dropped
        .iter()
        .flat_map(|d| d.errors.iter().map(|e| e.code))
        .collect();
    assert_eq!(codes, vec![ValidationCode::FreeTextFact], "{:?}", o.dropped);
}

#[tokio::test]
async fn a_number_outside_the_change_is_rejected() {
    let input = change_input();
    // basis.analysedGames is in the input but not in the change's evidence.
    let text = "Over {{num:basis.analysedGames|int}} games you may be missing back-rank threats.";
    let ai = fake(move |_| note(text, &two_cites()));
    let budget = no_budget();
    let o = explain_weakness_change(&ai, &budget, &input).await.unwrap();
    assert_eq!(o.status, SectionStatus::Dropped);
    let codes: Vec<ValidationCode> = o
        .dropped
        .iter()
        .flat_map(|d| d.errors.iter().map(|e| e.code))
        .collect();
    assert!(
        codes.contains(&ValidationCode::UnresolvedToken),
        "{codes:?}"
    );
}

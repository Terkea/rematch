//! Shared helpers of the generation tests (R3 Task 14): valid section envelopes for the b2-rapid
//! fixture, section detection on a request, and fake-client plumbing. Included with `#[path]`.

#![allow(dead_code)]

use std::sync::{Arc, Mutex};

use ai::client::fake::{FakeAiClient, FakeCallKind, FakeScript, FakeStructured};
use ai::client::port::{AiError, StructuredRequest};
use ai::generate::request::build_section_request;
use ai::generate::section::{SectionOutcome, SectionStatus};
use ai::input::types::CoachingInput;
use ai::output::codes::{SECTION_KEYS, SectionKey};
use futures::future::BoxFuture;
use serde_json::{Value, json};

pub const WINDOW: &str = "Over your last {{num:basis.analysedGames|int}} analysed rapid games";

/// An envelope for `section` with every other part null.
pub fn envelope(section: SectionKey, part: Value) -> Value {
    let mut env = json!({
        "section": section.as_str(),
        "overview": null, "weaknesses": null, "openings": null, "climbPlan": null, "areas": null,
    });
    let key = match section {
        SectionKey::Overview => "overview",
        SectionKey::Weaknesses => "weaknesses",
        SectionKey::Openings => "openings",
        SectionKey::ClimbPlan => "climbPlan",
        SectionKey::Areas => "areas",
    };
    env[key] = part;
    env
}

pub fn w1_item(text: &str) -> Value {
    json!({
        "text": text,
        "citations": [{"gameId": "li-w1-0", "ply": 5}],
        "weaknessId": "W1",
        "title": "Loose pieces",
    })
}

pub fn good_w1_text() -> String {
    format!("{WINDOW} you left pieces loose, as with {{{{move:li-w1-0:5}}}}.")
}

/// A leaking text: a move in free text (`free_text_fact`) and no evidence window.
pub const LEAKING_TEXT: &str = "You missed Nxe4 here.";

/// A valid envelope of `section` for the b2-rapid fixture: every item passes the validator.
pub fn valid(section: SectionKey) -> Value {
    let item = |t: &str| json!({"text": t, "citations": []});
    let part = match section {
        SectionKey::Overview => json!({
            "headline": item("Solid foundations."),
            "summary": item(&format!("{WINDOW} your play was steady.")),
            "strengths": [],
        }),
        SectionKey::Weaknesses => json!({"items": [w1_item(&good_w1_text())]}),
        SectionKey::Openings => json!({"items": []}),
        SectionKey::ClimbPlan => json!({
            "intro": item("A plan for the next few weeks."),
            "focuses": [{
                "weaknessId": "W1",
                "why": {"text": good_w1_text(), "citations": [{"gameId": "li-w1-0", "ply": 5}]},
                "drills": [{"drillType": "own_mistake_puzzles", "rationale": "Replay your own slips."}],
                "habit": item("Check every piece before you move."),
            }],
            "weeklyRhythm": item("Play slower games on weekdays."),
        }),
        SectionKey::Areas => json!({"items": [
            {"text": "Your opening play looks sound.", "citations": [], "area": "openings"},
            {"text": "Your middlegame play looks sound.", "citations": [], "area": "middlegame"},
            {"text": "Your endgame play looks sound.", "citations": [], "area": "endgame"},
            {"text": "Your tactical play looks sound.", "citations": [], "area": "tactics"},
            {"text": "Your calculation looks sound.", "citations": [], "area": "calculation"},
            {"text": "Your clock use looks sound.", "citations": [], "area": "time"},
            {"text": "Your mindset looks sound.", "citations": [], "area": "mindset"},
        ]}),
    };
    envelope(section, part)
}

pub fn out(v: Value) -> FakeStructured {
    FakeStructured {
        parsed: Some(v),
        ..FakeStructured::default()
    }
}

pub fn stop(reason: &str) -> FakeStructured {
    FakeStructured {
        text: Some(String::new()),
        stop_reason: Some(reason.to_owned()),
        ..FakeStructured::default()
    }
}

/// The section a section request asks for (its task block is the section's own prompt).
pub fn section_of(input: &CoachingInput, req: &StructuredRequest) -> SectionKey {
    let task = req.messages[0].content.last().cloned();
    SECTION_KEYS
        .into_iter()
        .find(|k| {
            build_section_request(input, *k, true).messages[0]
                .content
                .last()
                .cloned()
                == task
        })
        .expect("a section request")
}

/// A fake answering each section request with `f(section, attempt)`, where `attempt` counts the
/// calls for that section (1 for the first).
pub fn per_section(
    input: &CoachingInput,
    f: impl Fn(SectionKey, usize) -> Result<FakeStructured, AiError> + Send + Sync + 'static,
) -> FakeAiClient {
    let input = input.clone();
    let seen: Arc<Mutex<Vec<SectionKey>>> = Arc::default();
    FakeAiClient::new(FakeScript {
        structured: Some(Box::new(move |req, _| {
            let k = section_of(&input, req);
            let attempt = {
                let mut s = seen.lock().unwrap();
                s.push(k);
                s.iter().filter(|x| **x == k).count()
            };
            f(k, attempt)
        })),
        ..FakeScript::default()
    })
}

/// The structured requests the fake received, as JSON.
pub fn structured_requests(f: &FakeAiClient) -> Vec<Value> {
    f.calls()
        .into_iter()
        .filter(|c| c.kind == FakeCallKind::Structured)
        .map(|c| c.request_json)
        .collect()
}

pub type BudgetFn = Box<dyn Fn() -> BoxFuture<'static, Result<(), AiError>> + Send + Sync>;

pub fn no_budget() -> BudgetFn {
    Box::new(|| Box::pin(async { Ok(()) }))
}

pub fn ready(o: &SectionOutcome) -> bool {
    o.status == SectionStatus::Ready
}

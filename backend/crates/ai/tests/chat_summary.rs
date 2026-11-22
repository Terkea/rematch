//! Task 23: thread summaries and fallback titles (T12 fake client only).

use ai::chat::anchor::ChatAnchor;
use ai::chat::summary::{CHAT_SUMMARY_SCHEMA, ThreadSummary, fallback_title, summarise_thread};
use ai::client::fake::{FakeAiClient, FakeCallKind, FakeScript, FakeStructured};
use ai::client::model::{Effort, Models};
use ai::client::port::AiError;
use ai::output::codes::SectionKey;
use futures::future::BoxFuture;
use serde_json::{Value, json};
use uuid::Uuid;

fn fake(parsed: Value) -> FakeAiClient {
    FakeAiClient::new(FakeScript {
        structured: Some(Box::new(move |_, _| {
            Ok(FakeStructured {
                parsed: Some(parsed.clone()),
                ..FakeStructured::default()
            })
        })),
        ..FakeScript::default()
    })
}

fn transcript() -> Vec<(String, String)> {
    vec![
        (
            "Why do I keep losing with the Sicilian?".into(),
            "Look at {{move:li-w1-0:5}}.".into(),
        ),
        ("And my endgames?".into(), "Trade pieces when ahead.".into()),
    ]
}

async fn summarise(f: &FakeAiClient) -> ThreadSummary {
    let budget = || -> BoxFuture<'static, Result<(), AiError>> { Box::pin(async { Ok(()) }) };
    summarise_thread(f, &budget, &transcript())
        .await
        .expect("the call succeeds")
}

#[tokio::test]
async fn clean_title_and_summary_are_kept() {
    let f = fake(json!({
        "title": "Sicilian troubles and endgames",
        "summary": "We talked about your Sicilian games and how to handle endgames."
    }));
    let s = summarise(&f).await;
    assert_eq!(s.title.as_deref(), Some("Sicilian troubles and endgames"));
    assert_eq!(
        s.summary.as_deref(),
        Some("We talked about your Sicilian games and how to handle endgames.")
    );
    assert_eq!(s.calls.len(), 1);
    let calls = f.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].kind, FakeCallKind::Structured);
    let req = &calls[0].request_json;
    assert_eq!(req["model"], json!(Models::EXPLAIN));
    assert_eq!(req["effort"], json!(Effort::Low));
    assert_eq!(req["max_tokens"], json!(1000));
    assert_eq!(req["schema"], json!({"id": "chat-summary", "version": 1}));
    assert!(
        req.to_string()
            .contains("Why do I keep losing with the Sicilian?")
    );
    assert_eq!(CHAT_SUMMARY_SCHEMA.id, "chat-summary");
    assert_eq!(CHAT_SUMMARY_SCHEMA.version, 1);
    let schema = (CHAT_SUMMARY_SCHEMA.json)();
    assert_eq!(schema["required"], json!(["summary", "title"]));
}

#[tokio::test]
async fn title_with_digits_falls_back() {
    let f = fake(json!({
        "title": "Your 3 lost games",
        "summary": "We talked about your openings."
    }));
    let s = summarise(&f).await;
    assert_eq!(s.title, None);
    assert_eq!(s.summary.as_deref(), Some("We talked about your openings."));
}

#[tokio::test]
async fn unsafe_or_long_texts_fall_back() {
    for (title, summary) in [
        ("Upgrade to premium", "You will gain a lot of rating soon."),
        (
            "A move: Nf3 or e4",
            "We looked at the game from 2026-11-01.",
        ),
        (&"a".repeat(81) as &str, &"b ".repeat(1001) as &str),
        ("  ", ""),
    ] {
        let f = fake(json!({"title": title, "summary": summary}));
        let s = summarise(&f).await;
        assert_eq!(s.title, None, "{title}");
        assert_eq!(s.summary, None, "{summary}");
    }
    // A schema mismatch keeps neither.
    let f = fake(json!({"title": "Openings"}));
    let s = summarise(&f).await;
    assert_eq!((s.title, s.summary), (None, None));
    assert_eq!(s.calls.len(), 1);
}

#[test]
fn fallback_title_per_anchor_kind() {
    let cases = [
        (
            Some(ChatAnchor::ReportSection {
                report_id: Uuid::nil(),
                section: SectionKey::Overview,
            }),
            "Chat about your report",
        ),
        (
            Some(ChatAnchor::Weakness {
                weakness_id: "W1".into(),
            }),
            "Chat about a weakness",
        ),
        (
            Some(ChatAnchor::Position {
                game_id: "g".into(),
                ply: 3,
            }),
            "Chat about a position",
        ),
        (
            Some(ChatAnchor::Move {
                game_id: "g".into(),
                ply: 3,
            }),
            "Chat about a move",
        ),
        (
            Some(ChatAnchor::OpeningLine {
                line_key: "l".into(),
            }),
            "Chat about an opening",
        ),
        (
            Some(ChatAnchor::Game {
                game_id: "g".into(),
            }),
            "Chat about a game",
        ),
        (None, "Chat with your coach"),
    ];
    for (anchor, want) in cases {
        assert_eq!(fallback_title(anchor.as_ref()), want);
    }
}

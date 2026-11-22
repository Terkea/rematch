//! Task 13: the prompt context and the cache-friendly section request layout. No live call is
//! made: the sidecar body is captured by a local wiremock server.

use ai::client::model::{Effort, Models};
use ai::client::port::{
    AiUsage, Message, ModelUsage, Role, StructuredRequest, StructuredResult, TextBlock,
};
use ai::client::sidecar::{SidecarClient, SidecarConfig, SidecarToken};
use ai::client::wire::MessagesRequest;
use ai::generate::context::{build_prompt_context, canonical_json_escaped};
use ai::generate::request::{build_repair_messages, build_section_request};
use ai::input::types::{BandId, CoachingInput};
use ai::output::codes::{SECTION_KEYS, SectionKey, ValidationCode, ValidationError};
use ai::prompts::{REPORT_PROMPT_SET, load_prompt, render_template};
use ai::testing::{fixtures_dir, input};
use ai::validator::language::{confidence_phrase, evidence_window_token, likelihood_phrase};
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const ACCOUNT_ID: &str = "6f1c1d52-3a4b-4c8e-9d2a-0b7a5e1f3c44";

fn text_of(block: &Value) -> &str {
    block["text"].as_str().unwrap()
}

fn user_blocks(req: &StructuredRequest) -> &[Value] {
    assert_eq!(req.messages.len(), 1);
    assert_eq!(req.messages[0].role, Role::User);
    &req.messages[0].content
}

fn cached_blocks(req: &StructuredRequest) -> usize {
    req.system.iter().filter(|b| b.cache).count()
        + user_blocks(req)
            .iter()
            .filter(|b| b.get("cache_control").is_some())
            .count()
}

#[test]
fn section_requests_share_the_cached_prefix() {
    let b2 = input("b2-rapid");
    let b0 = input("b0-blitz");
    assert_eq!(b2.band, BandId::B2);
    assert_eq!(b0.band, BandId::B0);
    let reqs: Vec<StructuredRequest> = SECTION_KEYS
        .iter()
        .map(|k| build_section_request(&b2, *k, false))
        .collect();
    let first = &reqs[0];
    for (k, r) in SECTION_KEYS.iter().zip(&reqs) {
        assert_eq!(r.system.len(), 2, "{k}");
        assert_eq!(r.system[0], first.system[0], "{k}: system[0]");
        assert_eq!(r.system[1], first.system[1], "{k}: system[1]");
        assert!(r.system[0].cache && r.system[1].cache, "{k}");
        let blocks = user_blocks(r);
        assert_eq!(blocks.len(), 2, "{k}");
        assert_eq!(blocks[0], user_blocks(first)[0], "{k}: user block 1");
        assert_eq!(blocks[0]["cache_control"], json!({"type": "ephemeral"}));
        assert!(
            blocks[1].get("cache_control").is_none(),
            "{k}: the task is not cached"
        );
        assert_eq!(cached_blocks(r), 3, "{k}");
        assert_eq!(r.schema.id, "report-section");
        assert_eq!(r.schema.version, 1);
    }
    // The section tasks differ.
    let tasks: std::collections::BTreeSet<&str> =
        reqs.iter().map(|r| text_of(&user_blocks(r)[1])).collect();
    assert_eq!(tasks.len(), 5);

    // system[0] is shared across bands; system[1] carries the band's tone.
    let b0_req = build_section_request(&b0, SectionKey::Weaknesses, false);
    assert_eq!(b0_req.system[0], first.system[0]);
    assert_ne!(b0_req.system[1], first.system[1]);

    // The block texts are the brief's layout.
    // Each prompt at its version in the report's set (ADR 0084: core/tokens v2, report v2,
    // the overview v2; the glossary ends system[1]).
    let p = |id: &str| {
        let v = REPORT_PROMPT_SET
            .iter()
            .find(|(i, _)| *i == id)
            .map_or(1, |(_, v)| *v);
        load_prompt(id, v).unwrap().text
    };
    assert_eq!(
        first.system[0].text,
        format!("{}\n\n{}", p("core/rules"), p("core/tokens"))
    );
    assert_eq!(first.system[0], ai::generate::request::rules_block());
    assert_eq!(
        first.system[1].text,
        format!(
            "{}\n\n{}\n\n{}\n\n{}",
            p("report/report"),
            p("report/output"),
            p("tone/b2"),
            p("core/glossary")
        )
    );
    assert_eq!(
        b0_req.system[1].text,
        format!(
            "{}\n\n{}\n\n{}\n\n{}",
            p("report/report"),
            p("report/output"),
            p("tone/b0"),
            p("core/glossary")
        )
    );
    let ctx = build_prompt_context(&b2, false);
    assert_eq!(
        text_of(&user_blocks(first)[0]),
        format!(
            "<coaching_input>{}</coaching_input>",
            canonical_json_escaped(&ctx)
        )
    );
    for (k, r) in SECTION_KEYS.iter().zip(&reqs) {
        let id = format!("report/section-{}", k.as_str());
        assert_eq!(
            text_of(&user_blocks(r)[1]),
            render_template(p(&id), &[]).unwrap(),
            "{k}"
        );
    }
}

#[test]
fn section_request_uses_the_report_model_medium_effort_and_16000_tokens() {
    let r = build_section_request(&input("b2-rapid"), SectionKey::Overview, true);
    assert_eq!(r.model, Models::REPORT);
    assert_eq!(r.effort, Effort::Medium);
    assert_eq!(r.max_tokens, 16000);
    assert_eq!(r.messages.last().unwrap().role, Role::User);
}

#[test]
fn first_report_flag_changes_only_the_focuses() {
    let i = input("b2-rapid");
    let a = build_prompt_context(&i, true);
    let b = build_prompt_context(&i, false);
    assert_eq!(a["focuses"].as_array().unwrap().len(), 2);
    assert_eq!(b["focuses"].as_array().unwrap().len(), 3);
    let (mut a2, mut b2) = (a.clone(), b.clone());
    for v in [&mut a2, &mut b2] {
        v.as_object_mut().unwrap().remove("focuses");
        v.as_object_mut().unwrap().remove("allowedDrills");
    }
    assert_eq!(a2, b2);
}

fn sidecar_query_ok() -> String {
    let p = fixtures_dir().join("sidecar/query-ok.json");
    std::fs::read_to_string(p).unwrap()
}

#[tokio::test]
async fn request_json_has_no_account_id_or_sampling_params() {
    let i: CoachingInput = input("b2-rapid");
    assert_eq!(i.account_id.to_string(), ACCOUNT_ID);
    let forbidden = [
        ACCOUNT_ID,
        "accountId",
        "temperature",
        "top_p",
        "top_k",
        "topP",
        "topK",
        "budget_tokens",
    ];

    // The Messages wire request (batches).
    for k in SECTION_KEYS {
        let req = build_section_request(&i, k, false);
        let wire = serde_json::to_string(&MessagesRequest::structured(&req)).unwrap();
        for f in forbidden {
            assert!(!wire.contains(f), "{k}: Messages request contains {f}");
        }
        assert!(!wire.contains("\"thinking\""), "{k}");
    }

    // The sidecar body (interactive calls, ADR 0062), captured from the real client.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/query"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(sidecar_query_ok(), "application/json"),
        )
        .mount(&server)
        .await;
    let mut cfg = SidecarConfig::new(server.uri(), SidecarToken::new("t"));
    cfg.max_retries = 0;
    let client = SidecarClient::new(cfg, None).unwrap();
    let req = build_section_request(&i, SectionKey::Weaknesses, false);
    client.structured(&req).await.unwrap();
    let reqs = server.received_requests().await.unwrap();
    let body = String::from_utf8(reqs[0].body.clone()).unwrap();
    for f in forbidden {
        assert!(!body.contains(f), "sidecar body contains {f}");
    }
    let v: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["model"], Models::REPORT.as_str());
    assert!(v["systemPrompt"].as_str().unwrap().contains("Never"));
    assert!(!body.contains("cache_control"));
}

#[test]
fn context_lists_token_paths() {
    let ctx = build_prompt_context(&input("b2-rapid"), false);
    let num: Vec<&str> = ctx["refs"]["num"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(num.contains(&"basis.analysedGames"));
    assert!(num.contains(&"basis.importedGames"));
    assert!(num.windows(2).all(|w| w[0] < w[1]), "sorted");
    let date: Vec<&str> = ctx["refs"]["date"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(date.contains(&"basis.windowEnd"));
    assert_eq!(
        ctx["refs"]["lines"],
        json!(["w-italian", "b-sicilian-najdorf", "w-scandi-faced"])
    );
}

#[test]
fn context_drops_the_account_id_and_adds_the_computed_fields() {
    let i = input("b2-rapid");
    let ctx = build_prompt_context(&i, false);
    assert!(ctx.get("accountId").is_none());
    assert!(!ctx.to_string().contains(ACCOUNT_ID));
    // Everything else of the input is there, unchanged.
    let mut raw = i.to_json();
    raw.as_object_mut().unwrap().remove("accountId");
    for (k, v) in raw.as_object().unwrap() {
        if k != "weaknesses" {
            assert_eq!(&ctx[k], v, "{k}");
        }
    }
    assert_eq!(ctx["evidenceWindow"], evidence_window_token(i.time_class));
    assert_eq!(ctx["focuses"], json!(["W1", "W3", "W5"]));
    for (w, cw) in i
        .weaknesses
        .iter()
        .zip(ctx["weaknesses"].as_array().unwrap())
    {
        assert_eq!(cw["id"], w.id.as_str());
        assert_eq!(cw["likelihoodPhrase"], json!(likelihood_phrase(w.p_imp)));
        assert_eq!(cw["confidencePhrase"], confidence_phrase(w.confidence));
    }
    let drills = ctx["allowedDrills"].as_object().unwrap();
    assert_eq!(drills.len(), 3);
    assert!(
        drills["W1"]
            .as_array()
            .unwrap()
            .contains(&json!("own_game_analysis"))
    );
}

#[test]
fn early_read_context_has_general_focus() {
    let ctx = build_prompt_context(&input("early-read"), true);
    assert_eq!(ctx["focuses"], json!(["general"]));
    assert_eq!(
        ctx["allowedDrills"],
        json!({"general": ["own_game_analysis"]})
    );
}

fn result(content: Vec<Value>) -> StructuredResult {
    StructuredResult {
        parsed: None,
        parse_error: None,
        parse_issues: None,
        content,
        stop_reason: Some("end_turn".into()),
        usage: AiUsage::default(),
        model: Models::REPORT.as_str().into(),
        request_id: None,
        model_usage: vec![ModelUsage {
            model: Models::REPORT.as_str().into(),
            usage: AiUsage::default(),
        }],
        sdk_cost_usd: None,
    }
}

#[test]
fn repair_messages_append_raw_assistant_content_then_user() {
    let content = vec![
        json!({"type": "thinking", "thinking": "…", "signature": "sig"}),
        json!({"type": "text", "text": "{\"section\":\"overview\"}"}),
    ];
    let prev = result(content.clone());
    let errors = vec![
        ValidationError {
            code: ValidationCode::FreeTextFact,
            path: "overview.summary".into(),
            detail: "number:\"3\"".into(),
        },
        ValidationError {
            code: ValidationCode::MissingCitation,
            path: "overview.headline".into(),
            detail: "no citation".into(),
        },
    ];
    let msgs: Vec<Message> = build_repair_messages(&prev, &errors);
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[0].role, Role::Assistant);
    assert_eq!(msgs[0].content, content, "raw, byte for byte");
    assert_eq!(msgs[1].role, Role::User);
    assert_eq!(msgs.last().unwrap().role, Role::User);
    let expected = render_template(
        load_prompt("repair/repair", 1).unwrap().text,
        &[(
            "errors",
            "- overview.summary: free_text_fact: number:\"3\"\n- overview.headline: missing_citation: no citation",
        )],
    )
    .unwrap();
    assert_eq!(
        msgs[1].content,
        vec![TextBlock::new(expected).to_json()],
        "one uncached text block"
    );
}

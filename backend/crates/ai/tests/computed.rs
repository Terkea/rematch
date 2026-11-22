//! Port of TS plan B3's `computed.test.ts` plus the payload tests (R3 Task 14): the computed
//! report, the payload status rules, the JSON contract snapshot and the stored-payload loader.

#[path = "support/generate.rs"]
mod support;

use ai::client::model::Models;
use ai::db::reports::{self, NewReport, SectionWrite, SectionWriteStatus};
use ai::db::runs::{self, NewRun, ReportMode, ReportTrigger};
use ai::entitlements::Plan;
use ai::generate::computed::{ComputedReport, build_computed_report};
use ai::generate::payload::{
    PayloadParts, PayloadSection, REPORT_PAYLOAD_SCHEMA_VERSION, ReportStatus,
    assemble_report_payload, load_report_payload,
};
use ai::generate::report::generate_report;
use ai::generate::section::{SectionOutcome, SectionStatus};
use ai::input::types::{CoachingInput, WeaknessState};
use ai::output::codes::{DroppedItem, SECTION_KEYS, SectionKey, ValidationCode, ValidationError};
use ai::output::disclosure::{AI_DISCLOSURE_LABEL, AiDisclosure, make_disclosure};
use ai::testing::{fixtures_dir, input, make_input};
use ai::validator::language::format_evidence_window;
use futures::future::BoxFuture;
use serde_json::json;
use sqlx::PgPool;
use support::*;
use time::OffsetDateTime;
use time::macros::datetime;
use uuid::Uuid;

const REPORT_ID: Uuid = uuid::uuid!("7d0c3f4e-2a51-4b8e-9c61-0f2b7a9d1e35");
const AT: OffsetDateTime = datetime!(2026-11-20 08:30:00 UTC);

/// coaching-input-v2 (ADR 0084): the computed report carries the colour split and the
/// benchmark for the report's White vs Black block; a v1 input's report has neither key.
#[test]
fn computed_report_carries_the_colour_split() {
    let v2 = ai::testing::v2_input("b2-rapid");
    let c = build_computed_report(&v2, true);
    assert_eq!(c.by_color, v2.by_color);
    assert_eq!(c.benchmark, v2.benchmark);
    let json = serde_json::to_value(&c).unwrap();
    assert_eq!(json["byColor"]["white"]["games"], json!(42));
    assert_eq!(json["byColor"]["black"]["score"]["value"], serde_json::Value::Null);
    assert_eq!(json["benchmark"]["ratingHigh"], json!(975));
    let back: ComputedReport = serde_json::from_value(json).unwrap();
    assert_eq!(back, c);

    let v1 = build_computed_report(&input("b2-rapid"), true);
    let json = serde_json::to_value(&v1).unwrap();
    assert!(json.get("byColor").is_none() && json.get("benchmark").is_none());
    // A stored v1 computed report still loads.
    let back: ComputedReport = serde_json::from_value(json).unwrap();
    assert_eq!(back.by_color, None);
}

#[test]
fn computed_report_copies_the_input_and_splits_changes() {
    let i = input("b2-rapid");
    let c = build_computed_report(&i, true);
    assert_eq!(c.player.platform, i.platform);
    assert_eq!(c.player.time_class, i.time_class);
    assert_eq!(c.player.band, i.band);
    assert_eq!(c.player.rating, Some(1450));
    assert_eq!(c.basis, i.basis);
    assert_eq!(
        c.evidence_window,
        format_evidence_window(&i.basis, i.time_class)
    );
    assert!(
        c.evidence_window
            .starts_with("over your last 214 analysed rapid games")
    );
    assert_eq!(c.area_scores, i.area_scores);
    assert_eq!(c.metrics, i.metrics);
    assert_eq!(c.weaknesses, i.weaknesses);
    assert_eq!(c.openings, i.openings);
    assert_eq!(c.strengths, i.strengths);
    assert!(!c.strengths.is_empty());
    assert_eq!(c.focuses, ["W1", "W3"]);
    assert_eq!(build_computed_report(&i, false).focuses, ["W1", "W3", "W5"]);
    // b2-rapid: one change to active (new), one to improving (neither).
    assert_eq!(c.new_this_period.len(), 1);
    assert_eq!(c.new_this_period[0].to_state, WeaknessState::Active);
    assert!(c.fixed_this_period.is_empty());
}

#[test]
fn emerging_is_new_and_resolved_is_fixed() {
    let raw = make_input(json!({"changes": [{"toState": "resolved"}, {"toState": "emerging"}]}));
    let i: CoachingInput = serde_json::from_value(raw).unwrap();
    let c = build_computed_report(&i, true);
    assert_eq!(c.fixed_this_period.len(), 1);
    assert_eq!(c.fixed_this_period[0].series_key, i.changes[0].series_key);
    assert_eq!(c.new_this_period.len(), 1);
    assert_eq!(c.new_this_period[0].series_key, i.changes[1].series_key);
}

#[test]
fn early_read_has_no_focus() {
    assert!(
        build_computed_report(&input("early-read"), true)
            .focuses
            .is_empty()
    );
}

#[test]
fn computed_report_json_round_trips_in_camel_case() {
    let c = build_computed_report(&input("b2-rapid"), true);
    let v = serde_json::to_value(&c).unwrap();
    for k in [
        "player",
        "basis",
        "evidenceWindow",
        "areaScores",
        "metrics",
        "weaknesses",
        "openings",
        "strengths",
        "focuses",
        "newThisPeriod",
        "fixedThisPeriod",
    ] {
        assert!(v.get(k).is_some(), "missing {k}");
    }
    assert_eq!(v["player"]["timeClass"], "rapid");
    let back: ComputedReport = serde_json::from_value(v).unwrap();
    assert_eq!(back, c);
}

fn part(section: SectionKey, status: SectionStatus) -> PayloadSection {
    let ready = status == SectionStatus::Ready;
    PayloadSection {
        section,
        status,
        content: ready.then(|| {
            // Any validated content of the right key will do here.
            let i = input("b2-rapid");
            let ev = ai::validator::evidence::build_evidence(&i);
            let env = ai::output::schema::parse_report_section(&valid(section)).unwrap();
            let ctx = ai::validator::section::SectionContext {
                input: &i,
                evidence: &ev,
                focuses: &["W1".to_owned(), "W3".to_owned()],
            };
            ai::validator::section::validate_section(section, &env, &ctx)
                .content
                .unwrap()
        }),
        dropped: if ready {
            Vec::new()
        } else {
            vec![DroppedItem {
                section: section.as_str().to_owned(),
                path: String::new(),
                errors: vec![ValidationError {
                    code: ValidationCode::Refusal,
                    path: String::new(),
                    detail: "refused".into(),
                }],
            }]
        },
    }
}

fn disclosure() -> AiDisclosure {
    make_disclosure(Models::REPORT.as_str(), "report@1", AT)
}

fn parts(sections: Vec<PayloadSection>) -> PayloadParts {
    PayloadParts {
        report_id: REPORT_ID,
        version: 3,
        computed: build_computed_report(&input("b2-rapid"), true),
        sections,
        ai_disclosure: Some(disclosure()),
        generated_at: AT,
    }
}

#[test]
fn assemble_payload_status_rules_and_disclosure() {
    use SectionStatus::{Dropped, Ready};
    // All five ready: complete, with the disclosure.
    let all = SECTION_KEYS.into_iter().map(|k| part(k, Ready)).collect();
    let p = assemble_report_payload(parts(all));
    assert_eq!(p.status, ReportStatus::Complete);
    assert_eq!(p.ai_disclosure, Some(disclosure()));
    assert_eq!(p.sections.len(), 5);
    assert!(p.dropped.is_empty());
    assert_eq!(p.schema_version, REPORT_PAYLOAD_SCHEMA_VERSION);
    assert_eq!(p.report_id, REPORT_ID);
    assert_eq!(p.version, 3);
    assert_eq!(p.generated_at, "2026-11-20T08:30:00.000Z");

    // No section at all: computed only, no disclosure.
    let p = assemble_report_payload(parts(Vec::new()));
    assert_eq!(p.status, ReportStatus::ComputedOnly);
    assert_eq!(p.ai_disclosure, None);
    assert!(p.sections.is_empty());

    // Only dropped sections: still computed only, no disclosure; dropped are null and listed.
    let p = assemble_report_payload(parts(vec![
        part(SectionKey::Overview, Dropped),
        part(SectionKey::Areas, Dropped),
    ]));
    assert_eq!(p.status, ReportStatus::ComputedOnly);
    assert_eq!(p.ai_disclosure, None);
    assert_eq!(p.sections.get(&SectionKey::Overview), Some(&None));
    assert_eq!(p.dropped.len(), 2);

    // Some ready: partial, with the disclosure.
    let p = assemble_report_payload(parts(vec![
        part(SectionKey::Overview, Ready),
        part(SectionKey::Weaknesses, Dropped),
    ]));
    assert_eq!(p.status, ReportStatus::Partial);
    assert_eq!(p.ai_disclosure, Some(disclosure()));
    assert!(p.sections[&SectionKey::Overview].is_some());
    assert_eq!(p.sections[&SectionKey::Weaknesses], None);
    assert_eq!(p.dropped.len(), 1);

    // Four ready is not complete.
    let four = SECTION_KEYS
        .into_iter()
        .take(4)
        .map(|k| part(k, Ready))
        .collect();
    assert_eq!(
        assemble_report_payload(parts(four)).status,
        ReportStatus::Partial
    );
}

async fn complete_outcomes(i: &CoachingInput) -> Vec<SectionOutcome> {
    let ai = per_section(i, |k, _| Ok(out(valid(k))));
    let budget = no_budget();
    let on = |_: SectionOutcome| -> BoxFuture<'static, anyhow::Result<()>> {
        Box::pin(async { Ok(()) })
    };
    generate_report(&ai, &budget, i, true, &[], 2, &on)
        .await
        .unwrap()
}

/// Compares `v` with the snapshot `payload/<name>`; `UPDATE_SNAPSHOTS=1` rewrites it.
fn snapshot(name: &str, v: &serde_json::Value) {
    let text = format!(
        "{}
",
        serde_json::to_string_pretty(v).unwrap()
    );
    let path = fixtures_dir().join("payload").join(name);
    if std::env::var_os("UPDATE_SNAPSHOTS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &text).unwrap();
    }
    let stored = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "{}: {e} (run with UPDATE_SNAPSHOTS=1 to write it)",
            path.display()
        )
    });
    assert_eq!(
        stored.replace(
            "

", "
"
        ),
        text,
        "the payload contract changed; review it and rerun with UPDATE_SNAPSHOTS=1"
    );
}

#[tokio::test]
async fn payload_json_shape_is_camel_case() {
    let i = input("b2-rapid");
    let outcomes = complete_outcomes(&i).await;
    let p = assemble_report_payload(PayloadParts {
        sections: outcomes.iter().map(PayloadSection::from).collect(),
        ..parts(Vec::new())
    });
    assert_eq!(p.status, ReportStatus::Complete);
    let v = serde_json::to_value(&p).unwrap();
    assert_eq!(v["schemaVersion"], "report-payload-v1");
    assert_eq!(v["reportId"], REPORT_ID.to_string());
    assert_eq!(v["status"], "complete");
    assert_eq!(v["aiDisclosure"]["label"], AI_DISCLOSURE_LABEL);
    assert_eq!(v["generatedAt"], "2026-11-20T08:30:00.000Z");
    assert!(v["computed"]["evidenceWindow"].is_string());
    // Computed strengths ship with their exemplar positions (board data).
    let s0 = &v["computed"]["strengths"][0];
    assert_eq!(s0["area"], "openings");
    assert!(s0["exemplars"][0]["fen"].is_string());
    let cp = &v["sections"]["climb_plan"]["climbPlan"];
    assert!(cp["intro"]["segments"].is_array());
    assert!(cp["focuses"][0]["why"]["segments"].is_array());
    assert!(
        v["sections"]["weaknesses"]["weaknesses"]["items"][0]["segments"]
            .as_array()
            .is_some_and(|s| !s.is_empty())
    );
    // The payload reads back.
    let back: ai::generate::payload::ReportPayload = serde_json::from_value(v.clone()).unwrap();
    assert_eq!(back, p);

    // The hand-off contract for the web.
    snapshot("report-payload.example.json", &v);
}

/// Model text that fails validation (a free-text move and a hallucinated weakness id).
const LEAKED_MOVE: &str = "Qxh7";
const HALLUCINATED_ID: &str = "W99zzhallucinated";

async fn partial_outcomes(i: &CoachingInput) -> Vec<SectionOutcome> {
    let ai = per_section(i, |k, _| {
        Ok(match k {
            SectionKey::Weaknesses => out(envelope(
                k,
                json!({"items": [
                    w1_item(&good_w1_text()),
                    {"text": format!("You missed {LEAKED_MOVE} here."), "citations": [{"gameId": "li-w3-1", "ply": 18}], "weaknessId": "W3", "title": "Forks"},
                    {"text": good_w1_text(), "citations": [{"gameId": "li-w1-0", "ply": 5}], "weaknessId": HALLUCINATED_ID, "title": "Made up"},
                ]}),
            )),
            SectionKey::Openings => stop("refusal"),
            _ => out(valid(k)),
        })
    });
    let budget = no_budget();
    let on = |_: SectionOutcome| -> BoxFuture<'static, anyhow::Result<()>> {
        Box::pin(async { Ok(()) })
    };
    // Climb plan and areas not generated (yet).
    generate_report(
        &ai,
        &budget,
        i,
        true,
        &[SectionKey::ClimbPlan, SectionKey::Areas],
        2,
        &on,
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn the_payload_never_carries_rejected_model_text() {
    let i = input("b2-rapid");
    let outcomes = partial_outcomes(&i).await;
    // Server side, the dropped details keep the offending fragments (audit and repair).
    let internal = serde_json::to_string(
        &outcomes
            .iter()
            .flat_map(|o| o.dropped.clone())
            .collect::<Vec<_>>(),
    )
    .unwrap();
    // The free-text move is quoted as its square.
    assert!(internal.contains(r#"square:\"h7\""#), "{internal}");
    assert!(internal.contains(HALLUCINATED_ID), "{internal}");

    let p = assemble_report_payload(PayloadParts {
        sections: outcomes.iter().map(PayloadSection::from).collect(),
        ..parts(Vec::new())
    });
    assert_eq!(p.status, ReportStatus::Partial);
    let text = serde_json::to_string(&p).unwrap();
    assert!(!text.contains(LEAKED_MOVE));
    assert!(!text.contains("square:"));
    assert!(!text.contains(HALLUCINATED_ID));
    assert!(!text.contains("detail"));
    // The web gets the section, the path and the stable codes.
    let v = serde_json::to_value(&p).unwrap();
    let dropped = v["dropped"].as_array().unwrap();
    let paths: Vec<&str> = dropped
        .iter()
        .map(|d| d["path"].as_str().unwrap())
        .collect();
    assert_eq!(
        paths,
        ["weaknesses.items[1]", "weaknesses.items[2]", ""],
        "{dropped:?}"
    );
    for d in dropped {
        let keys: Vec<&String> = d.as_object().unwrap().keys().collect();
        assert_eq!(keys, ["errors", "path", "section"]);
        for e in d["errors"].as_array().unwrap() {
            let keys: Vec<&String> = e.as_object().unwrap().keys().collect();
            assert_eq!(keys, ["code", "path"]);
        }
    }
    assert_eq!(dropped[2]["section"], "openings");
    assert_eq!(dropped[2]["errors"][0]["code"], "refusal");
}

#[tokio::test]
async fn partial_payload_snapshot() {
    let i = input("b2-rapid");
    let outcomes = partial_outcomes(&i).await;
    let p = assemble_report_payload(PayloadParts {
        sections: outcomes.iter().map(PayloadSection::from).collect(),
        ..parts(Vec::new())
    });
    let v = serde_json::to_value(&p).unwrap();
    assert_eq!(v["status"], "partial");
    assert!(v["sections"]["openings"].is_null());
    assert!(v["sections"].get("climb_plan").is_none());
    assert!(v["sections"].get("areas").is_none());
    snapshot("report-payload.partial.example.json", &v);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn load_report_payload_assembles_the_stored_rows(pool: PgPool) {
    let i = input("b2-rapid");
    let outcomes = complete_outcomes(&i).await;
    let computed = build_computed_report(&i, true);
    let account_id = testkit::insert_account(&pool, "lichess", "t14").await;

    let mut tx = pool.begin().await.unwrap();
    let run_id = runs::insert_run(
        &mut tx,
        &NewRun {
            account_id,
            time_class: "rapid".into(),
            trigger: ReportTrigger::Manual,
            plan: Plan::Pro,
            mode: ReportMode::Sync,
            prompt_version: "report@1".into(),
            is_first_report: true,
            ai_disabled_reason: None,
        },
    )
    .await
    .unwrap();
    let report = reports::create_report_with_next_version(
        &mut tx,
        &NewReport {
            account_id,
            time_class: "rapid".into(),
            report_run_id: run_id,
            schema_version: REPORT_PAYLOAD_SCHEMA_VERSION.into(),
            computed: serde_json::to_value(&computed).unwrap(),
            status: ReportStatus::ComputedOnly,
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();

    let mut conn = pool.acquire().await.unwrap();
    // Overview and weaknesses ready, openings dropped.
    for o in &outcomes[..2] {
        write(&mut conn, report.id, o).await;
    }
    let dropped = PayloadSection {
        dropped: part(SectionKey::Openings, SectionStatus::Dropped).dropped,
        ..part(SectionKey::Openings, SectionStatus::Dropped)
    };
    reports::upsert_section(
        &mut conn,
        report.id,
        &SectionWrite {
            section: SectionKey::Openings,
            status: SectionWriteStatus::Dropped,
            content: None,
            dropped: serde_json::to_value(&dropped.dropped).unwrap(),
            attempts: 2,
            model: Models::REPORT.as_str().into(),
        },
    )
    .await
    .unwrap();
    let disc = disclosure();
    reports::set_status(
        &mut conn,
        report.id,
        ReportStatus::Partial,
        Some(&serde_json::to_value(&disc).unwrap()),
    )
    .await
    .unwrap();

    let p = load_report_payload(&mut conn, report.id)
        .await
        .unwrap()
        .expect("the report exists");
    let row = reports::get_report(&mut conn, report.id)
        .await
        .unwrap()
        .unwrap();
    let mut sections: Vec<PayloadSection> =
        outcomes[..2].iter().map(PayloadSection::from).collect();
    sections.push(dropped);
    let expected = assemble_report_payload(PayloadParts {
        report_id: report.id,
        version: 1,
        computed,
        sections,
        ai_disclosure: Some(disc),
        generated_at: row.updated_at,
    });
    assert_eq!(p, expected);
    assert_eq!(p.status, ReportStatus::Partial);

    assert_eq!(
        load_report_payload(&mut conn, Uuid::nil()).await.unwrap(),
        None
    );

    // A row of another payload version is not read as v1.
    sqlx::query("UPDATE reports SET schema_version = 'report-payload-v2' WHERE id = $1")
        .bind(report.id)
        .execute(&mut *conn)
        .await
        .unwrap();
    let e = load_report_payload(&mut conn, report.id).await.unwrap_err();
    assert!(e.to_string().contains("report-payload-v2"), "{e:#}");
}

async fn write(conn: &mut sqlx::PgConnection, report_id: Uuid, o: &SectionOutcome) {
    reports::upsert_section(
        conn,
        report_id,
        &SectionWrite {
            section: o.section,
            status: SectionWriteStatus::Ready,
            content: Some(serde_json::to_value(o.content.as_ref().unwrap()).unwrap()),
            dropped: serde_json::to_value(&o.dropped).unwrap(),
            attempts: i16::from(o.attempts),
            model: Models::REPORT.as_str().into(),
        },
    )
    .await
    .unwrap();
}

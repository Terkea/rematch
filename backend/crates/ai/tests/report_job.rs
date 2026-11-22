//! Task 18: the `generate-report` job (TS plan C2). Progressive sections, degradation, budgets,
//! metering and idempotent retries. Fake client only (no network, no key); a fixture provider
//! serves the b2-rapid input; the clock is injected.

#[path = "support/generate.rs"]
mod support;

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use ai::client::cost::{AI_MONTHLY_BUDGET_MICRO_USD, PricingTable, cost_micro_usd};
use ai::client::fake::{FakeAiClient, FakeScript, FakeStructured};
use ai::client::model::{AiModel, Models};
use ai::client::port::{
    AiClient, AiError, AiUsage, BatchItem, BatchResultItem, BatchStatus, ChatTurnRequest,
    ModelUsage, SchemaResolver, StructuredRequest, StructuredResult, ToolHandler, TurnEvent,
    UnavailableReason,
};
use ai::db::reports::{self, ReportStatus};
use ai::db::runs::{self, ReportTrigger, RunStatus};
use ai::db::usage::AiFeature;
use ai::entitlements::{Entitlements, Plan};
use ai::generate::section::CallRecord;
use ai::input::types::{CoachingInput, TimeClass};
use ai::jobs::budget::{BudgetConfig, BudgetConfigError, budget_guard, meter};
use ai::jobs::kinds::{self, KIND_GENERATE_REPORT};
use ai::jobs::providers::{CoachingInputProvider, NullInputProvider};
use ai::jobs::report_job::{
    FREE_REPORT_INTERVAL_DAYS, GenerateReportHandler, HEARTBEAT_EVERY, RECLAIM_STALE_AFTER,
    ReportDeps, request_report,
};
use ai::output::codes::SectionKey;
use ai::output::disclosure::make_disclosure;
use ai::testing::{input, load_fixture};
use futures::future::BoxFuture;
use futures::stream::BoxStream;
use queue::queue::Queue;
use queue::runner::{Handler, JobContext};
use queue::{Job, JobError};
use serde_json::{Value, json};
use sqlx::PgPool;
use time::OffsetDateTime;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use support::*;

// ---------------------------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------------------------

/// Now, at microsecond precision (what Postgres stores).
fn now_micros() -> OffsetDateTime {
    let t = OffsetDateTime::now_utc();
    t.replace_nanosecond(t.nanosecond() / 1000 * 1000).unwrap()
}

/// A provider serving one fixed input (`None` = no input yet).
struct FixtureInputs(Option<Value>);

impl CoachingInputProvider for FixtureInputs {
    fn coaching_input<'a>(
        &'a self,
        _account_id: Uuid,
        _tc: TimeClass,
    ) -> BoxFuture<'a, anyhow::Result<Option<Value>>> {
        let v = self.0.clone();
        Box::pin(async move { Ok(v) })
    }
}

/// Every account on one plan, each its own budget.
struct FixedPlan(Plan);

impl Entitlements for FixedPlan {
    fn plan<'a>(&'a self, _account_id: Uuid) -> BoxFuture<'a, Plan> {
        let p = self.0;
        Box::pin(async move { p })
    }
    fn budget_scope<'a>(&'a self, account_id: Uuid) -> BoxFuture<'a, Vec<Uuid>> {
        Box::pin(async move { vec![account_id] })
    }
}

fn b2() -> CoachingInput {
    input("b2-rapid")
}

/// A fake answering every section validly, with `usage` per call.
fn valid_fake(usage: AiUsage) -> FakeAiClient {
    per_section(&b2(), move |k, _| {
        Ok(FakeStructured {
            usage,
            ..out(valid(k))
        })
    })
}

fn some_usage() -> AiUsage {
    AiUsage {
        input_tokens: 1200,
        output_tokens: 340,
        cache_creation_tokens: 2500,
        cache_read_tokens: 4000,
    }
}

fn unavailable() -> AiError {
    AiError::Unavailable {
        reason: UnavailableReason::Server,
        retry_after: None,
    }
}

/// Delays chosen per section in front of a client, so completion order is deterministic.
struct Staggered {
    inner: Arc<dyn AiClient>,
    input: CoachingInput,
    delays: HashMap<SectionKey, Duration>,
}

impl AiClient for Staggered {
    fn structured<'a>(
        &'a self,
        req: &'a StructuredRequest,
    ) -> BoxFuture<'a, Result<StructuredResult, AiError>> {
        Box::pin(async move {
            let k = section_of(&self.input, req);
            if let Some(d) = self.delays.get(&k) {
                tokio::time::sleep(*d).await;
            }
            self.inner.structured(req).await
        })
    }
    fn stream_turn<'a>(
        &'a self,
        req: &'a ChatTurnRequest,
        tools: Option<Arc<dyn ToolHandler>>,
    ) -> BoxStream<'a, Result<TurnEvent, AiError>> {
        self.inner.stream_turn(req, tools)
    }
    fn batch_submit<'a>(
        &'a self,
        items: &'a [BatchItem],
    ) -> BoxFuture<'a, Result<String, AiError>> {
        self.inner.batch_submit(items)
    }
    fn batch_status<'a>(&'a self, id: &'a str) -> BoxFuture<'a, Result<BatchStatus, AiError>> {
        self.inner.batch_status(id)
    }
    fn batch_results<'a>(
        &'a self,
        id: &'a str,
        resolver: &'a dyn SchemaResolver,
    ) -> BoxStream<'a, Result<BatchResultItem, AiError>> {
        self.inner.batch_results(id, resolver)
    }
}

/// Completion order overview, weaknesses, openings, climb_plan, areas (2 in flight).
fn in_order(inner: Arc<dyn AiClient>) -> Arc<dyn AiClient> {
    let delays = HashMap::from([
        (SectionKey::Weaknesses, Duration::from_millis(20)),
        (SectionKey::Openings, Duration::from_millis(200)),
        (SectionKey::ClimbPlan, Duration::from_millis(400)),
        (SectionKey::Areas, Duration::from_millis(400)),
    ]);
    Arc::new(Staggered {
        inner,
        input: b2(),
        delays,
    })
}

fn deps(pool: &PgPool, ai: Option<Arc<dyn AiClient>>, now: OffsetDateTime) -> ReportDeps {
    ReportDeps {
        pool: pool.clone(),
        queue: Queue::new(pool.clone()),
        ai,
        inputs: Arc::new(FixtureInputs(Some(load_fixture("b2-rapid")))),
        entitlements: Arc::new(FixedPlan(Plan::Pro)),
        budgets: BudgetConfig::default(),
        pricing: Arc::new(PricingTable::seed()),
        now: Arc::new(move || now),
        heartbeat_every: HEARTBEAT_EVERY,
    }
}

async fn account(pool: &PgPool) -> Uuid {
    testkit::insert_account(pool, "lichess", "t18player").await
}

async fn request(pool: &PgPool, account_id: Uuid, trigger: ReportTrigger, plan: Plan) -> Uuid {
    request_report(
        pool,
        &Queue::new(pool.clone()),
        account_id,
        TimeClass::Rapid,
        trigger,
        plan,
    )
    .await
    .unwrap()
}

fn job(run_id: Uuid) -> Job {
    let now = OffsetDateTime::now_utc();
    Job {
        id: 1,
        kind: KIND_GENERATE_REPORT.into(),
        payload: json!({ "reportRunId": run_id }),
        dedupe_key: Some(kinds::report_key(run_id)),
        user_key: None,
        priority: kinds::REPORT_PRIORITY,
        attempts: 1,
        max_attempts: kinds::REPORT_MAX_ATTEMPTS,
        lease_token: Uuid::new_v4(),
        run_at: now,
        created_at: now,
        started_at: now,
    }
}

fn ctx() -> JobContext {
    JobContext {
        cancel: CancellationToken::new(),
    }
}

async fn handle(d: &ReportDeps, run_id: Uuid) -> Result<Value, JobError> {
    GenerateReportHandler(d.clone())
        .handle(job(run_id), ctx())
        .await
}

async fn run_row(pool: &PgPool, id: Uuid) -> runs::RunRow {
    let mut c = pool.acquire().await.unwrap();
    runs::get_run(&mut c, id).await.unwrap().unwrap()
}

async fn report_of(pool: &PgPool, run_id: Uuid) -> Option<reports::ReportRow> {
    let mut c = pool.acquire().await.unwrap();
    reports::report_for_run(&mut c, run_id).await.unwrap()
}

async fn section_rows(pool: &PgPool, report_id: Uuid) -> Vec<reports::SectionRow> {
    let mut c = pool.acquire().await.unwrap();
    reports::sections(&mut c, report_id).await.unwrap()
}

/// The account's events in id order: a section event as its `section`, others as their type.
async fn event_labels(pool: &PgPool, account_id: Uuid) -> Vec<String> {
    events(pool, account_id)
        .await
        .into_iter()
        .map(|(ty, p)| {
            if ty == "report.section.ready" {
                p["section"].as_str().unwrap().to_owned()
            } else {
                ty
            }
        })
        .collect()
}

async fn events(pool: &PgPool, account_id: Uuid) -> Vec<(String, Value)> {
    sqlx::query_as("SELECT type, payload FROM user_events WHERE account_id = $1 ORDER BY id")
        .bind(account_id)
        .fetch_all(pool)
        .await
        .unwrap()
}

async fn usage_rows(pool: &PgPool, account_id: Uuid) -> Vec<(String, i64, String, i64)> {
    sqlx::query_as(
        "SELECT model, cost_micro_usd, coalesce(ref_id, ''), input_tokens::bigint FROM ai_usage \
         WHERE account_id = $1 ORDER BY id",
    )
    .bind(account_id)
    .fetch_all(pool)
    .await
    .unwrap()
}

async fn seed_spend(pool: &PgPool, account_id: Uuid, micro: i64) {
    sqlx::query(
        "INSERT INTO ai_usage (account_id, feature, model, input_tokens, output_tokens, \
         cache_creation_tokens, cache_read_tokens, cost_micro_usd) \
         VALUES ($1, 'report', 'claude-sonnet-5-5', 0, 0, 0, 0, $2)",
    )
    .bind(account_id)
    .bind(micro)
    .execute(pool)
    .await
    .unwrap();
}

const FULL: [&str; 13] = [
    "computed",
    "overview",
    "weaknesses",
    "openings",
    "climb_plan",
    "area:openings",
    "area:middlegame",
    "area:endgame",
    "area:tactics",
    "area:calculation",
    "area:time",
    "area:mindset",
    "report.ready",
];

// ---------------------------------------------------------------------------------------------
// Constants and configuration
// ---------------------------------------------------------------------------------------------

#[test]
fn constants_match_the_brief() {
    assert_eq!(FREE_REPORT_INTERVAL_DAYS, 30);
    assert_eq!(HEARTBEAT_EVERY, Duration::from_secs(15));
    assert!(RECLAIM_STALE_AFTER >= HEARTBEAT_EVERY * 3);
}

#[test]
fn budget_config_reads_its_three_variables() {
    let vars = HashMap::from([
        ("AI_BUDGET_FREE_MICRO_USD", "150000"),
        ("AI_BUDGET_PRO_MICRO_USD", "3000000"),
        ("AI_GLOBAL_MONTHLY_BUDGET_MICRO_USD", "50000000"),
    ]);
    let cfg = BudgetConfig::from_vars(|k| vars.get(k).map(|v| (*v).to_owned())).unwrap();
    assert_eq!(cfg.per_plan.free, 150_000);
    assert_eq!(cfg.per_plan.pro, 3_000_000);
    assert_eq!(cfg.global_monthly, Some(50_000_000));

    let defaults = BudgetConfig::from_vars(|_| None).unwrap();
    assert_eq!(defaults, BudgetConfig::default());
    assert_eq!(defaults.per_plan, AI_MONTHLY_BUDGET_MICRO_USD);
    assert_eq!(defaults.global_monthly, None);

    let bad = BudgetConfig::from_vars(|k| (k == "AI_BUDGET_PRO_MICRO_USD").then(|| "-1".into()));
    assert!(
        matches!(bad, Err(BudgetConfigError { ref var, .. }) if var == "AI_BUDGET_PRO_MICRO_USD")
    );
}

#[tokio::test]
async fn null_input_provider_has_no_input() {
    let p = NullInputProvider;
    assert!(
        p.coaching_input(Uuid::new_v4(), TimeClass::Rapid)
            .await
            .unwrap()
            .is_none()
    );
}

// ---------------------------------------------------------------------------------------------
// The full run
// ---------------------------------------------------------------------------------------------

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn full_run_publishes_sections_progressively(pool: PgPool) {
    let a = account(&pool).await;
    let d = deps(
        &pool,
        Some(in_order(Arc::new(valid_fake(some_usage())))),
        now_micros(),
    );
    let run = request(&pool, a, ReportTrigger::Onboarding, Plan::Pro).await;
    handle(&d, run).await.unwrap();

    assert_eq!(event_labels(&pool, a).await, FULL);
    let r = run_row(&pool, run).await;
    assert_eq!(r.status, RunStatus::Done);
    assert!(r.is_first_report);
    let rep = report_of(&pool, run).await.unwrap();
    assert_eq!(r.report_id, Some(rep.id));
    assert_eq!(rep.version, 1);
    assert_eq!(rep.status, ReportStatus::Complete);
    assert_eq!(section_rows(&pool, rep.id).await.len(), 5);
    let (_, ready) = events(&pool, a).await.pop().unwrap();
    assert_eq!(ready, json!({"reportId": rep.id, "status": "complete"}));

    // The snapshot is stored with its hashes.
    assert!(r.input.is_some());
    assert_eq!(r.input_hash.as_deref().map(str::len), Some(64));
    assert_eq!(r.prompt_hash.as_deref().map(str::len), Some(64));

    let run2 = request(&pool, a, ReportTrigger::Manual, Plan::Pro).await;
    assert!(!run_row(&pool, run2).await.is_first_report);
    handle(&d, run2).await.unwrap();
    assert_eq!(report_of(&pool, run2).await.unwrap().version, 2);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn request_report_enqueues_one_job_after_the_commit(pool: PgPool) {
    let a = account(&pool).await;
    let run = request(&pool, a, ReportTrigger::Manual, Plan::Free).await;
    let r = run_row(&pool, run).await;
    assert_eq!(r.status, RunStatus::Queued);
    assert_eq!(r.plan, Plan::Free);
    assert_eq!(r.prompt_version, "report@1");
    let jobs: Vec<(String, Value)> =
        sqlx::query_as("SELECT kind, payload FROM jobs WHERE dedupe_key = $1")
            .bind(kinds::report_key(run))
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(
        jobs,
        vec![(KIND_GENERATE_REPORT.to_owned(), json!({"reportRunId": run}))]
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn event_and_section_commit_together(pool: PgPool) {
    let a = account(&pool).await;
    let mut listener = sqlx::postgres::PgListener::connect_with(&pool)
        .await
        .unwrap();
    listener.listen("user_events").await.unwrap();
    let checker = {
        let pool = pool.clone();
        tokio::spawn(async move {
            let mut checked = 0;
            loop {
                let n = listener.recv().await.unwrap();
                let v: Value = serde_json::from_str(n.payload()).unwrap();
                if v["type"] == "report.ready" {
                    return checked;
                }
                if v["type"] != "report.section.ready" {
                    continue;
                }
                let p = &v["payload"];
                let report_id: Uuid = serde_json::from_value(p["reportId"].clone()).unwrap();
                let section = p["section"].as_str().unwrap();
                let mut c = pool.acquire().await.unwrap();
                if section == "computed" {
                    assert!(
                        reports::get_report(&mut c, report_id)
                            .await
                            .unwrap()
                            .is_some()
                    );
                } else {
                    let key = if section.starts_with("area:") {
                        "areas"
                    } else {
                        section
                    };
                    let rows = reports::sections(&mut c, report_id).await.unwrap();
                    assert!(
                        rows.iter().any(|r| r.section.as_str() == key),
                        "{section}: no row at its event"
                    );
                }
                checked += 1;
            }
        })
    };
    let d = deps(
        &pool,
        Some(Arc::new(valid_fake(some_usage()))),
        now_micros(),
    );
    let run = request(&pool, a, ReportTrigger::Onboarding, Plan::Pro).await;
    handle(&d, run).await.unwrap();
    let checked = tokio::time::timeout(Duration::from_secs(30), checker)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(checked, 12);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn usage_rows_match_cost(pool: PgPool) {
    let a = account(&pool).await;
    let d = deps(
        &pool,
        Some(Arc::new(valid_fake(some_usage()))),
        now_micros(),
    );
    let run = request(&pool, a, ReportTrigger::Onboarding, Plan::Pro).await;
    handle(&d, run).await.unwrap();

    let rows = usage_rows(&pool, a).await;
    assert_eq!(rows.len(), 5, "one row per fake call");
    let want = cost_micro_usd(
        &PricingTable::seed(),
        Models::REPORT.as_str(),
        &some_usage(),
        false,
    )
    .unwrap();
    for (model, cost, ref_id, input_tokens) in rows {
        assert_eq!(model, "claude-sonnet-5-5");
        assert_eq!(cost, i64::try_from(want).unwrap());
        assert_eq!(ref_id, run.to_string());
        assert_eq!(input_tokens, 1200);
    }
    let features: Vec<String> =
        sqlx::query_scalar("SELECT DISTINCT feature::text FROM ai_usage WHERE account_id = $1")
            .bind(a)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(features, vec!["report"]);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn two_model_call_meters_two_rows(pool: PgPool) {
    let a = account(&pool).await;
    let haiku = AiUsage {
        input_tokens: 50,
        output_tokens: 10,
        cache_creation_tokens: 0,
        cache_read_tokens: 0,
    };
    let fake = per_section(&b2(), move |k, _| {
        let mut s = out(valid(k));
        if k == SectionKey::Overview {
            s.model_usage = Some(vec![
                ModelUsage {
                    model: "claude-sonnet-5-5".into(),
                    usage: some_usage(),
                },
                ModelUsage {
                    model: "claude-haiku-5-5".into(),
                    usage: haiku,
                },
            ]);
        }
        Ok(s)
    });
    let d = deps(&pool, Some(Arc::new(fake)), now_micros());
    let run = request(&pool, a, ReportTrigger::Onboarding, Plan::Pro).await;
    handle(&d, run).await.unwrap();

    let rows = usage_rows(&pool, a).await;
    assert_eq!(rows.len(), 6, "the overview call touched two models");
    let h: Vec<_> = rows.iter().filter(|r| r.0 == "claude-haiku-5-5").collect();
    assert_eq!(h.len(), 1);
    let want = cost_micro_usd(&PricingTable::seed(), "claude-haiku-5-5", &haiku, false).unwrap();
    assert_eq!(h[0].1, i64::try_from(want).unwrap());
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn meter_prices_unknown_models_at_the_requested_rates_and_keeps_the_sdk_estimate(
    pool: PgPool,
) {
    let a = account(&pool).await;
    let calls = [
        CallRecord {
            model: "claude-mystery-9".into(),
            usage: some_usage(),
            batch: false,
            sdk_cost_usd: Some(0.01234),
        },
        CallRecord {
            model: "claude-sonnet-5-5".into(),
            usage: some_usage(),
            batch: true,
            sdk_cost_usd: None,
        },
    ];
    let mut c = pool.acquire().await.unwrap();
    meter(
        &mut c,
        &PricingTable::seed(),
        a,
        AiFeature::Report,
        &calls,
        "ref-1",
        AiModel::Sonnet55,
    )
    .await
    .unwrap();
    let rows: Vec<(String, bool, i64, Option<i64>)> = sqlx::query_as(
        "SELECT model, batch, cost_micro_usd, sdk_estimate_micro_usd FROM ai_usage \
         WHERE account_id = $1 ORDER BY id",
    )
    .bind(a)
    .fetch_all(&pool)
    .await
    .unwrap();
    let t = PricingTable::seed();
    let sync = cost_micro_usd(&t, "claude-sonnet-5-5", &some_usage(), false).unwrap();
    let batch = cost_micro_usd(&t, "claude-sonnet-5-5", &some_usage(), true).unwrap();
    assert_eq!(
        rows,
        vec![
            (
                "claude-mystery-9".into(),
                false,
                i64::try_from(sync).unwrap(),
                Some(12_340)
            ),
            (
                "claude-sonnet-5-5".into(),
                true,
                i64::try_from(batch).unwrap(),
                None
            ),
        ]
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn disclosure_is_stored_on_complete_reports(pool: PgPool) {
    let a = account(&pool).await;
    let now = now_micros();
    let d = deps(&pool, Some(Arc::new(valid_fake(some_usage()))), now);
    let run = request(&pool, a, ReportTrigger::Onboarding, Plan::Pro).await;
    handle(&d, run).await.unwrap();
    let rep = report_of(&pool, run).await.unwrap();
    assert_eq!(rep.status, ReportStatus::Complete);
    assert_eq!(
        rep.ai_disclosure,
        Some(serde_json::to_value(make_disclosure("claude-sonnet-5-5", "report@1", now)).unwrap())
    );
    let mut c = pool.acquire().await.unwrap();
    let payload = ai::generate::payload::load_report_payload(&mut c, rep.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(payload.status, ReportStatus::Complete);
    assert!(payload.ai_disclosure.is_some());
}

// ---------------------------------------------------------------------------------------------
// Outages, retries and resume
// ---------------------------------------------------------------------------------------------

/// Fails openings with `Unavailable`; weaknesses finishes after openings has failed.
fn openings_down() -> Arc<dyn AiClient> {
    let fake = per_section(&b2(), |k, _| {
        if k == SectionKey::Openings {
            Err(unavailable())
        } else {
            Ok(out(valid(k)))
        }
    });
    Arc::new(Staggered {
        inner: Arc::new(fake),
        input: b2(),
        delays: HashMap::from([(SectionKey::Weaknesses, Duration::from_millis(200))]),
    })
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn ai_unavailable_mid_run_releases_with_the_schedule(pool: PgPool) {
    let a = account(&pool).await;
    let now = now_micros();
    let d = deps(&pool, Some(openings_down()), now);
    let run = request(&pool, a, ReportTrigger::Onboarding, Plan::Pro).await;

    let res = handle(&d, run).await;
    match res {
        Err(JobError::Release { delay, .. }) => assert_eq!(delay, Duration::from_secs(300)),
        other => panic!("expected a release, got {other:?}"),
    }
    let r = run_row(&pool, run).await;
    assert_eq!(r.status, RunStatus::AwaitingRetry);
    assert_eq!(r.retry_count, 1);
    assert_eq!(r.next_attempt_at, Some(now + Duration::from_secs(300)));
    let rep = report_of(&pool, run).await.unwrap();
    assert_eq!(rep.status, ReportStatus::Partial);
    assert!(
        rep.ai_disclosure.is_some(),
        "a partial report shows AI text"
    );
    let keys: Vec<SectionKey> = section_rows(&pool, rep.id)
        .await
        .iter()
        .map(|s| s.section)
        .collect();
    assert_eq!(keys, vec![SectionKey::Overview, SectionKey::Weaknesses]);
    let evs = events(&pool, a).await;
    let (ty, p) = evs.last().unwrap();
    assert_eq!(ty, "report.degraded");
    assert_eq!(p["reason"], "unavailable");
    assert_eq!(
        p["retryAt"],
        json!(common::js_time::to_iso_string(
            now + Duration::from_secs(300)
        ))
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn resume_regenerates_only_missing_sections(pool: PgPool) {
    let a = account(&pool).await;
    let now = now_micros();
    let run = request(&pool, a, ReportTrigger::Onboarding, Plan::Pro).await;
    let _ = handle(&deps(&pool, Some(openings_down()), now), run).await;
    let before = events(&pool, a).await.len();
    let rows_before = usage_rows(&pool, a).await.len();

    let fake = Arc::new(valid_fake(some_usage()));
    let d = deps(&pool, Some(fake.clone() as Arc<dyn AiClient>), now);
    handle(&d, run).await.unwrap();

    let input = b2();
    let called: Vec<SectionKey> = fake
        .calls()
        .iter()
        .map(|c| {
            let task = c.request_json["messages"][0]["content"]
                .as_array()
                .unwrap()
                .last()
                .cloned()
                .unwrap();
            ai::output::codes::SECTION_KEYS
                .into_iter()
                .find(|k| {
                    ai::generate::request::build_section_request(&input, *k, true).messages[0]
                        .content
                        .last()
                        == Some(&task)
                })
                .unwrap()
        })
        .collect();
    assert_eq!(called.len(), 3);
    assert!(!called.contains(&SectionKey::Overview));
    assert!(!called.contains(&SectionKey::Weaknesses));

    let rep = report_of(&pool, run).await.unwrap();
    assert_eq!(rep.version, 1);
    assert_eq!(rep.status, ReportStatus::Complete);
    assert_eq!(run_row(&pool, run).await.status, RunStatus::Done);
    // Only the new sections are announced, each once.
    let labels = event_labels(&pool, a).await;
    let new: Vec<&str> = labels[before..].iter().map(String::as_str).collect();
    assert_eq!(new.len(), 10);
    for l in [
        "openings",
        "climb_plan",
        "area:openings",
        "area:mindset",
        "report.ready",
    ] {
        assert_eq!(labels.iter().filter(|x| *x == l).count(), 1, "{l}");
    }
    for l in ["computed", "overview", "weaknesses"] {
        assert_eq!(labels.iter().filter(|x| *x == l).count(), 1, "{l}");
    }
    // No double metering: the earlier rows stay, three new ones.
    assert_eq!(usage_rows(&pool, a).await.len(), rows_before + 3);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_paid_call_of_a_failed_section_is_metered(pool: PgPool) {
    let a = account(&pool).await;
    let ok = Arc::new(AtomicUsize::new(0));
    let ok2 = ok.clone();
    // Openings: attempt 1 is a refusal (paid), its repair is unavailable.
    let fake = per_section(&b2(), move |k, attempt| {
        if k == SectionKey::Openings {
            if attempt == 1 {
                ok2.fetch_add(1, Ordering::SeqCst);
                return Ok(FakeStructured {
                    usage: some_usage(),
                    ..stop("refusal")
                });
            }
            return Err(unavailable());
        }
        ok2.fetch_add(1, Ordering::SeqCst);
        Ok(out(valid(k)))
    });
    let ai = Arc::new(Staggered {
        inner: Arc::new(fake),
        input: b2(),
        delays: HashMap::from([(SectionKey::Weaknesses, Duration::from_millis(200))]),
    });
    let d = deps(&pool, Some(ai), now_micros());
    let run = request(&pool, a, ReportTrigger::Onboarding, Plan::Pro).await;
    let res = handle(&d, run).await;
    assert!(matches!(res, Err(JobError::Release { .. })), "{res:?}");
    assert_eq!(ok.load(Ordering::SeqCst), 3);
    let rows = usage_rows(&pool, a).await;
    assert_eq!(rows.len(), 3, "every paid call is metered: {rows:?}");
    assert_eq!(rows.iter().filter(|r| r.3 == 1200).count(), 1);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn retries_exhausted_fails_and_publishes_job_failed(pool: PgPool) {
    let a = account(&pool).await;
    let run = request(&pool, a, ReportTrigger::Onboarding, Plan::Pro).await;
    sqlx::query("UPDATE report_runs SET retry_count = 5 WHERE id = $1")
        .bind(run)
        .execute(&pool)
        .await
        .unwrap();
    let fake = per_section(&b2(), |_, _| Err(unavailable()));
    let d = deps(&pool, Some(Arc::new(fake)), now_micros());
    handle(&d, run).await.unwrap();
    let r = run_row(&pool, run).await;
    assert_eq!(r.status, RunStatus::Failed);
    assert_eq!(r.last_error.as_deref(), Some("ai_unavailable"));
    let (ty, p) = events(&pool, a).await.pop().unwrap();
    assert_eq!(ty, "job.failed");
    assert_eq!(
        p,
        json!({"kind": "generate-report", "reason": "ai_unavailable"})
    );
    // The computed report still ships.
    let rep = report_of(&pool, run).await.unwrap();
    assert_eq!(rep.status, ReportStatus::ComputedOnly);
    assert_eq!(rep.ai_disclosure, None);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn auth_failure_fails_the_run(pool: PgPool) {
    let a = account(&pool).await;
    let run = request(&pool, a, ReportTrigger::Onboarding, Plan::Pro).await;
    let fake = per_section(&b2(), |_, _| Err(AiError::Auth { status: 401 }));
    let d = deps(&pool, Some(Arc::new(fake)), now_micros());
    let res = handle(&d, run).await;
    assert!(
        matches!(res, Err(JobError::Unrecoverable { .. })),
        "{res:?}"
    );
    let r = run_row(&pool, run).await;
    assert_eq!(r.status, RunStatus::Failed);
    assert_eq!(r.last_error.as_deref(), Some("ai_auth"));
    let (ty, p) = events(&pool, a).await.pop().unwrap();
    assert_eq!(ty, "job.failed");
    assert_eq!(p["reason"], "ai_auth");
}

// ---------------------------------------------------------------------------------------------
// Degradation without AI
// ---------------------------------------------------------------------------------------------

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn no_api_key_is_computed_only_without_retry(pool: PgPool) {
    let a = account(&pool).await;
    let d = deps(&pool, None, now_micros());
    let run = request(&pool, a, ReportTrigger::Onboarding, Plan::Pro).await;
    handle(&d, run).await.unwrap();
    let r = run_row(&pool, run).await;
    assert_eq!(r.status, RunStatus::Done);
    assert_eq!(r.ai_disabled_reason.as_deref(), Some("no_api_key"));
    assert_eq!(r.retry_count, 0);
    assert_eq!(r.next_attempt_at, None);
    let rep = report_of(&pool, run).await.unwrap();
    assert_eq!(rep.status, ReportStatus::ComputedOnly);
    assert_eq!(rep.ai_disclosure, None);
    assert!(section_rows(&pool, rep.id).await.is_empty());
    assert_eq!(
        events(&pool, a).await,
        vec![
            (
                "report.section.ready".to_owned(),
                json!({"reportId": rep.id, "reportRunId": run, "section": "computed", "narrative": false})
            ),
            (
                "report.degraded".to_owned(),
                json!({"reportId": rep.id, "reason": "ai_disabled", "retryAt": null})
            ),
        ]
    );
}

async fn assert_budget_stop(pool: &PgPool, a: Uuid, run: Uuid, fake: &FakeAiClient) {
    assert!(fake.calls().is_empty(), "no model call past the budget");
    let r = run_row(pool, run).await;
    assert_eq!(r.status, RunStatus::Done);
    assert_eq!(r.ai_disabled_reason.as_deref(), Some("budget"));
    let rep = report_of(pool, run).await.unwrap();
    assert_eq!(rep.status, ReportStatus::ComputedOnly);
    let (ty, p) = events(pool, a).await.pop().unwrap();
    assert_eq!(ty, "report.degraded");
    assert_eq!(p["reason"], "budget");
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn budget_exceeded_makes_no_calls(pool: PgPool) {
    let a = account(&pool).await;
    seed_spend(&pool, a, 200_000).await;
    let fake = Arc::new(valid_fake(some_usage()));
    let mut d = deps(&pool, Some(fake.clone() as Arc<dyn AiClient>), now_micros());
    d.entitlements = Arc::new(FixedPlan(Plan::Free));
    let run = request(&pool, a, ReportTrigger::Onboarding, Plan::Free).await;
    handle(&d, run).await.unwrap();
    assert_budget_stop(&pool, a, run, &fake).await;
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn global_budget_exceeded_makes_no_calls(pool: PgPool) {
    let a = account(&pool).await;
    let other = testkit::insert_account(&pool, "lichess", "someone-else").await;
    seed_spend(&pool, other, 1_000).await;
    let fake = Arc::new(valid_fake(some_usage()));
    let mut d = deps(&pool, Some(fake.clone() as Arc<dyn AiClient>), now_micros());
    d.budgets.global_monthly = Some(1_000);
    let run = request(&pool, a, ReportTrigger::Onboarding, Plan::Pro).await;
    handle(&d, run).await.unwrap();
    assert_budget_stop(&pool, a, run, &fake).await;
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn budget_reached_mid_run_keeps_the_sections(pool: PgPool) {
    let a = account(&pool).await;
    // Free budget 200 000; each call costs more than half of it, so the third check refuses.
    let big = AiUsage {
        input_tokens: 60_000,
        output_tokens: 0,
        cache_creation_tokens: 0,
        cache_read_tokens: 0,
    };
    let fake = Arc::new(valid_fake(big));
    let mut d = deps(&pool, Some(fake.clone() as Arc<dyn AiClient>), now_micros());
    d.entitlements = Arc::new(FixedPlan(Plan::Free));
    let run = request(&pool, a, ReportTrigger::Onboarding, Plan::Free).await;
    handle(&d, run).await.unwrap();
    let r = run_row(&pool, run).await;
    assert_eq!(r.status, RunStatus::Done);
    assert_eq!(r.ai_disabled_reason.as_deref(), Some("budget"));
    let rep = report_of(&pool, run).await.unwrap();
    assert_eq!(rep.status, ReportStatus::Partial);
    let n = section_rows(&pool, rep.id).await.len();
    assert!((1..5).contains(&n), "{n} sections");
    assert_eq!(usage_rows(&pool, a).await.len(), fake.calls().len());
    let (ty, p) = events(&pool, a).await.pop().unwrap();
    assert_eq!(ty, "report.degraded");
    assert_eq!(p["reason"], "budget");
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn budget_guard_checks_scope_and_global_since_the_month_start(pool: PgPool) {
    let a = account(&pool).await;
    let b = testkit::insert_account(&pool, "lichess", "partner").await;
    seed_spend(&pool, b, 150_000).await;
    // An old row (last month or earlier) does not count.
    sqlx::query(
        "INSERT INTO ai_usage (account_id, feature, model, input_tokens, output_tokens, \
         cache_creation_tokens, cache_read_tokens, cost_micro_usd, created_at) \
         VALUES ($1, 'report', 'm', 0, 0, 0, 0, 999999999, now() - interval '40 days')",
    )
    .bind(a)
    .execute(&pool)
    .await
    .unwrap();
    let cfg = BudgetConfig::default();
    let now = OffsetDateTime::now_utc();
    let only_a = budget_guard(pool.clone(), vec![a], Plan::Free, cfg, now);
    assert_eq!(only_a().await, Ok(()));
    let both = budget_guard(pool.clone(), vec![a, b], Plan::Free, cfg, now);
    assert_eq!(both().await, Ok(()));
    seed_spend(&pool, a, 50_000).await;
    assert_eq!(both().await, Err(AiError::Budget));
    assert_eq!(only_a().await, Ok(()));
    let global = BudgetConfig {
        global_monthly: Some(200_001),
        ..cfg
    };
    let g = budget_guard(pool.clone(), vec![a], Plan::Pro, global, now);
    assert_eq!(g().await, Ok(()));
    seed_spend(&pool, b, 1).await;
    assert_eq!(g().await, Err(AiError::Budget));
}

// ---------------------------------------------------------------------------------------------
// Gates and input
// ---------------------------------------------------------------------------------------------

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn invalid_input_fails_without_report_row(pool: PgPool) {
    let a = account(&pool).await;
    let mut d = deps(
        &pool,
        Some(Arc::new(valid_fake(some_usage()))),
        now_micros(),
    );
    d.inputs = Arc::new(FixtureInputs(Some(json!({}))));
    let run = request(&pool, a, ReportTrigger::Onboarding, Plan::Pro).await;
    let res = handle(&d, run).await;
    assert!(
        matches!(res, Err(JobError::Unrecoverable { .. })),
        "{res:?}"
    );
    assert_eq!(run_row(&pool, run).await.status, RunStatus::Failed);
    assert!(report_of(&pool, run).await.is_none());

    // No input at all is the same.
    d.inputs = Arc::new(NullInputProvider);
    let run2 = request(&pool, a, ReportTrigger::Onboarding, Plan::Pro).await;
    let res = handle(&d, run2).await;
    assert!(
        matches!(res, Err(JobError::Unrecoverable { .. })),
        "{res:?}"
    );
    assert!(report_of(&pool, run2).await.is_none());
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn free_plan_interval(pool: PgPool) {
    let a = account(&pool).await;
    let now = now_micros();
    let mut d = deps(&pool, Some(Arc::new(valid_fake(some_usage()))), now);
    d.entitlements = Arc::new(FixedPlan(Plan::Free));
    let first = request(&pool, a, ReportTrigger::Onboarding, Plan::Free).await;
    handle(&d, first).await.unwrap();

    let later = now + time::Duration::days(10);
    d.now = Arc::new(move || later);
    let run = request(&pool, a, ReportTrigger::Manual, Plan::Free).await;
    let res = handle(&d, run).await;
    assert!(
        matches!(res, Err(JobError::Unrecoverable { .. })),
        "{res:?}"
    );
    let r = run_row(&pool, run).await;
    assert_eq!(r.status, RunStatus::Failed);
    assert_eq!(r.last_error.as_deref(), Some("plan_interval"));
    assert!(report_of(&pool, run).await.is_none());

    // Onboarding is never gated; a pro plan is never gated.
    let onboarding = request(&pool, a, ReportTrigger::Onboarding, Plan::Free).await;
    handle(&d, onboarding).await.unwrap();
    assert_eq!(run_row(&pool, onboarding).await.status, RunStatus::Done);
    d.entitlements = Arc::new(FixedPlan(Plan::Pro));
    let pro = request(&pool, a, ReportTrigger::Manual, Plan::Pro).await;
    handle(&d, pro).await.unwrap();
    assert_eq!(run_row(&pool, pro).await.status, RunStatus::Done);

    // 31 days on, the free plan may run again.
    let much_later = now + time::Duration::days(31);
    d.entitlements = Arc::new(FixedPlan(Plan::Free));
    d.now = Arc::new(move || much_later);
    sqlx::query("UPDATE reports SET created_at = created_at - interval '31 days'")
        .execute(&pool)
        .await
        .unwrap();
    let again = request(&pool, a, ReportTrigger::Manual, Plan::Free).await;
    handle(&d, again).await.unwrap();
    assert_eq!(run_row(&pool, again).await.status, RunStatus::Done);
}

// ---------------------------------------------------------------------------------------------
// Idempotency, concurrency, heartbeat
// ---------------------------------------------------------------------------------------------

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn concurrent_handlers_on_one_run_do_not_duplicate(pool: PgPool) {
    let a = account(&pool).await;
    let d = deps(
        &pool,
        Some(Arc::new(valid_fake(some_usage()))),
        now_micros(),
    );
    let run = request(&pool, a, ReportTrigger::Onboarding, Plan::Pro).await;
    let h1 = GenerateReportHandler(d.clone());
    let h2 = GenerateReportHandler(d.clone());
    let (r1, r2) = tokio::join!(h1.handle(job(run), ctx()), h2.handle(job(run), ctx()));
    r1.unwrap();
    r2.unwrap();
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM reports WHERE account_id = $1")
        .bind(a)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 1);
    let rep = report_of(&pool, run).await.unwrap();
    assert_eq!(section_rows(&pool, rep.id).await.len(), 5);
    assert_eq!(event_labels(&pool, a).await.len(), FULL.len());
    let labels = event_labels(&pool, a).await;
    for l in FULL {
        assert_eq!(labels.iter().filter(|x| *x == l).count(), 1, "{l}");
    }
    assert_eq!(usage_rows(&pool, a).await.len(), 5);

    // A finished run is a no-op.
    handle(&d, run).await.unwrap();
    assert_eq!(event_labels(&pool, a).await.len(), FULL.len());
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_stale_running_run_is_reclaimed_and_the_old_claim_is_fenced(pool: PgPool) {
    let a = account(&pool).await;
    let run = request(&pool, a, ReportTrigger::Onboarding, Plan::Pro).await;
    let mut c = pool.acquire().await.unwrap();
    let old = runs::claim_run(&mut c, run).await.unwrap().unwrap();
    let old = runs::RunClaim {
        id: run,
        started_at: old.started_at.unwrap(),
    };
    // A fresh claim held by another worker: the redelivered job leaves it alone.
    let d = deps(
        &pool,
        Some(Arc::new(valid_fake(some_usage()))),
        now_micros(),
    );
    handle(&d, run).await.unwrap();
    assert_eq!(run_row(&pool, run).await.status, RunStatus::Running);
    assert!(runs::heartbeat_claim(&mut c, &old).await.unwrap());

    // Its worker died: the heartbeat goes stale and the next delivery reclaims the run.
    sqlx::query(
        "UPDATE report_runs SET heartbeat_at = now() - make_interval(secs => $2) WHERE id = $1",
    )
    .bind(run)
    .bind(RECLAIM_STALE_AFTER.as_secs_f64() + 5.0)
    .execute(&pool)
    .await
    .unwrap();
    handle(&d, run).await.unwrap();
    assert_eq!(run_row(&pool, run).await.status, RunStatus::Done);
    // The zombie's writes no longer land.
    assert!(!runs::heartbeat_claim(&mut c, &old).await.unwrap());
    assert!(!runs::release_claim(&mut c, &old).await.unwrap());
    assert_eq!(run_row(&pool, run).await.status, RunStatus::Done);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn heartbeat_advances(pool: PgPool) {
    let a = account(&pool).await;
    let input = b2();
    let fake = FakeAiClient::new(FakeScript {
        structured: Some(Box::new(move |req, _| {
            Ok(out(valid(section_of(&input, req))))
        })),
        delay: Some(Duration::from_millis(300)),
        ..FakeScript::default()
    });
    let mut d = deps(&pool, Some(Arc::new(fake)), now_micros());
    d.heartbeat_every = Duration::from_millis(50);
    let run = request(&pool, a, ReportTrigger::Onboarding, Plan::Pro).await;
    handle(&d, run).await.unwrap();
    let r = run_row(&pool, run).await;
    assert_eq!(r.status, RunStatus::Done);
    let beat = r.heartbeat_at.unwrap() - r.started_at.unwrap();
    assert!(
        beat >= time::Duration::milliseconds(500),
        "heartbeat only {beat} after the claim"
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_cancelled_run_is_released_for_the_next_delivery(pool: PgPool) {
    let a = account(&pool).await;
    let d = deps(
        &pool,
        Some(Arc::new(valid_fake(some_usage()))),
        now_micros(),
    );
    let run = request(&pool, a, ReportTrigger::Onboarding, Plan::Pro).await;
    let c = ctx();
    c.cancel.cancel();
    let res = GenerateReportHandler(d.clone()).handle(job(run), c).await;
    assert!(matches!(res, Err(JobError::Release { .. })), "{res:?}");
    assert_eq!(run_row(&pool, run).await.status, RunStatus::Queued);
    handle(&d, run).await.unwrap();
    assert_eq!(run_row(&pool, run).await.status, RunStatus::Done);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_dead_job_fails_its_run(pool: PgPool) {
    let a = account(&pool).await;
    let d = deps(&pool, None, now_micros());
    let run = request(&pool, a, ReportTrigger::Onboarding, Plan::Pro).await;
    GenerateReportHandler(d.clone())
        .on_dead(&job(run), "boom")
        .await;
    let r = run_row(&pool, run).await;
    assert_eq!(r.status, RunStatus::Failed);
    assert_eq!(r.last_error.as_deref(), Some("job_dead"));
    let (ty, p) = events(&pool, a).await.pop().unwrap();
    assert_eq!(ty, "job.failed");
    assert_eq!(p, json!({"kind": "generate-report", "reason": "job_dead"}));
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_worker_whose_run_was_taken_over_stops_writing_but_meters_its_calls(pool: PgPool) {
    let a = account(&pool).await;
    let input = b2();
    let paid = Arc::new(AtomicUsize::new(0));
    let p = paid.clone();
    let fake = FakeAiClient::new(FakeScript {
        structured: Some(Box::new(move |req, _| {
            p.fetch_add(1, Ordering::SeqCst);
            Ok(out(valid(section_of(&input, req))))
        })),
        delay: Some(Duration::from_millis(300)),
        ..FakeScript::default()
    });
    let d = deps(&pool, Some(Arc::new(fake)), now_micros());
    let run = request(&pool, a, ReportTrigger::Onboarding, Plan::Pro).await;
    let worker = tokio::spawn({
        let d = d.clone();
        async move { handle(&d, run).await }
    });
    // While overview is in flight, another delivery takes the run over.
    tokio::time::sleep(Duration::from_millis(150)).await;
    let mut c = pool.acquire().await.unwrap();
    let new = runs::claim_run_or_stale(&mut c, run, Duration::ZERO)
        .await
        .unwrap()
        .expect("taken over");
    let res = worker.await.unwrap().unwrap();
    assert_eq!(res["lost"], json!(true), "{res}");

    let r = run_row(&pool, run).await;
    assert_eq!(
        r.status,
        RunStatus::Running,
        "the old worker did not finish the run"
    );
    assert_eq!(r.started_at, new.started_at);
    let rep = report_of(&pool, run).await.unwrap();
    assert!(section_rows(&pool, rep.id).await.is_empty());
    assert_eq!(event_labels(&pool, a).await, vec!["computed"]);
    // The overview call was paid, so it is metered although it was not stored.
    assert_eq!(paid.load(Ordering::SeqCst), 1);
    assert_eq!(usage_rows(&pool, a).await.len(), 1);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn meter_keyed_meters_each_call_once(pool: PgPool) {
    let a = account(&pool).await;
    let call = |model: &str| CallRecord {
        model: model.into(),
        usage: some_usage(),
        batch: false,
        sdk_cost_usd: Some(0.0125),
    };
    let calls = [call("claude-sonnet-5-5"), call("claude-haiku-5-5")];
    let t = PricingTable::seed();
    let mut c = pool.acquire().await.unwrap();
    for prefix in ["msg-1:attempt-1", "msg-1:attempt-1", "msg-1:attempt-2"] {
        ai::jobs::budget::meter_keyed(
            &mut c,
            &t,
            a,
            AiFeature::Chat,
            &calls,
            "msg-1",
            AiModel::Sonnet55,
            prefix,
        )
        .await
        .unwrap();
    }
    let rows: Vec<(String, String, Option<i64>)> = sqlx::query_as(
        "SELECT idempotency_key, model, sdk_estimate_micro_usd FROM ai_usage \
         WHERE account_id = $1 ORDER BY id",
    )
    .bind(a)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        rows,
        vec![
            (
                "msg-1:attempt-1:0".into(),
                "claude-sonnet-5-5".into(),
                Some(12_500)
            ),
            (
                "msg-1:attempt-1:1".into(),
                "claude-haiku-5-5".into(),
                Some(12_500)
            ),
            (
                "msg-1:attempt-2:0".into(),
                "claude-sonnet-5-5".into(),
                Some(12_500)
            ),
            (
                "msg-1:attempt-2:1".into(),
                "claude-haiku-5-5".into(),
                Some(12_500)
            ),
        ],
        "a re-run of the same write meters nothing twice"
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn report_usage_is_keyed_per_claim_and_section(pool: PgPool) {
    let a = account(&pool).await;
    let now = now_micros();
    let run = request(&pool, a, ReportTrigger::Onboarding, Plan::Pro).await;
    let _ = handle(&deps(&pool, Some(openings_down()), now), run).await;
    handle(
        &deps(&pool, Some(Arc::new(valid_fake(some_usage()))), now),
        run,
    )
    .await
    .unwrap();
    let keys: Vec<Option<String>> =
        sqlx::query_scalar("SELECT idempotency_key FROM ai_usage WHERE account_id = $1")
            .bind(a)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(keys.len(), 5);
    let keys: std::collections::BTreeSet<String> = keys.into_iter().map(Option::unwrap).collect();
    assert_eq!(keys.len(), 5, "every paid call has its own key: {keys:?}");
    let prefix = format!("report:{run}:");
    assert!(keys.iter().all(|k| k.starts_with(&prefix)), "{keys:?}");
    // Two claims: overview and weaknesses under the first, three sections under the second.
    let claims: std::collections::BTreeSet<&str> =
        keys.iter().map(|k| k.split(':').nth(2).unwrap()).collect();
    assert_eq!(claims.len(), 2, "{keys:?}");
}

// ---------------------------------------------------------------------------------------------
// Fix round 1: every paid call is metered on every path (cancel, degrade, fail, takeover), and
// a zombie can never finish a run.
// ---------------------------------------------------------------------------------------------

/// The token count that marks overview's paid first attempt in `ai_usage`.
const MARK: u64 = 7_777;

/// Overview's attempt 1 is a paid refusal (marked), so a repair follows, answered by `repair`.
/// Every other section is valid.
fn overview_repair(
    repair: impl Fn() -> Result<FakeStructured, AiError> + Send + Sync + 'static,
) -> FakeAiClient {
    per_section(&b2(), move |k, attempt| {
        if k == SectionKey::Overview {
            if attempt == 1 {
                return Ok(FakeStructured {
                    usage: AiUsage {
                        input_tokens: MARK,
                        output_tokens: 0,
                        cache_creation_tokens: 0,
                        cache_read_tokens: 0,
                    },
                    ..stop("refusal")
                });
            }
            return repair();
        }
        Ok(out(valid(k)))
    })
}

async fn marked_rows(pool: &PgPool, account_id: Uuid) -> usize {
    usage_rows(pool, account_id)
        .await
        .iter()
        .filter(|r| r.3 == MARK as i64)
        .count()
}

type Hook = Box<dyn Fn(usize) -> BoxFuture<'static, ()> + Send + Sync>;

/// Delegates to `inner`; `before(n)` runs before structured call `n` (0-based) is sent and
/// `after(n)` after it returned.
struct Hooked {
    inner: Arc<dyn AiClient>,
    n: AtomicUsize,
    before: Hook,
    after: Hook,
}

fn nothing() -> Hook {
    Box::new(|_| Box::pin(async {}))
}

impl AiClient for Hooked {
    fn structured<'a>(
        &'a self,
        req: &'a StructuredRequest,
    ) -> BoxFuture<'a, Result<StructuredResult, AiError>> {
        Box::pin(async move {
            let n = self.n.fetch_add(1, Ordering::SeqCst);
            (self.before)(n).await;
            let r = self.inner.structured(req).await;
            (self.after)(n).await;
            r
        })
    }
    fn stream_turn<'a>(
        &'a self,
        req: &'a ChatTurnRequest,
        tools: Option<Arc<dyn ToolHandler>>,
    ) -> BoxStream<'a, Result<TurnEvent, AiError>> {
        self.inner.stream_turn(req, tools)
    }
    fn batch_submit<'a>(
        &'a self,
        items: &'a [BatchItem],
    ) -> BoxFuture<'a, Result<String, AiError>> {
        self.inner.batch_submit(items)
    }
    fn batch_status<'a>(&'a self, id: &'a str) -> BoxFuture<'a, Result<BatchStatus, AiError>> {
        self.inner.batch_status(id)
    }
    fn batch_results<'a>(
        &'a self,
        id: &'a str,
        resolver: &'a dyn SchemaResolver,
    ) -> BoxStream<'a, Result<BatchResultItem, AiError>> {
        self.inner.batch_results(id, resolver)
    }
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_cancel_meters_the_calls_already_paid(pool: PgPool) {
    let a = account(&pool).await;
    // The repair hangs; the handler is cancelled while it is in flight.
    let started = Arc::new(tokio::sync::Notify::new());
    let s = started.clone();
    let ai = Hooked {
        inner: Arc::new(overview_repair(|| Ok(out(valid(SectionKey::Overview))))),
        n: AtomicUsize::new(0),
        before: Box::new(move |n| {
            let s = s.clone();
            Box::pin(async move {
                if n == 1 {
                    s.notify_one();
                    tokio::time::sleep(Duration::from_secs(60)).await;
                }
            })
        }),
        after: nothing(),
    };
    let d = deps(&pool, Some(Arc::new(ai)), now_micros());
    let run = request(&pool, a, ReportTrigger::Onboarding, Plan::Pro).await;
    let c = ctx();
    let cancel = c.cancel.clone();
    let worker = tokio::spawn({
        let d = d.clone();
        async move { GenerateReportHandler(d).handle(job(run), c).await }
    });
    tokio::time::timeout(Duration::from_secs(20), started.notified())
        .await
        .expect("the repair started");
    cancel.cancel();
    let res = worker.await.unwrap();
    assert!(matches!(res, Err(JobError::Release { .. })), "{res:?}");
    assert_eq!(marked_rows(&pool, a).await, 1, "attempt 1 was paid");
    assert_eq!(run_row(&pool, run).await.status, RunStatus::Queued);

    // The next delivery regenerates overview; attempt 1 stays metered once.
    let d2 = deps(
        &pool,
        Some(Arc::new(valid_fake(some_usage()))),
        now_micros(),
    );
    handle(&d2, run).await.unwrap();
    assert_eq!(run_row(&pool, run).await.status, RunStatus::Done);
    assert_eq!(marked_rows(&pool, a).await, 1);
    assert_eq!(usage_rows(&pool, a).await.len(), 6);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_budget_refusing_the_repair_meters_the_paid_first_attempt(pool: PgPool) {
    let a = account(&pool).await;
    // Another call of the scope spends the budget while attempt 1 runs.
    let p = pool.clone();
    let ai = Hooked {
        inner: Arc::new(overview_repair(|| Ok(out(valid(SectionKey::Overview))))),
        n: AtomicUsize::new(0),
        before: nothing(),
        after: Box::new(move |n| {
            let p = p.clone();
            Box::pin(async move {
                if n == 0 {
                    seed_spend(&p, a, 2_000_000).await;
                }
            })
        }),
    };
    let d = deps(&pool, Some(Arc::new(ai)), now_micros());
    let run = request(&pool, a, ReportTrigger::Onboarding, Plan::Pro).await;
    handle(&d, run).await.unwrap();
    let r = run_row(&pool, run).await;
    assert_eq!(r.status, RunStatus::Done);
    assert_eq!(r.ai_disabled_reason.as_deref(), Some("budget"));
    assert_eq!(marked_rows(&pool, a).await, 1);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn exhausted_retries_meter_the_paid_first_attempt(pool: PgPool) {
    let a = account(&pool).await;
    let run = request(&pool, a, ReportTrigger::Onboarding, Plan::Pro).await;
    sqlx::query("UPDATE report_runs SET retry_count = 5 WHERE id = $1")
        .bind(run)
        .execute(&pool)
        .await
        .unwrap();
    let d = deps(
        &pool,
        Some(Arc::new(overview_repair(|| Err(unavailable())))),
        now_micros(),
    );
    handle(&d, run).await.unwrap();
    assert_eq!(run_row(&pool, run).await.status, RunStatus::Failed);
    assert_eq!(marked_rows(&pool, a).await, 1);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn auth_on_the_repair_meters_the_paid_first_attempt(pool: PgPool) {
    let a = account(&pool).await;
    let run = request(&pool, a, ReportTrigger::Onboarding, Plan::Pro).await;
    let fake = overview_repair(|| Err(AiError::Auth { status: 401 }));
    let d = deps(&pool, Some(Arc::new(fake)), now_micros());
    let res = handle(&d, run).await;
    assert!(
        matches!(res, Err(JobError::Unrecoverable { .. })),
        "{res:?}"
    );
    assert_eq!(
        run_row(&pool, run).await.last_error.as_deref(),
        Some("ai_auth")
    );
    assert_eq!(marked_rows(&pool, a).await, 1);
}

/// Entitlements that let another delivery take the run over (stale-reclaim it) when the job
/// asks for the plan (`at_plan`) or for the budget scope.
struct Takeover {
    pool: PgPool,
    run: Uuid,
    at_plan: bool,
}

impl Takeover {
    async fn take(&self) {
        let mut c = self.pool.acquire().await.unwrap();
        runs::claim_run_or_stale(&mut c, self.run, Duration::ZERO)
            .await
            .unwrap()
            .expect("taken over");
    }
}

impl Entitlements for Takeover {
    fn plan<'a>(&'a self, _account_id: Uuid) -> BoxFuture<'a, Plan> {
        Box::pin(async move {
            if self.at_plan {
                self.take().await;
            }
            Plan::Pro
        })
    }
    fn budget_scope<'a>(&'a self, account_id: Uuid) -> BoxFuture<'a, Vec<Uuid>> {
        Box::pin(async move {
            if !self.at_plan {
                self.take().await;
            }
            vec![account_id]
        })
    }
}

async fn report_ready_count(pool: &PgPool, a: Uuid) -> usize {
    event_labels(pool, a)
        .await
        .iter()
        .filter(|l| *l == "report.ready")
        .count()
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_zombie_cannot_finish_a_run_taken_over(pool: PgPool) {
    let a = account(&pool).await;
    let d = deps(
        &pool,
        Some(Arc::new(valid_fake(some_usage()))),
        now_micros(),
    );
    let run = request(&pool, a, ReportTrigger::Onboarding, Plan::Pro).await;
    handle(&d, run).await.unwrap();
    // The run crashed after its last section, before the finish: it is redelivered.
    sqlx::query("UPDATE report_runs SET status = 'queued', finished_at = NULL WHERE id = $1")
        .bind(run)
        .execute(&pool)
        .await
        .unwrap();
    let ready_before = report_ready_count(&pool, a).await;
    let mut z = d.clone();
    z.entitlements = Arc::new(Takeover {
        pool: pool.clone(),
        run,
        at_plan: true,
    });
    let res = handle(&z, run).await.unwrap();
    assert_eq!(res["lost"], json!(true), "{res}");
    assert_eq!(run_row(&pool, run).await.status, RunStatus::Running);
    assert_eq!(
        report_ready_count(&pool, a).await,
        ready_before,
        "the zombie announced nothing"
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_zombie_failing_after_a_takeover_meters_its_paid_calls(pool: PgPool) {
    let a = account(&pool).await;
    let run = request(&pool, a, ReportTrigger::Onboarding, Plan::Pro).await;
    let mut d = deps(
        &pool,
        Some(Arc::new(overview_repair(|| Err(unavailable())))),
        now_micros(),
    );
    d.entitlements = Arc::new(Takeover {
        pool: pool.clone(),
        run,
        at_plan: false,
    });
    let res = handle(&d, run).await.unwrap();
    assert_eq!(res["lost"], json!(true), "{res}");
    let r = run_row(&pool, run).await;
    assert_eq!(
        r.status,
        RunStatus::Running,
        "the new holder's run is untouched"
    );
    assert_eq!(r.retry_count, 0);
    assert_eq!(
        marked_rows(&pool, a).await,
        1,
        "the zombie still meters what it paid"
    );
    assert!(
        !event_labels(&pool, a)
            .await
            .iter()
            .any(|l| l == "report.degraded")
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_dead_job_leaves_a_live_run_alone(pool: PgPool) {
    let a = account(&pool).await;
    let d = deps(&pool, None, now_micros());
    let run = request(&pool, a, ReportTrigger::Onboarding, Plan::Pro).await;
    let mut c = pool.acquire().await.unwrap();
    runs::claim_run(&mut c, run).await.unwrap().unwrap();
    GenerateReportHandler(d.clone())
        .on_dead(&job(run), "boom")
        .await;
    assert_eq!(run_row(&pool, run).await.status, RunStatus::Running);
    assert!(events(&pool, a).await.is_empty());
}

/// Serves the fixture after every caller reached the barrier: both runs pass the early plan
/// check before either creates its report.
struct BarrierInputs(Arc<tokio::sync::Barrier>);

impl CoachingInputProvider for BarrierInputs {
    fn coaching_input<'a>(
        &'a self,
        _account_id: Uuid,
        _tc: TimeClass,
    ) -> BoxFuture<'a, anyhow::Result<Option<Value>>> {
        Box::pin(async move {
            self.0.wait().await;
            Ok(Some(load_fixture("b2-rapid")))
        })
    }
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn two_racing_free_runs_get_one_report(pool: PgPool) {
    let a = account(&pool).await;
    let mut d = deps(
        &pool,
        Some(Arc::new(valid_fake(some_usage()))),
        now_micros(),
    );
    d.entitlements = Arc::new(FixedPlan(Plan::Free));
    d.inputs = Arc::new(BarrierInputs(Arc::new(tokio::sync::Barrier::new(2))));
    let r1 = request(&pool, a, ReportTrigger::Manual, Plan::Free).await;
    let r2 = request(&pool, a, ReportTrigger::Manual, Plan::Free).await;
    let (x, y) = tokio::join!(handle(&d, r1), handle(&d, r2));
    let failed = [x, y]
        .iter()
        .filter(|r| matches!(r, Err(JobError::Unrecoverable { code: Some(c), .. }) if c == "plan_interval"))
        .count();
    assert_eq!(failed, 1);
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM reports WHERE account_id = $1")
        .bind(a)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 1, "one report inside the free interval");
}

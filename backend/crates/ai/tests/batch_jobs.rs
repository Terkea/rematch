//! R3 Task 20: monthly reports and weekly reviews regenerated through the Message Batches API.
//! A scripted batch client stands in for the API (no live calls): it records each submission,
//! reports a status per batch, and answers every submitted item through a test closure.

#[path = "support/generate.rs"]
mod support;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use ai::client::cost::{AI_MONTHLY_BUDGET_MICRO_USD, PricingTable, cost_micro_usd};
use ai::client::fake::{
    FakeAiClient, FakeBatch, FakeBatchItem, FakeCallKind, FakeScript, FakeStructured,
};
use ai::client::model::Models;
use ai::client::port::{
    AiClient, AiError, AiUsage, BatchItem, BatchOutcome, BatchResultItem, BatchStatus,
    ChatTurnRequest, SchemaResolver, StructuredRequest, StructuredResult, ToolHandler, TurnEvent,
    UnavailableReason,
};
use ai::db::reports::{self, NewReport, ReportStatus};
use ai::db::runs::{self, NewRun, ReportMode, ReportTrigger};
use ai::entitlements::{DefaultEntitlements, Entitlements, Plan};
use ai::generate::batch::batch_item_id;
use ai::input::types::{CoachingInput, TimeClass};
use ai::jobs::batch_jobs::{
    AiBatchPollHandler, BATCH_GIVE_UP_AFTER, BATCH_RESULTS_GIVE_UP_AFTER, BATCH_SUBMIT_TIMEOUT,
    BatchDeps, NullWeeklyProvider, ReportBatchScheduleHandler, WeeklyBatchScheduleHandler,
    WeeklyInputProvider, reviewed_week,
};
use ai::jobs::budget::BudgetConfig;
use ai::jobs::kinds::{KIND_AI_BATCH_POLL, KIND_GENERATE_REPORT, KIND_WEEKLY_REVIEW};
use ai::jobs::providers::{CoachingInputProvider, NullInputProvider};
use ai::jobs::reconcile::BATCH_SUBMIT_GRACE;
use ai::output::codes::{SECTION_KEYS, SectionKey};
use ai::testing::{load_fixture, make_input};
use futures::future::BoxFuture;
use futures::stream::{self, BoxStream};
use futures::{StreamExt, TryStreamExt};
use queue::queue::Queue;
use queue::runner::{Handler, JobContext};
use queue::types::Job;
use serde_json::{Value, json};
use sqlx::PgPool;
use support::{LEAKING_TEXT, envelope, section_of, valid, w1_item};
use time::{Date, Duration as TDuration, OffsetDateTime};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

const BATCH_USAGE: AiUsage = AiUsage {
    input_tokens: 1200,
    output_tokens: 300,
    cache_creation_tokens: 0,
    cache_read_tokens: 4000,
};

// ---------------------------------------------------------------------------------------------
// The scripted batch client.

/// How a submitted item is answered.
type Answer = Box<dyn Fn(&BatchItem) -> FakeBatchItem + Send + Sync>;

#[derive(Default)]
struct BatchState {
    /// Submissions in order: (batch id, items).
    submitted: Vec<(String, Vec<BatchItem>)>,
    /// Per batch id; absent means `default_status`.
    status: HashMap<String, BatchStatus>,
    /// Results served for a batch id: the items of these batches (default: its own).
    results_from: HashMap<String, Vec<String>>,
    /// The next results stream fails after this many items (once).
    fail_after: Option<usize>,
    /// Every results stream yields an undecodable line (a `Response` error, as the real client
    /// does for a bad or oversized line) in place of the item at this index, and goes on.
    poison_at: Option<usize>,
    /// Every results read fails outright (an endpoint that keeps failing).
    results_fail: bool,
    results_calls: usize,
}

struct TestAi {
    /// Answers the synchronous repairs.
    sync: FakeAiClient,
    enabled: bool,
    default_status: Mutex<BatchStatus>,
    answer: Answer,
    state: Mutex<BatchState>,
}

impl TestAi {
    fn new(answer: Answer) -> Self {
        Self::with_sync(answer, |req| {
            Ok(FakeStructured {
                parsed: Some(valid(section_of(&b2(), req))),
                usage: BATCH_USAGE,
                ..FakeStructured::default()
            })
        })
    }

    fn with_sync(
        answer: Answer,
        sync: impl Fn(&StructuredRequest) -> Result<FakeStructured, AiError> + Send + Sync + 'static,
    ) -> Self {
        TestAi {
            sync: FakeAiClient::new(FakeScript {
                structured: Some(Box::new(move |req, _| sync(req))),
                ..FakeScript::default()
            }),
            enabled: true,
            default_status: Mutex::new(BatchStatus::Ended),
            answer,
            state: Mutex::default(),
        }
    }

    fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }

    fn set_default_status(&self, s: BatchStatus) {
        *self.default_status.lock().unwrap() = s;
    }

    fn set_status(&self, id: &str, s: BatchStatus) {
        self.state.lock().unwrap().status.insert(id.to_owned(), s);
    }

    fn submissions(&self) -> Vec<(String, Vec<BatchItem>)> {
        self.state.lock().unwrap().submitted.clone()
    }

    fn sync_calls(&self) -> usize {
        self.sync
            .calls()
            .iter()
            .filter(|c| c.kind == FakeCallKind::Structured)
            .count()
    }

    fn results_calls(&self) -> usize {
        self.state.lock().unwrap().results_calls
    }
}

impl AiClient for TestAi {
    fn structured<'a>(
        &'a self,
        req: &'a StructuredRequest,
    ) -> BoxFuture<'a, Result<StructuredResult, AiError>> {
        self.sync.structured(req)
    }

    fn stream_turn<'a>(
        &'a self,
        _req: &'a ChatTurnRequest,
        _tools: Option<Arc<dyn ToolHandler>>,
    ) -> BoxStream<'a, Result<TurnEvent, AiError>> {
        panic!("no chat turn in batch tests")
    }

    fn batches_enabled(&self) -> bool {
        self.enabled
    }

    fn batch_submit<'a>(
        &'a self,
        items: &'a [BatchItem],
    ) -> BoxFuture<'a, Result<String, AiError>> {
        Box::pin(async move {
            assert!(self.enabled, "submit with batches disabled");
            let mut s = self.state.lock().unwrap();
            let id = format!("msgbatch_{}", s.submitted.len() + 1);
            s.submitted.push((id.clone(), items.to_vec()));
            Ok(id)
        })
    }

    fn batch_status<'a>(&'a self, id: &'a str) -> BoxFuture<'a, Result<BatchStatus, AiError>> {
        Box::pin(async move {
            let s = self.state.lock().unwrap();
            Ok(s.status
                .get(id)
                .copied()
                .unwrap_or(*self.default_status.lock().unwrap()))
        })
    }

    fn batch_results<'a>(
        &'a self,
        id: &'a str,
        resolver: &'a dyn SchemaResolver,
    ) -> BoxStream<'a, Result<BatchResultItem, AiError>> {
        let (lines, fail_after, poison_at) = {
            let mut s = self.state.lock().unwrap();
            s.results_calls += 1;
            if s.results_fail {
                return stream::once(async {
                    Err(AiError::Request {
                        status: 404,
                        api_error_type: Some("not_found_error".into()),
                    })
                })
                .boxed();
            }
            let from = s
                .results_from
                .get(id)
                .cloned()
                .unwrap_or_else(|| vec![id.to_owned()]);
            let lines: Vec<FakeBatchItem> = from
                .iter()
                .flat_map(|b| {
                    s.submitted
                        .iter()
                        .filter(move |(sid, _)| sid == b)
                        .flat_map(|(_, items)| items.iter().map(|i| (self.answer)(i)))
                })
                .collect();
            (lines, s.fail_after.take(), s.poison_at)
        };
        // A plain fake serves the scripted lines, so they are parsed exactly as the real
        // client parses them (with the resolver's schema).
        let mut script = FakeScript::default();
        script.batches.insert(
            "b".to_owned(),
            FakeBatch {
                status: BatchStatus::Ended,
                results: lines,
            },
        );
        let fake = FakeAiClient::new(script);
        stream::once(async move {
            let items: Vec<_> = fake.batch_results("b", resolver).collect().await;
            stream::iter(items)
        })
        .flatten()
        .enumerate()
        .map(move |(i, r)| match (fail_after, poison_at) {
            // A cut connection, as the real client reports one (the stream then ends).
            (Some(n), _) if i >= n => Err(AiError::Unavailable {
                reason: UnavailableReason::Connection,
                retry_after: None,
            }),
            (_, Some(p)) if i == p => Err(AiError::Response(
                "batch result line could not be decoded".into(),
            )),
            _ => r,
        })
        .take_while({
            let mut failed = false;
            move |r| {
                let go = !failed;
                // Only a transport error ends the stream; a bad line is skipped.
                failed |= matches!(r, Err(AiError::Unavailable { .. }));
                futures::future::ready(go)
            }
        })
        .boxed()
    }
}

// ---------------------------------------------------------------------------------------------
// Answers.

fn b2() -> CoachingInput {
    ai::testing::input("b2-rapid")
}

fn batch_line(item: &BatchItem, text: Value) -> FakeBatchItem {
    FakeBatchItem {
        custom_id: item.custom_id.clone(),
        outcome: BatchOutcome::Succeeded,
        text: Some(text.to_string()),
        usage: BATCH_USAGE,
        model: Some(item.req.model.as_str().to_owned()),
        ..FakeBatchItem::default()
    }
}

/// Every report section valid; every weekly review valid.
fn all_valid(item: &BatchItem) -> FakeBatchItem {
    if item.req.schema.id == "weekly-review" {
        return batch_line(item, review());
    }
    batch_line(item, valid(section_of(&b2(), &item.req)))
}

fn expired(item: &BatchItem) -> FakeBatchItem {
    FakeBatchItem {
        custom_id: item.custom_id.clone(),
        outcome: BatchOutcome::Expired,
        ..FakeBatchItem::default()
    }
}

// ---------------------------------------------------------------------------------------------
// Fixtures.

/// Pro for exactly these accounts, free for everyone else.
struct ProFor(Vec<Uuid>);

impl Entitlements for ProFor {
    fn plan<'a>(&'a self, account_id: Uuid) -> BoxFuture<'a, Plan> {
        let pro = self.0.contains(&account_id);
        Box::pin(async move { if pro { Plan::Pro } else { Plan::Free } })
    }

    fn budget_scope<'a>(&'a self, account_id: Uuid) -> BoxFuture<'a, Vec<Uuid>> {
        Box::pin(async move { vec![account_id] })
    }
}

/// The b2-rapid fixture for any account (rapid only).
struct FixtureInputs;

impl CoachingInputProvider for FixtureInputs {
    fn coaching_input<'a>(
        &'a self,
        account_id: Uuid,
        tc: TimeClass,
    ) -> BoxFuture<'a, anyhow::Result<Option<Value>>> {
        Box::pin(async move {
            Ok((tc == TimeClass::Rapid).then(|| make_input(json!({ "accountId": account_id }))))
        })
    }
}

/// These accounts are due a rapid weekly review.
struct WeeklyFor(Vec<Uuid>);

impl WeeklyInputProvider for WeeklyFor {
    fn due<'a>(
        &'a self,
        week_start: Date,
    ) -> BoxStream<'a, anyhow::Result<(Uuid, TimeClass, Value)>> {
        stream::iter(
            self.0
                .iter()
                .map(move |a| Ok((*a, TimeClass::Rapid, weekly_json(week_start)))),
        )
        .boxed()
    }
}

fn monday(now: OffsetDateTime) -> Date {
    let d = now.date();
    d - TDuration::days(i64::from(d.weekday().number_days_from_monday()))
}

/// A week for the b2-rapid player (as in the Task 15 tests).
fn weekly_json(week_start: Date) -> Value {
    let b2 = load_fixture("b2-rapid");
    let iso = |d: Date| format!("{:04}-{:02}-{:02}", d.year(), u8::from(d.month()), d.day());
    json!({
        "inputVersion": "weekly-input-v1",
        "timeClass": b2["timeClass"],
        "band": b2["band"],
        "weekStart": iso(week_start),
        "weekEnd": iso(week_start + TDuration::days(6)),
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

fn review() -> Value {
    let item = |t: &str| json!({ "text": t, "citations": [] });
    json!({
        "headline": item("A steady week of practice."),
        "process": item("You put in {{num:adherence.minutesDone|int}} minutes of practice this week."),
        "outcomes": [{
            "text": "Whether the work on hanging pieces shows in your results is unclear yet.",
            "citations": [], "weaknessId": "W1"
        }],
        "exemplars": [{
            "text": "In this game {{move:li-w1-0:5}} left a piece loose.",
            "citations": [{ "gameId": "li-w1-0", "ply": 5 }],
        }],
        "nextWeek": item("Next week, look for loose pieces before you move."),
    })
}

/// An account with a rapid report created `age_days` ago.
async fn account_with_report(pool: &PgPool, name: &str, age_days: i64) -> Uuid {
    let a = testkit::insert_account(pool, "chesscom", name).await;
    let mut conn = pool.acquire().await.unwrap();
    let run = runs::insert_run(
        &mut conn,
        &NewRun {
            account_id: a,
            time_class: "rapid".into(),
            trigger: ReportTrigger::Onboarding,
            plan: Plan::Pro,
            mode: ReportMode::Sync,
            prompt_version: "report@1".into(),
            is_first_report: true,
            ai_disabled_reason: None,
        },
    )
    .await
    .unwrap();
    let r = reports::create_report_with_next_version(
        &mut conn,
        &NewReport {
            account_id: a,
            time_class: "rapid".into(),
            report_run_id: run,
            schema_version: "report-payload-v1".into(),
            computed: json!({}),
            status: ReportStatus::Complete,
        },
    )
    .await
    .unwrap();
    runs::set_done(&mut conn, run, Some(r.id)).await.unwrap();
    sqlx::query("UPDATE reports SET created_at = now() - make_interval(days => $2) WHERE id = $1")
        .bind(r.id)
        .bind(i32::try_from(age_days).unwrap())
        .execute(&mut *conn)
        .await
        .unwrap();
    a
}

fn deps(
    pool: &PgPool,
    ai: Option<Arc<TestAi>>,
    entitlements: Arc<dyn Entitlements>,
    weekly: Arc<dyn WeeklyInputProvider>,
) -> BatchDeps {
    BatchDeps {
        pool: pool.clone(),
        queue: Queue::new(pool.clone()),
        ai: ai.map(|a| a as Arc<dyn AiClient>),
        inputs: Arc::new(FixtureInputs),
        weekly,
        entitlements,
        budgets: BudgetConfig {
            per_plan: AI_MONTHLY_BUDGET_MICRO_USD,
            global_monthly: None,
            enforce: true,
        },
        pricing: Arc::new(PricingTable::seed()),
        now: Arc::new(OffsetDateTime::now_utc),
    }
}

fn pro(accounts: &[Uuid]) -> Arc<dyn Entitlements> {
    Arc::new(ProFor(accounts.to_vec()))
}

fn job(kind: &str) -> Job {
    let now = OffsetDateTime::now_utc();
    Job {
        id: 1,
        kind: kind.into(),
        payload: json!({}),
        dedupe_key: Some(kind.into()),
        user_key: None,
        priority: 100,
        attempts: 1,
        max_attempts: 3,
        lease_token: Uuid::new_v4(),
        run_at: now,
        created_at: now,
        started_at: now,
    }
}

async fn run(h: &impl Handler) -> Value {
    h.handle(
        job(h.kind()),
        JobContext {
            cancel: CancellationToken::new(),
        },
    )
    .await
    .unwrap_or_else(|e| panic!("{} failed: {e:?}", h.kind()))
}

async fn schedule(d: &BatchDeps) -> Value {
    run(&ReportBatchScheduleHandler(d.clone())).await
}

async fn poll(d: &BatchDeps) -> Value {
    run(&AiBatchPollHandler(d.clone())).await
}

async fn count(pool: &PgPool, sql: &str) -> i64 {
    sqlx::query_scalar(sqlx::AssertSqlSafe(sql.to_owned()))
        .fetch_one(pool)
        .await
        .unwrap()
}

/// `(account, status, mode, trigger)` of the scheduled runs, by account.
async fn scheduled_runs(pool: &PgPool) -> Vec<(Uuid, Uuid, String, String)> {
    sqlx::query_as(
        "SELECT id, account_id, status::text, mode::text FROM report_runs \
         WHERE trigger = 'scheduled' ORDER BY account_id",
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

async fn events(pool: &PgPool, ty: &str) -> Vec<(Uuid, Value)> {
    sqlx::query_as("SELECT account_id, payload FROM user_events WHERE type = $1 ORDER BY id")
        .bind(ty)
        .fetch_all(pool)
        .await
        .unwrap()
}

async fn jobs_of(pool: &PgPool, kind: &str) -> Vec<String> {
    sqlx::query_scalar("SELECT dedupe_key FROM jobs WHERE kind = $1 ORDER BY dedupe_key")
        .bind(kind)
        .fetch_all(pool)
        .await
        .unwrap()
}

/// `(feature, model, batch, cost)` of every usage row.
async fn usage(pool: &PgPool) -> Vec<(String, String, bool, i64)> {
    sqlx::query_as("SELECT feature::text, model, batch, cost_micro_usd FROM ai_usage ORDER BY id")
        .fetch_all(pool)
        .await
        .unwrap()
}

/// Everything a re-poll must not add to.
async fn footprint(pool: &PgPool) -> [i64; 5] {
    [
        count(pool, "SELECT count(*) FROM report_sections").await,
        count(pool, "SELECT count(*) FROM user_events").await,
        count(pool, "SELECT count(*) FROM ai_usage").await,
        count(pool, "SELECT count(*) FROM jobs").await,
        count(
            pool,
            "SELECT count(*) FROM weekly_reviews WHERE status <> 'pending'",
        )
        .await,
    ]
}

// ---------------------------------------------------------------------------------------------
// Scheduling.

#[test]
fn the_submit_timeout_stays_below_the_reconciler_grace() {
    assert!(BATCH_SUBMIT_TIMEOUT < BATCH_SUBMIT_GRACE);
    assert!(BATCH_GIVE_UP_AFTER >= std::time::Duration::from_secs(24 * 3600));
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn default_entitlements_schedule_nothing(pool: PgPool) {
    let _a = account_with_report(&pool, "old", 31).await;
    let ai = Arc::new(TestAi::new(Box::new(all_valid)));
    let d = deps(
        &pool,
        Some(ai.clone()),
        Arc::new(DefaultEntitlements),
        Arc::new(NullWeeklyProvider),
    );
    schedule(&d).await;
    // No AI at all: nothing either.
    let none = deps(&pool, None, pro(&[_a]), Arc::new(NullWeeklyProvider));
    schedule(&none).await;
    assert!(ai.submissions().is_empty());
    assert!(scheduled_runs(&pool).await.is_empty());
    assert_eq!(count(&pool, "SELECT count(*) FROM ai_batches").await, 0);
    assert_eq!(count(&pool, "SELECT count(*) FROM reports").await, 1);
}

/// M5b Task B ruling: without `ANTHROPIC_API_KEY` (batches off) but with a sidecar credential (an
/// AI client exists), both schedules enqueue the normal sync jobs instead of a batch.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn without_batches_the_schedules_enqueue_sync_jobs(pool: PgPool) {
    let a = account_with_report(&pool, "old", 31).await;
    let ai = Arc::new(TestAi::new(Box::new(all_valid)).disabled());
    let d = deps(
        &pool,
        Some(ai.clone()),
        pro(&[a]),
        Arc::new(WeeklyFor(vec![a])),
    );
    let out = schedule(&d).await;
    assert_eq!(out["path"], "sync", "{out}");
    let runs = scheduled_runs(&pool).await;
    assert_eq!(runs.len(), 1);
    let (run_id, account, status, mode) = runs[0].clone();
    assert_eq!(
        (account, status.as_str(), mode.as_str()),
        (a, "queued", "sync")
    );
    assert_eq!(
        jobs_of(&pool, KIND_GENERATE_REPORT).await,
        vec![format!("report:{run_id}")]
    );
    let payload: Value = sqlx::query_scalar("SELECT payload FROM jobs WHERE kind = $1")
        .bind(KIND_GENERATE_REPORT)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(payload, json!({ "reportRunId": run_id }));

    let out = run(&WeeklyBatchScheduleHandler(d.clone())).await;
    assert_eq!(out["path"], "sync", "{out}");
    let rows: Vec<(Uuid, String, String)> =
        sqlx::query_as("SELECT id, status::text, mode::text FROM weekly_reviews")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        (rows[0].1.as_str(), rows[0].2.as_str()),
        ("pending", "sync")
    );
    assert_eq!(
        jobs_of(&pool, KIND_WEEKLY_REVIEW).await,
        vec![format!("weekly:{}", rows[0].0)]
    );

    // No batch, no model call at scheduling time.
    assert!(ai.submissions().is_empty());
    assert_eq!(count(&pool, "SELECT count(*) FROM ai_batches").await, 0);
    assert_eq!(ai.sync_calls(), 0);

    // Running both again adds nothing: the run is unfinished and the week has its review.
    schedule(&d).await;
    run(&WeeklyBatchScheduleHandler(d.clone())).await;
    assert_eq!(scheduled_runs(&pool).await.len(), 1);
    assert_eq!(count(&pool, "SELECT count(*) FROM weekly_reviews").await, 1);
    assert_eq!(jobs_of(&pool, KIND_GENERATE_REPORT).await.len(), 1);
    assert_eq!(jobs_of(&pool, KIND_WEEKLY_REVIEW).await.len(), 1);
}

/// With the key set, the same due items go to one batch each and no sync job is enqueued.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn with_batches_the_schedules_submit_batches(pool: PgPool) {
    let a = account_with_report(&pool, "old", 31).await;
    let ai = Arc::new(TestAi::new(Box::new(all_valid)));
    let d = deps(
        &pool,
        Some(ai.clone()),
        pro(&[a]),
        Arc::new(WeeklyFor(vec![a])),
    );
    assert_eq!(schedule(&d).await["path"], "batch");
    assert_eq!(
        run(&WeeklyBatchScheduleHandler(d.clone())).await["path"],
        "batch"
    );
    assert_eq!(ai.submissions().len(), 2);
    assert_eq!(scheduled_runs(&pool).await[0].3, "batch");
    let mode: String = sqlx::query_scalar("SELECT mode::text FROM weekly_reviews")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(mode, "batch");
    assert!(jobs_of(&pool, KIND_GENERATE_REPORT).await.is_empty());
    assert!(jobs_of(&pool, KIND_WEEKLY_REVIEW).await.is_empty());
}

/// Without any AI client nothing is scheduled at all.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn no_ai_schedules_nothing(pool: PgPool) {
    let a = account_with_report(&pool, "old", 31).await;
    let d = deps(&pool, None, pro(&[a]), Arc::new(WeeklyFor(vec![a])));
    schedule(&d).await;
    run(&WeeklyBatchScheduleHandler(d.clone())).await;
    assert!(scheduled_runs(&pool).await.is_empty());
    assert_eq!(count(&pool, "SELECT count(*) FROM weekly_reviews").await, 0);
    assert!(jobs_of(&pool, KIND_GENERATE_REPORT).await.is_empty());
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn pro_accounts_schedule_one_batch_of_ten(pool: PgPool) {
    let a = account_with_report(&pool, "a", 31).await;
    let b = account_with_report(&pool, "b", 45).await;
    // Too recent, and free: neither is scheduled.
    let fresh = account_with_report(&pool, "fresh", 10).await;
    let free = account_with_report(&pool, "free", 40).await;
    let ai = Arc::new(TestAi::new(Box::new(all_valid)));
    let d = deps(
        &pool,
        Some(ai.clone()),
        pro(&[a, b, fresh]),
        Arc::new(NullWeeklyProvider),
    );
    schedule(&d).await;

    let subs = ai.submissions();
    assert_eq!(subs.len(), 1, "one batch");
    let (batch_id, items) = &subs[0];
    assert_eq!(items.len(), 10);

    let runs = scheduled_runs(&pool).await;
    let mut accounts: Vec<Uuid> = runs.iter().map(|r| r.1).collect();
    accounts.sort();
    let mut want = vec![a, b];
    want.sort();
    assert_eq!(accounts, want);
    for (_, _, status, mode) in &runs {
        assert_eq!(
            (status.as_str(), mode.as_str()),
            ("awaiting_batch", "batch")
        );
    }
    let _ = free;

    let (status, n): (String, i32) =
        sqlx::query_as("SELECT status::text, request_count FROM ai_batches WHERE id = $1")
            .bind(batch_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!((status.as_str(), n), ("submitted", 10));

    // Every submitted request is exactly the sync request of the section its item row names,
    // built from that run's own snapshot.
    for item in items {
        let row: (Uuid, Uuid, String, Value, Option<Uuid>) = sqlx::query_as(
            "SELECT i.id, r.account_id, i.section, r.input, r.report_id FROM ai_batch_items i \
             JOIN report_runs r ON r.id = i.report_run_id WHERE i.custom_id = $1 AND i.batch_id = $2",
        )
        .bind(&item.custom_id)
        .bind(batch_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(batch_item_id(&item.custom_id), Some(row.0));
        let input: CoachingInput = serde_json::from_value(row.3).unwrap();
        assert_eq!(input.account_id, row.1);
        assert!(row.4.is_some(), "the computed report exists");
        let section = SectionKey::try_from(row.2).unwrap();
        assert_eq!(section_of(&input, &item.req), section);
        let sync = ai::generate::request::build_section_request(&input, section, false);
        assert_eq!(
            serde_json::to_value(ai::client::wire::MessagesRequest::structured(&item.req)).unwrap(),
            serde_json::to_value(ai::client::wire::MessagesRequest::structured(&sync)).unwrap()
        );
    }
    for run in &runs {
        let sections: Vec<String> = sqlx::query_scalar(
            "SELECT section FROM ai_batch_items WHERE report_run_id = $1 ORDER BY section",
        )
        .bind(run.0)
        .fetch_all(&pool)
        .await
        .unwrap();
        let mut want: Vec<String> = SECTION_KEYS.iter().map(|k| k.as_str().to_owned()).collect();
        want.sort();
        assert_eq!(sections, want);
    }

    // The new computed report is fresh: a second schedule submits nothing.
    schedule(&d).await;
    assert_eq!(ai.submissions().len(), 1);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn over_budget_account_is_not_batched(pool: PgPool) {
    let a = account_with_report(&pool, "a", 31).await;
    let spent = account_with_report(&pool, "spent", 31).await;
    sqlx::query(
        "INSERT INTO ai_usage (account_id, feature, model, input_tokens, output_tokens, \
         cache_creation_tokens, cache_read_tokens, cost_micro_usd) \
         VALUES ($1, 'report', 'claude-sonnet-5-5', 0, 0, 0, 0, $2)",
    )
    .bind(spent)
    .bind(i64::try_from(AI_MONTHLY_BUDGET_MICRO_USD.pro).unwrap())
    .execute(&pool)
    .await
    .unwrap();
    let ai = Arc::new(TestAi::new(Box::new(all_valid)));
    let d = deps(
        &pool,
        Some(ai.clone()),
        pro(&[a, spent]),
        Arc::new(NullWeeklyProvider),
    );
    schedule(&d).await;
    let subs = ai.submissions();
    assert_eq!(subs.len(), 1);
    assert_eq!(subs[0].1.len(), 5);
    let runs = scheduled_runs(&pool).await;
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].1, a);
}

// ---------------------------------------------------------------------------------------------
// Polling.

async fn two_scheduled(pool: &PgPool, ai: Arc<TestAi>) -> (BatchDeps, Uuid, Uuid) {
    let a = account_with_report(pool, "a", 31).await;
    let b = account_with_report(pool, "b", 31).await;
    let d = deps(pool, Some(ai), pro(&[a, b]), Arc::new(NullWeeklyProvider));
    schedule(&d).await;
    (d, a, b)
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn poll_in_progress_writes_no_sections(pool: PgPool) {
    let ai = Arc::new(TestAi::new(Box::new(all_valid)));
    ai.set_default_status(BatchStatus::InProgress);
    let (d, _, _) = two_scheduled(&pool, ai.clone()).await;
    poll(&d).await;
    assert_eq!(
        count(&pool, "SELECT count(*) FROM report_sections").await,
        0
    );
    assert_eq!(ai.results_calls(), 0);
    let (status, polled): (String, Option<OffsetDateTime>) =
        sqlx::query_as("SELECT status::text, last_polled_at FROM ai_batches")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(status, "submitted");
    assert!(polled.is_some());
    for r in scheduled_runs(&pool).await {
        assert_eq!(r.2, "awaiting_batch");
    }
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn poll_ended_writes_sections_and_marks_runs_done(pool: PgPool) {
    let ai = Arc::new(TestAi::new(Box::new(all_valid)));
    let (d, a, b) = two_scheduled(&pool, ai.clone()).await;
    poll(&d).await;

    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM report_sections WHERE status = 'ready'"
        )
        .await,
        10
    );
    for r in scheduled_runs(&pool).await {
        assert_eq!(r.2, "done");
    }
    let ready = events(&pool, "report.ready").await;
    let mut who: Vec<Uuid> = ready.iter().map(|e| e.0).collect();
    who.sort();
    let mut want = vec![a, b];
    want.sort();
    assert_eq!(who, want);
    for (_, p) in &ready {
        assert_eq!(p["status"], "complete");
    }
    assert_eq!(ai.sync_calls(), 0);

    // Ten rows, each a batch call at half the sync price.
    let pricing = PricingTable::seed();
    let model = Models::REPORT.as_str();
    let batch_cost = cost_micro_usd(&pricing, model, &BATCH_USAGE, true).unwrap();
    let sync_cost = cost_micro_usd(&pricing, model, &BATCH_USAGE, false).unwrap();
    assert!(batch_cost * 2 >= sync_cost - 1 && batch_cost * 2 <= sync_cost + 1);
    let rows = usage(&pool).await;
    assert_eq!(rows.len(), 10);
    for (feature, m, batch, cost) in rows {
        assert_eq!(feature, "report");
        assert_eq!(m, model);
        assert!(batch);
        assert_eq!(cost, i64::try_from(batch_cost).unwrap());
    }
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM ai_batches WHERE status = 'processed'"
        )
        .await,
        1
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM ai_batch_items WHERE processed_at IS NULL"
        )
        .await,
        0
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn one_invalid_result_makes_exactly_one_sync_call(pool: PgPool) {
    let bad: Arc<Mutex<Option<String>>> = Arc::default();
    let pick = bad.clone();
    let ai = Arc::new(TestAi::new(Box::new(move |item| {
        let section = section_of(&b2(), &item.req);
        let mut first = pick.lock().unwrap();
        if section == SectionKey::Weaknesses && first.as_ref().is_none_or(|c| *c == item.custom_id)
        {
            *first = Some(item.custom_id.clone());
            return batch_line(
                item,
                envelope(
                    SectionKey::Weaknesses,
                    json!({"items": [w1_item(LEAKING_TEXT)]}),
                ),
            );
        }
        all_valid(item)
    })));
    let (d, _, _) = two_scheduled(&pool, ai.clone()).await;
    poll(&d).await;
    assert_eq!(ai.sync_calls(), 1);
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM report_sections WHERE status = 'ready'"
        )
        .await,
        10
    );
    let rows = usage(&pool).await;
    assert_eq!(rows.len(), 11);
    assert_eq!(rows.iter().filter(|r| r.2).count(), 10, "ten batch rows");
    assert_eq!(rows.iter().filter(|r| !r.2).count(), 1, "one sync repair");
    for r in scheduled_runs(&pool).await {
        assert_eq!(r.2, "done");
    }
}

/// T20 parked item (T26 hardening): a sync repair is paid as soon as it returns. When the item's
/// write then fails, the next poll repairs again, so both repairs are metered (each once).
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_paid_repair_is_metered_when_its_item_write_fails(pool: PgPool) {
    let bad: Arc<Mutex<Option<String>>> = Arc::default();
    let pick = bad.clone();
    let ai = Arc::new(TestAi::new(Box::new(move |item| {
        let section = section_of(&b2(), &item.req);
        let mut first = pick.lock().unwrap();
        if section == SectionKey::Weaknesses && first.as_ref().is_none_or(|c| *c == item.custom_id)
        {
            *first = Some(item.custom_id.clone());
            return batch_line(
                item,
                envelope(
                    SectionKey::Weaknesses,
                    json!({"items": [w1_item(LEAKING_TEXT)]}),
                ),
            );
        }
        all_valid(item)
    })));
    let (d, _, _) = two_scheduled(&pool, ai.clone()).await;
    // The repaired section's write fails once (after the paid repair returned).
    sqlx::raw_sql(
        "CREATE FUNCTION t26_fail_weaknesses() RETURNS trigger LANGUAGE plpgsql AS $$ \
         BEGIN IF NEW.section = 'weaknesses' AND NEW.status = 'ready' THEN \
         RAISE EXCEPTION 't26 write failure'; END IF; RETURN NEW; END $$; \
         CREATE TRIGGER t26_fail BEFORE INSERT ON report_sections FOR EACH ROW \
         EXECUTE FUNCTION t26_fail_weaknesses();",
    )
    .execute(&pool)
    .await
    .unwrap();
    let first = AiBatchPollHandler(d.clone())
        .handle(
            job(KIND_AI_BATCH_POLL),
            JobContext {
                cancel: CancellationToken::new(),
            },
        )
        .await;
    assert!(first.is_err(), "the failed write surfaces: {first:?}");
    assert_eq!(ai.sync_calls(), 1);
    let repairs = |rows: &[(String, String, bool, i64)]| rows.iter().filter(|r| !r.2).count();
    assert_eq!(
        repairs(&usage(&pool).await),
        1,
        "the paid repair is metered"
    );
    sqlx::raw_sql("DROP TRIGGER t26_fail ON report_sections")
        .execute(&pool)
        .await
        .unwrap();
    poll(&d).await;
    assert_eq!(ai.sync_calls(), 2);
    let rows = usage(&pool).await;
    assert_eq!(repairs(&rows), 2, "each paid repair once");
    assert_eq!(
        rows.iter().filter(|r| r.2).count(),
        10,
        "each batch result once"
    );
    for r in scheduled_runs(&pool).await {
        assert_eq!(r.2, "done");
    }
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn expired_result_enqueues_generate_report(pool: PgPool) {
    let gone: Arc<Mutex<Option<String>>> = Arc::default();
    let pick = gone.clone();
    let ai = Arc::new(TestAi::new(Box::new(move |item| {
        let mut first = pick.lock().unwrap();
        if first.as_ref().is_none_or(|c| *c == item.custom_id) {
            *first = Some(item.custom_id.clone());
            return expired(item);
        }
        all_valid(item)
    })));
    let (d, _, _) = two_scheduled(&pool, ai.clone()).await;
    poll(&d).await;

    let gone = gone.lock().unwrap().clone().unwrap();
    let run: Uuid =
        sqlx::query_scalar("SELECT report_run_id FROM ai_batch_items WHERE custom_id = $1")
            .bind(&gone)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        ai.sync_calls(),
        0,
        "the sync job does the work, not the poller"
    );
    assert_eq!(
        jobs_of(&pool, KIND_GENERATE_REPORT).await,
        vec![format!("report:{run}")]
    );
    for r in scheduled_runs(&pool).await {
        let want = if r.0 == run { "queued" } else { "done" };
        assert_eq!(r.2, want);
    }
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM report_sections WHERE status = 'ready'"
        )
        .await,
        9
    );
    // The expired item was not billed: nine usage rows.
    assert_eq!(usage(&pool).await.len(), 9);
    assert_eq!(events(&pool, "report.ready").await.len(), 1);
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM ai_batches WHERE status = 'processed'"
        )
        .await,
        1
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn polling_a_processed_batch_again_adds_nothing(pool: PgPool) {
    let ai = Arc::new(TestAi::new(Box::new(all_valid)));
    let (d, _, _) = two_scheduled(&pool, ai.clone()).await;
    poll(&d).await;
    let before = footprint(&pool).await;
    let calls = ai.results_calls();
    poll(&d).await;
    poll(&d).await;
    assert_eq!(footprint(&pool).await, before);
    assert_eq!(ai.results_calls(), calls);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn stream_error_midway_resumes_next_poll_without_duplicates(pool: PgPool) {
    let ai = Arc::new(TestAi::new(Box::new(all_valid)));
    let (d, _, _) = two_scheduled(&pool, ai.clone()).await;
    ai.state.lock().unwrap().fail_after = Some(4);
    poll(&d).await;
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM ai_batch_items WHERE processed_at IS NOT NULL"
        )
        .await,
        4
    );
    assert_eq!(usage(&pool).await.len(), 4);
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM ai_batches WHERE status = 'ended'"
        )
        .await,
        1
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM report_runs WHERE status = 'done' AND trigger = 'scheduled'"
        )
        .await,
        0
    );

    poll(&d).await;
    assert_eq!(usage(&pool).await.len(), 10);
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM report_sections WHERE status = 'ready'"
        )
        .await,
        10
    );
    // One event per section label (areas announces one per area), never twice.
    let section_events = events(&pool, "report.section.ready").await;
    let mut seen = std::collections::HashSet::new();
    for (account, p) in &section_events {
        assert!(
            seen.insert((*account, p["reportId"].clone(), p["section"].clone())),
            "duplicate event {p}"
        );
    }
    assert_eq!(events(&pool, "report.ready").await.len(), 2);
    for r in scheduled_runs(&pool).await {
        assert_eq!(r.2, "done");
    }
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_result_for_an_item_outside_the_batch_is_never_applied(pool: PgPool) {
    let ai = Arc::new(TestAi::new(Box::new(all_valid)));
    let a = account_with_report(&pool, "a", 31).await;
    let b = account_with_report(&pool, "b", 31).await;
    // A is batched first, alone; then B.
    let da = deps(
        &pool,
        Some(ai.clone()),
        pro(&[a]),
        Arc::new(NullWeeklyProvider),
    );
    schedule(&da).await;
    let d = deps(
        &pool,
        Some(ai.clone()),
        pro(&[a, b]),
        Arc::new(NullWeeklyProvider),
    );
    schedule(&d).await;
    let subs = ai.submissions();
    assert_eq!(subs.len(), 2);
    let (first, second) = (subs[0].0.clone(), subs[1].0.clone());
    // A's batch is still running; B's batch serves A's results too (a misrouted stream).
    ai.set_status(&first, BatchStatus::InProgress);
    ai.state
        .lock()
        .unwrap()
        .results_from
        .insert(second.clone(), vec![first.clone(), second.clone()]);
    poll(&d).await;

    let ready_of = |acc: Uuid| {
        let pool = pool.clone();
        async move {
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM report_sections s JOIN reports r ON r.id = s.report_id \
                 WHERE r.account_id = $1",
            )
            .bind(acc)
            .fetch_one(&pool)
            .await
            .unwrap()
        }
    };
    assert_eq!(
        ready_of(a).await,
        0,
        "A's results are not applied from B's batch"
    );
    assert_eq!(ready_of(b).await, 5);
    let pending: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM ai_batch_items WHERE batch_id = $1 AND processed_at IS NULL",
    )
    .bind(&first)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(pending, 5);
    let a_usage: i64 = sqlx::query_scalar("SELECT count(*) FROM ai_usage WHERE account_id = $1")
        .bind(a)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(a_usage, 0);
    for r in scheduled_runs(&pool).await {
        let want = if r.1 == a { "awaiting_batch" } else { "done" };
        assert_eq!(r.2, want);
    }

    // A's own batch then ends and is applied normally.
    ai.set_status(&first, BatchStatus::Ended);
    poll(&d).await;
    assert_eq!(ready_of(a).await, 5);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_batch_that_never_ends_is_given_up_to_sync(pool: PgPool) {
    let ai = Arc::new(TestAi::new(Box::new(all_valid)));
    ai.set_default_status(BatchStatus::InProgress);
    let (d, _, _) = two_scheduled(&pool, ai.clone()).await;
    let age = BATCH_GIVE_UP_AFTER + std::time::Duration::from_secs(3600);
    sqlx::query("UPDATE ai_batches SET submitted_at = now() - make_interval(secs => $1)")
        .bind(age.as_secs_f64())
        .execute(&pool)
        .await
        .unwrap();
    poll(&d).await;
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM ai_batches WHERE status = 'failed'"
        )
        .await,
        1
    );
    assert_eq!(
        count(&pool, "SELECT count(*) FROM report_sections").await,
        0
    );
    assert_eq!(jobs_of(&pool, KIND_GENERATE_REPORT).await.len(), 2);
    for r in scheduled_runs(&pool).await {
        assert_eq!(r.2, "queued");
    }
    // Given up once: a later poll does not see it.
    let before = footprint(&pool).await;
    poll(&d).await;
    assert_eq!(footprint(&pool).await, before);
}

// ---------------------------------------------------------------------------------------------
// Weekly.

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn weekly_batch_round_trip(pool: PgPool) {
    let a = testkit::insert_account(&pool, "chesscom", "wa").await;
    let b = testkit::insert_account(&pool, "chesscom", "wb").await;
    let ai = Arc::new(TestAi::new(Box::new(all_valid)));
    let d = deps(
        &pool,
        Some(ai.clone()),
        pro(&[a, b]),
        Arc::new(WeeklyFor(vec![a, b])),
    );
    run(&WeeklyBatchScheduleHandler(d.clone())).await;

    let subs = ai.submissions();
    assert_eq!(subs.len(), 1);
    assert_eq!(subs[0].1.len(), 2);
    let rows: Vec<(Uuid, String, String, Date)> = sqlx::query_as(
        "SELECT account_id, status::text, mode::text, week_start FROM weekly_reviews ORDER BY account_id",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(rows.len(), 2);
    for r in &rows {
        assert_eq!((r.1.as_str(), r.2.as_str()), ("pending", "batch"));
        // M5b Task B: the review is of the week that just ended, not the one starting today.
        assert_eq!(r.3, monday(OffsetDateTime::now_utc()) - TDuration::days(7));
    }
    // Scheduling the same week again adds nothing.
    run(&WeeklyBatchScheduleHandler(d.clone())).await;
    assert_eq!(ai.submissions().len(), 1);

    poll(&d).await;
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM weekly_reviews WHERE status = 'ready'"
        )
        .await,
        2
    );
    let ready = events(&pool, "weekly_review.ready").await;
    assert_eq!(ready.len(), 2);
    let rows = usage(&pool).await;
    assert_eq!(rows.len(), 2);
    for (feature, model, batch, _) in rows {
        assert_eq!(feature, "weekly_review");
        assert_eq!(model, Models::WEEKLY.as_str());
        assert!(batch);
    }
    assert_eq!(ai.sync_calls(), 0);
    assert!(jobs_of(&pool, KIND_WEEKLY_REVIEW).await.is_empty());
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn an_expired_weekly_result_enqueues_the_weekly_job(pool: PgPool) {
    let a = testkit::insert_account(&pool, "chesscom", "wa").await;
    let ai = Arc::new(TestAi::new(Box::new(expired)));
    let d = deps(
        &pool,
        Some(ai.clone()),
        pro(&[a]),
        Arc::new(WeeklyFor(vec![a])),
    );
    run(&WeeklyBatchScheduleHandler(d.clone())).await;
    poll(&d).await;
    let id: Uuid = sqlx::query_scalar("SELECT id FROM weekly_reviews")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        jobs_of(&pool, KIND_WEEKLY_REVIEW).await,
        vec![format!("weekly:{id}")]
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM weekly_reviews WHERE status = 'pending'"
        )
        .await,
        1
    );
    assert!(usage(&pool).await.is_empty());
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn null_providers_have_nothing_due(pool: PgPool) {
    let p = NullWeeklyProvider;
    let due: Vec<_> = p
        .due(monday(OffsetDateTime::now_utc()))
        .try_collect()
        .await
        .unwrap();
    assert!(due.is_empty());
    let none = NullInputProvider
        .coaching_input(Uuid::new_v4(), TimeClass::Rapid)
        .await
        .unwrap();
    assert!(none.is_none());
    let _ = pool;
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_result_for_a_run_no_longer_waiting_is_metered_but_not_applied(pool: PgPool) {
    let ai = Arc::new(TestAi::new(Box::new(all_valid)));
    let (d, a, _) = two_scheduled(&pool, ai.clone()).await;
    // A's run was finished elsewhere meanwhile.
    sqlx::query(
        "UPDATE report_runs SET status = 'failed' WHERE account_id = $1 AND trigger = 'scheduled'",
    )
    .bind(a)
    .execute(&pool)
    .await
    .unwrap();
    poll(&d).await;
    let a_sections: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM report_sections s JOIN reports r ON r.id = s.report_id \
         WHERE r.account_id = $1",
    )
    .bind(a)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(a_sections, 0);
    // Its five results were billed: metered once each, at batch rates.
    let batch_cost = cost_micro_usd(
        &PricingTable::seed(),
        Models::REPORT.as_str(),
        &BATCH_USAGE,
        true,
    )
    .unwrap();
    let rows: Vec<(bool, i64)> =
        sqlx::query_as("SELECT batch, cost_micro_usd FROM ai_usage WHERE account_id = $1")
            .bind(a)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(rows.len(), 5);
    for (batch, cost) in rows {
        assert!(batch);
        assert_eq!(cost, i64::try_from(batch_cost).unwrap());
    }
    assert_eq!(events(&pool, "report.ready").await.len(), 1);
    assert!(jobs_of(&pool, KIND_GENERATE_REPORT).await.is_empty());
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM ai_batches WHERE status = 'processed'"
        )
        .await,
        1
    );
}

// ---------------------------------------------------------------------------------------------
// Fix round 1: bounded result reads, forged ids, concurrent pollers.

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn an_undecodable_results_line_sends_only_its_item_to_sync(pool: PgPool) {
    let ai = Arc::new(TestAi::new(Box::new(all_valid)));
    let (d, _, _) = two_scheduled(&pool, ai.clone()).await;
    ai.state.lock().unwrap().poison_at = Some(3);
    let poisoned = ai.submissions()[0].1[3].custom_id.clone();
    poll(&d).await;

    let run: Uuid =
        sqlx::query_scalar("SELECT report_run_id FROM ai_batch_items WHERE custom_id = $1")
            .bind(&poisoned)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM report_sections WHERE status = 'ready'"
        )
        .await,
        9,
        "every other line still applies"
    );
    assert_eq!(usage(&pool).await.len(), 9);
    assert_eq!(
        jobs_of(&pool, KIND_GENERATE_REPORT).await,
        vec![format!("report:{run}")]
    );
    for r in scheduled_runs(&pool).await {
        let want = if r.0 == run { "queued" } else { "done" };
        assert_eq!(r.2, want);
    }
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM ai_batches WHERE status = 'processed'"
        )
        .await,
        1
    );
    // The poison line is not re-read forever.
    let calls = ai.results_calls();
    poll(&d).await;
    assert_eq!(ai.results_calls(), calls);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn an_ended_batch_whose_results_never_read_is_given_up(pool: PgPool) {
    let ai = Arc::new(TestAi::new(Box::new(all_valid)));
    let (d, _, _) = two_scheduled(&pool, ai.clone()).await;
    ai.state.lock().unwrap().results_fail = true;
    poll(&d).await;
    // Within the bound: still open, nothing written.
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM ai_batches WHERE status = 'ended'"
        )
        .await,
        1
    );
    assert!(jobs_of(&pool, KIND_GENERATE_REPORT).await.is_empty());

    let age = BATCH_RESULTS_GIVE_UP_AFTER + std::time::Duration::from_secs(600);
    sqlx::query("UPDATE ai_batches SET ended_at = now() - make_interval(secs => $1)")
        .bind(age.as_secs_f64())
        .execute(&pool)
        .await
        .unwrap();
    poll(&d).await;
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM ai_batches WHERE status = 'failed'"
        )
        .await,
        1
    );
    assert_eq!(
        count(&pool, "SELECT count(*) FROM report_sections").await,
        0
    );
    assert!(usage(&pool).await.is_empty());
    assert_eq!(jobs_of(&pool, KIND_GENERATE_REPORT).await.len(), 2);
    for r in scheduled_runs(&pool).await {
        assert_eq!(r.2, "queued");
    }
    let calls = ai.results_calls();
    poll(&d).await;
    assert_eq!(
        ai.results_calls(),
        calls,
        "a failed batch is not polled again"
    );
}

async fn item_of(pool: &PgPool, account: Uuid, section: &str) -> (Uuid, String) {
    sqlx::query_as(
        "SELECT i.id, i.custom_id FROM ai_batch_items i \
         JOIN report_runs r ON r.id = i.report_run_id \
         WHERE r.account_id = $1 AND i.section = $2",
    )
    .bind(account)
    .bind(section)
    .fetch_one(pool)
    .await
    .unwrap()
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_forged_item_id_is_never_applied(pool: PgPool) {
    let ai = Arc::new(TestAi::new(Box::new(all_valid)));
    let (d, a, b) = two_scheduled(&pool, ai.clone()).await;
    // Swap the custom ids of an item of A (overview) and an item of B (areas): each custom id
    // now finds the other account's row, whose id it does not encode.
    let (ia, ca) = item_of(&pool, a, "overview").await;
    let (ib, cb) = item_of(&pool, b, "areas").await;
    let mut tx = pool.begin().await.unwrap();
    for (id, cid) in [(ia, "tmp".to_owned()), (ib, ca), (ia, cb)] {
        sqlx::query("UPDATE ai_batch_items SET custom_id = $2 WHERE id = $1")
            .bind(id)
            .bind(cid)
            .execute(&mut *tx)
            .await
            .unwrap();
    }
    tx.commit().await.unwrap();
    poll(&d).await;

    for acc in [a, b] {
        let n: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM report_sections s JOIN reports r ON r.id = s.report_id \
             WHERE r.account_id = $1",
        )
        .bind(acc)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(n, 4, "the forged result is applied nowhere");
        let m: i64 = sqlx::query_scalar("SELECT count(*) FROM ai_usage WHERE account_id = $1")
            .bind(acc)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(m, 4, "and metered nowhere");
    }
    assert_eq!(ai.sync_calls(), 0);
    assert_eq!(jobs_of(&pool, KIND_GENERATE_REPORT).await.len(), 2);
    for r in scheduled_runs(&pool).await {
        assert_eq!(r.2, "queued");
    }
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn concurrent_pollers_apply_and_meter_once(pool: PgPool) {
    let ai = Arc::new(TestAi::new(Box::new(all_valid)));
    let (d, _, _) = two_scheduled(&pool, ai.clone()).await;
    tokio::join!(poll(&d), poll(&d), poll(&d));
    assert_eq!(usage(&pool).await.len(), 10);
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM report_sections WHERE status = 'ready'"
        )
        .await,
        10
    );
    assert_eq!(events(&pool, "report.ready").await.len(), 2);
    let section_events = events(&pool, "report.section.ready").await;
    let mut seen = std::collections::HashSet::new();
    for (account, p) in &section_events {
        assert!(
            seen.insert((*account, p["reportId"].clone(), p["section"].clone())),
            "duplicate event {p}"
        );
    }
}

/// M5b Task B (G2): the Monday schedule reviews the complete week before it.
#[test]
fn the_reviewed_week_is_the_last_complete_one() {
    use time::macros::{date, datetime};
    // Monday 04:00 UTC, when the schedule runs: the week of Monday 16 Nov.
    assert_eq!(
        reviewed_week(datetime!(2026-11-23 04:00 UTC)),
        date!(2026 - 11 - 16)
    );
    // Sunday night is still inside the week of 16 Nov, so the complete week is the one before.
    assert_eq!(
        reviewed_week(datetime!(2026-11-22 23:59 UTC)),
        date!(2026 - 11 - 09)
    );
    // Another offset is converted to UTC first.
    assert_eq!(
        reviewed_week(datetime!(2026-11-23 01:00 +2)),
        date!(2026 - 11 - 09)
    );
}

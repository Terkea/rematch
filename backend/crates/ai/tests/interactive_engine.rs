//! Task 24: interactive engine checks for the coach chat. `DbInteractiveEngine` charges the
//! account's engine-check quota atomically, reuses a recent result for free, and waits on the
//! `interactive_evals` row that `InteractiveAnalyseHandler` (an `interactive-analyse` job on R1's
//! queue) fills. Results never reach `position_evals`.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use ai::chat::data_source::{EngineCheck, InteractiveEngine};
use ai::chat::interactive::{
    DbInteractiveEngine, INTERACTIVE_MULTIPV, INTERACTIVE_NODES, INTERACTIVE_PROTOCOL,
    INTERACTIVE_TIMEOUT, InteractiveAnalyseHandler, REUSE_WINDOW, engine_evidence_from_row,
};
use ai::db::chat::{self, ChatRole, ChatStatus, NewConversation, NewMessage};
use ai::db::interactive::{self, EvalRow, EvalStatus, NewEval};
use ai::entitlements::{Entitlements, Plan, QuotaWindow};
use ai::input::types::Score;
use ai::jobs::kinds::KIND_INTERACTIVE_ANALYSE;
use ai::validator::evidence::create_evidence;
use ai::validator::format::resolve_token;
use ai::validator::tokens::parse_tokens;
use chess_core::{EngineScore, START_FEN};
use engine::{AnalyseOptions, Analyser, AnalysisLine, AnalysisResult, EngineError};
use futures::future::BoxFuture;
use queue::queue::Queue;
use queue::runner::{Handler, JobContext, Runner, RunnerConfig, RunnerHandle};
use sqlx::PgPool;
use time::{Duration as TimeDuration, OffsetDateTime, Time, Weekday};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

// --- Fakes ---

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Ok,
    Crash,
    PoolClosed,
}

/// Returns 3 lines after `delay` (side-to-move scores 20, 10, -5; the pv is legal after
/// 1.e4 e5 2.Nf3 only, so SAN is empty elsewhere).
struct FakeAnalyser {
    delay: Duration,
    mode: Mode,
    calls: Mutex<Vec<(String, AnalyseOptions)>>,
}

impl FakeAnalyser {
    fn new(delay: Duration, mode: Mode) -> Arc<Self> {
        Arc::new(Self {
            delay,
            mode,
            calls: Mutex::new(Vec::new()),
        })
    }

    fn call_count(&self) -> usize {
        self.calls.lock().unwrap().len()
    }
}

fn line(multipv: u32, cp: i32, pv: &[&str]) -> AnalysisLine {
    AnalysisLine {
        multipv,
        score: EngineScore::Cp(cp),
        pv: pv.iter().map(|s| (*s).to_owned()).collect(),
        depth: 22,
        nodes: 1_000_000,
    }
}

impl Analyser for FakeAnalyser {
    fn analyse<'a>(
        &'a self,
        fen: &'a str,
        opts: AnalyseOptions,
    ) -> BoxFuture<'a, Result<AnalysisResult, EngineError>> {
        Box::pin(async move {
            self.calls.lock().unwrap().push((fen.to_owned(), opts));
            tokio::time::sleep(self.delay).await;
            match self.mode {
                Mode::Crash => Err(EngineError::Crashed),
                Mode::PoolClosed => Err(EngineError::PoolClosed),
                Mode::Ok => Ok(AnalysisResult {
                    lines: vec![
                        line(1, 20, &["b8c6", "f1b5"]),
                        line(2, 10, &["g8f6", "f3e5"]),
                        line(3, -5, &["d7d6", "d2d4"]),
                    ],
                    wall: self.delay,
                    cpu: None,
                }),
            }
        })
    }
}

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

// --- Fixtures ---

const AFTER_NF3: [&str; 3] = ["e2e4", "e7e5", "g1f3"];
const FIRST_MOVES: [&str; 8] = [
    "e2e4", "d2d4", "c2c4", "g1f3", "b1c3", "f2f4", "g2g3", "b2b3",
];

fn mv(moves: &[&str]) -> Vec<String> {
    moves.iter().map(|s| (*s).to_owned()).collect()
}

struct Account {
    id: Uuid,
    message_id: Uuid,
}

async fn account(pool: &PgPool, name: &str) -> Account {
    let id = testkit::insert_account(pool, "chesscom", name).await;
    let mut conn = pool.acquire().await.unwrap();
    let conv = chat::create_conversation(
        &mut conn,
        &NewConversation {
            account_id: id,
            user_id: None,
            time_class: "blitz".into(),
            title: None,
            anchor: None,
        },
    )
    .await
    .unwrap()
    .id;
    let message_id = chat::insert_message(
        &mut conn,
        &NewMessage {
            conversation_id: conv,
            seq: 1,
            role: ChatRole::Assistant,
            status: ChatStatus::Pending,
            anchor: None,
            text: None,
        },
    )
    .await
    .unwrap()
    .id;
    Account { id, message_id }
}

fn engine(pool: &PgPool, acct: &Account, plan: Plan) -> DbInteractiveEngine {
    DbInteractiveEngine::new(
        pool.clone(),
        Queue::new(pool.clone()),
        Arc::new(FixedPlan(plan)),
        acct.id,
        acct.message_id,
        OffsetDateTime::now_utc(),
    )
}

fn runner(pool: &PgPool, analyser: Arc<FakeAnalyser>) -> RunnerHandle {
    let handler = InteractiveAnalyseHandler {
        pool: pool.clone(),
        analyser,
    };
    Runner::spawn(
        Queue::new(pool.clone()),
        Arc::new(handler),
        RunnerConfig {
            concurrency: 4,
            ..RunnerConfig::fast_for_tests()
        },
    )
}

/// Seeds `n` charged evals of distinct positions (created now).
async fn seed_charged(pool: &PgPool, account_id: Uuid, n: usize) {
    let mut conn = pool.acquire().await.unwrap();
    for i in 0..n {
        interactive::insert_eval(
            &mut conn,
            &NewEval {
                account_id,
                message_id: None,
                protocol: INTERACTIVE_PROTOCOL.into(),
                base_fen: START_FEN.into(),
                moves: vec![],
                fen: format!("seed-{i}"),
            },
        )
        .await
        .unwrap();
    }
}

async fn rows(pool: &PgPool, account_id: Uuid) -> Vec<EvalRow> {
    let ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM interactive_evals WHERE account_id = $1 ORDER BY created_at, id",
    )
    .bind(account_id)
    .fetch_all(pool)
    .await
    .unwrap();
    let mut conn = pool.acquire().await.unwrap();
    let mut out = Vec::new();
    for id in ids {
        out.push(interactive::get_eval(&mut conn, id).await.unwrap().unwrap());
    }
    out
}

async fn charged(pool: &PgPool, account_id: Uuid) -> usize {
    rows(pool, account_id)
        .await
        .iter()
        .filter(|r| r.charged)
        .count()
}

async fn row(pool: &PgPool, eval_id: &str) -> EvalRow {
    let mut conn = pool.acquire().await.unwrap();
    interactive::get_eval(&mut conn, Uuid::try_parse(eval_id).unwrap())
        .await
        .unwrap()
        .expect("eval row")
}

fn eval_id(check: &EngineCheck) -> String {
    match check {
        EngineCheck::Done { eval_id, .. } | EngineCheck::Pending { eval_id } => eval_id.clone(),
        other => panic!("expected Done or Pending, got {other:?}"),
    }
}

fn next_monday(now: OffsetDateTime) -> OffsetDateTime {
    let mut d = now.date() + TimeDuration::days(1);
    while d.weekday() != Weekday::Monday {
        d += TimeDuration::days(1);
    }
    d.with_time(Time::MIDNIGHT).assume_utc()
}

const LONG: Duration = Duration::from_secs(5);

// --- Tests ---

#[test]
fn protocol_constants_are_pinned() {
    assert_eq!(INTERACTIVE_PROTOCOL, "sf19-n1m-mpv3-interactive-v1");
    assert_eq!(INTERACTIVE_NODES, 1_000_000);
    assert_eq!(INTERACTIVE_MULTIPV, 3);
    assert_eq!(INTERACTIVE_TIMEOUT, Duration::from_secs(30));
    assert_eq!(REUSE_WINDOW, Duration::from_secs(24 * 3600));
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn what_if_round_trip_with_a_fake_engine(pool: PgPool) {
    let fake = FakeAnalyser::new(Duration::from_millis(50), Mode::Ok);
    let r = runner(&pool, fake.clone());
    let acct = account(&pool, "whatif").await;
    let check = engine(&pool, &acct, Plan::Free)
        .request(START_FEN, &mv(&AFTER_NF3), LONG)
        .await
        .unwrap();
    let EngineCheck::Done { eval_id, evidence } = check else {
        panic!("expected Done, got {check:?}");
    };
    assert_eq!(eval_id.len(), 32, "eval ids are 32 lowercase hex");
    assert_eq!(evidence.tried_san, vec!["e4", "e5", "Nf3"]);
    assert_eq!(evidence.lines.len(), 3);
    // Black to move: the engine's +20 for the mover is -20 from White's point of view.
    assert_eq!(evidence.lines[0].score, Score::Cp(-20));
    assert_eq!(evidence.lines[0].san, vec!["Nc6", "Bb5"]);

    let mut ev = create_evidence();
    ev.add_engine_eval(eval_id.clone(), evidence);
    let parsed = parse_tokens(&format!(
        "{{{{eng:{eval_id}:1}}}} and {{{{eng:{eval_id}:1:1}}}}"
    ));
    assert_eq!(parsed.errors, vec![]);
    assert!(resolve_token(&ev, &parsed.tokens[0].body).is_some());
    assert_eq!(
        resolve_token(&ev, &parsed.tokens[1].body).as_deref(),
        Some("Nc6")
    );

    let stored = row(&pool, &eval_id).await;
    assert_eq!(stored.status, EvalStatus::Done);
    assert_eq!(stored.protocol, INTERACTIVE_PROTOCOL);
    assert_eq!(stored.source, "interactive");
    assert!(stored.charged);
    assert_eq!(stored.account_id, acct.id);
    assert_eq!(stored.message_id, Some(acct.message_id));
    assert_eq!(stored.moves, mv(&AFTER_NF3));
    let lines = stored.lines.clone().unwrap();
    assert_eq!(lines[0]["multipv"], 1);
    assert_eq!(lines[0]["score"], serde_json::json!({"cp": -20}));
    assert_eq!(lines[0]["pv"], serde_json::json!(["b8c6", "f1b5"]));
    assert_eq!(lines[0]["san"], serde_json::json!(["Nc6", "Bb5"]));
    assert_eq!(lines[0]["depth"], 22);
    assert_eq!(lines[0]["nodes"], 1_000_000);
    assert_eq!(
        engine_evidence_from_row(&stored).lines[2].score,
        Score::Cp(5)
    );

    let calls = fake.calls.lock().unwrap().clone();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, stored.fen);
    assert_eq!(
        calls[0].1,
        AnalyseOptions {
            nodes: 1_000_000,
            multipv: 3,
            timeout: Duration::from_secs(30),
        }
    );
    r.shutdown(LONG).await;
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn interactive_results_never_touch_position_evals(pool: PgPool) {
    let before: i64 = sqlx::query_scalar("SELECT count(*) FROM position_evals")
        .fetch_one(&pool)
        .await
        .unwrap();
    let r = runner(&pool, FakeAnalyser::new(Duration::ZERO, Mode::Ok));
    let acct = account(&pool, "noevals").await;
    let eng = engine(&pool, &acct, Plan::Free);
    for m in &FIRST_MOVES[..3] {
        let c = eng.request(START_FEN, &mv(&[m]), LONG).await.unwrap();
        assert!(matches!(c, EngineCheck::Done { .. }), "{c:?}");
    }
    let after: i64 = sqlx::query_scalar("SELECT count(*) FROM position_evals")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(after, before);
    r.shutdown(LONG).await;
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn slow_engine_returns_pending_then_wait_completes(pool: PgPool) {
    let r = runner(
        &pool,
        FakeAnalyser::new(Duration::from_millis(1500), Mode::Ok),
    );
    let acct = account(&pool, "slow").await;
    let eng = engine(&pool, &acct, Plan::Free);
    let first = eng
        .request(START_FEN, &mv(&AFTER_NF3), Duration::from_millis(200))
        .await
        .unwrap();
    let EngineCheck::Pending { eval_id } = first else {
        panic!("expected Pending, got {first:?}");
    };
    let done = eng.wait(&eval_id, LONG).await.unwrap();
    let EngineCheck::Done {
        eval_id: done_id,
        evidence,
    } = done
    else {
        panic!("expected Done, got {done:?}");
    };
    assert_eq!(done_id, eval_id);
    assert_eq!(evidence.tried_san, vec!["e4", "e5", "Nf3"]);
    r.shutdown(LONG).await;
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn engine_quota_exhaustion_returns_limit_and_note(pool: PgPool) {
    let acct = account(&pool, "freelimit").await;
    seed_charged(&pool, acct.id, 3).await;
    let now = OffsetDateTime::now_utc();
    let check = engine(&pool, &acct, Plan::Free)
        .request(START_FEN, &mv(&["e2e4"]), LONG)
        .await
        .unwrap();
    assert_eq!(
        check,
        EngineCheck::Limit {
            window: QuotaWindow::Week,
            resets_at: next_monday(now),
        }
    );
    assert_eq!(rows(&pool, acct.id).await.len(), 3, "no new row");
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn pro_quota_is_20_per_day(pool: PgPool) {
    let acct = account(&pool, "pro").await;
    seed_charged(&pool, acct.id, 19).await;
    let eng = engine(&pool, &acct, Plan::Pro);
    let twentieth = eng
        .request(START_FEN, &mv(&["e2e4"]), Duration::ZERO)
        .await
        .unwrap();
    assert!(
        matches!(twentieth, EngineCheck::Pending { .. }),
        "{twentieth:?}"
    );
    let now = OffsetDateTime::now_utc();
    let check = eng
        .request(START_FEN, &mv(&["d2d4"]), Duration::ZERO)
        .await
        .unwrap();
    let tomorrow = (now.date() + TimeDuration::days(1))
        .with_time(Time::MIDNIGHT)
        .assume_utc();
    assert_eq!(
        check,
        EngineCheck::Limit {
            window: QuotaWindow::Day,
            resets_at: tomorrow,
        }
    );
    assert_eq!(rows(&pool, acct.id).await.len(), 20);
}

/// N+1 parallel requests against N remaining checks: exactly N are accepted.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn parallel_requests_never_overspend_the_quota(pool: PgPool) {
    for (plan, seeded, remaining) in [(Plan::Free, 0, 3), (Plan::Pro, 15, 5)] {
        let acct = account(&pool, &format!("race-{plan:?}")).await;
        seed_charged(&pool, acct.id, seeded).await;
        let eng = Arc::new(engine(&pool, &acct, plan));
        let barrier = Arc::new(tokio::sync::Barrier::new(remaining + 1));
        let mut tasks = Vec::new();
        for m in &FIRST_MOVES[..=remaining] {
            let (eng, barrier, moves) = (eng.clone(), barrier.clone(), mv(&[m]));
            tasks.push(tokio::spawn(async move {
                barrier.wait().await;
                eng.request(START_FEN, &moves, Duration::ZERO)
                    .await
                    .unwrap()
            }));
        }
        let mut accepted = 0;
        let mut limited = 0;
        for t in tasks {
            match t.await.unwrap() {
                EngineCheck::Pending { .. } => accepted += 1,
                EngineCheck::Limit { .. } => limited += 1,
                other => panic!("unexpected {other:?}"),
            }
        }
        assert_eq!((accepted, limited), (remaining, 1), "{plan:?}");
        assert_eq!(
            charged(&pool, acct.id).await,
            seeded + remaining,
            "{plan:?}"
        );
    }
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn reused_result_is_not_charged(pool: PgPool) {
    let fake = FakeAnalyser::new(Duration::ZERO, Mode::Ok);
    let r = runner(&pool, fake.clone());
    let acct = account(&pool, "reuse").await;
    let eng = engine(&pool, &acct, Plan::Free);
    let first = eng.request(START_FEN, &mv(&AFTER_NF3), LONG).await.unwrap();
    let second = eng.request(START_FEN, &mv(&AFTER_NF3), LONG).await.unwrap();
    let (EngineCheck::Done { evidence: e1, .. }, EngineCheck::Done { evidence: e2, .. }) =
        (&first, &second)
    else {
        panic!("expected two Done, got {first:?} and {second:?}");
    };
    assert_eq!(e1, e2);
    assert_ne!(
        eval_id(&first),
        eval_id(&second),
        "the reuse is its own row"
    );
    assert_eq!(fake.call_count(), 1);
    assert_eq!(charged(&pool, acct.id).await, 1);
    let reused = row(&pool, &eval_id(&second)).await;
    assert_eq!(reused.status, EvalStatus::Done);
    assert!(!reused.charged);
    assert_eq!(reused.message_id, Some(acct.message_id));

    // A reuse is free even when the quota is used up.
    seed_charged(&pool, acct.id, 2).await;
    let third = eng.request(START_FEN, &mv(&AFTER_NF3), LONG).await.unwrap();
    assert!(matches!(third, EngineCheck::Done { .. }), "{third:?}");
    assert_eq!(fake.call_count(), 1);
    r.shutdown(LONG).await;
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_result_older_than_the_reuse_window_is_not_reused(pool: PgPool) {
    let fake = FakeAnalyser::new(Duration::ZERO, Mode::Ok);
    let r = runner(&pool, fake.clone());
    let acct = account(&pool, "stale").await;
    let eng = engine(&pool, &acct, Plan::Pro);
    let first = eng.request(START_FEN, &mv(&AFTER_NF3), LONG).await.unwrap();
    sqlx::query(
        "UPDATE interactive_evals SET created_at = now() - interval '24 hours 1 minute' \
         WHERE id = $1",
    )
    .bind(Uuid::try_parse(&eval_id(&first)).unwrap())
    .execute(&pool)
    .await
    .unwrap();
    let second = eng.request(START_FEN, &mv(&AFTER_NF3), LONG).await.unwrap();
    assert!(matches!(second, EngineCheck::Done { .. }), "{second:?}");
    assert_eq!(fake.call_count(), 2);
    r.shutdown(LONG).await;
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn failed_analysis_refunds_quota(pool: PgPool) {
    let r = runner(&pool, FakeAnalyser::new(Duration::ZERO, Mode::Crash));
    let acct = account(&pool, "crash").await;
    let check = engine(&pool, &acct, Plan::Free)
        .request(START_FEN, &mv(&["e2e4"]), LONG)
        .await
        .unwrap();
    assert!(
        matches!(check, EngineCheck::Unavailable { .. }),
        "{check:?}"
    );
    let all = rows(&pool, acct.id).await;
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].status, EvalStatus::Failed);
    assert!(!all[0].charged, "refunded");
    r.shutdown(LONG).await;
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn abandon_refunds_quota(pool: PgPool) {
    let acct = account(&pool, "abandon").await;
    let eng = engine(&pool, &acct, Plan::Free);
    let check = eng
        .request(START_FEN, &mv(&["e2e4"]), Duration::ZERO)
        .await
        .unwrap();
    let id = eval_id(&check);
    assert_eq!(charged(&pool, acct.id).await, 1);
    eng.abandon(&id).await.unwrap();
    assert_eq!(charged(&pool, acct.id).await, 0);
}

/// T23 m6: a turn cancelled while it waits on a check (its future dropped) refunds the check.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_dropped_wait_refunds_quota(pool: PgPool) {
    let acct = account(&pool, "dropped").await;
    let eng = engine(&pool, &acct, Plan::Free);
    let cut = tokio::time::timeout(
        Duration::from_millis(300),
        eng.request(START_FEN, &mv(&["e2e4"]), LONG),
    )
    .await;
    assert!(cut.is_err(), "the request was still waiting");
    let pending = eng
        .request(START_FEN, &mv(&["d2d4"]), Duration::ZERO)
        .await
        .unwrap();
    let cut = tokio::time::timeout(
        Duration::from_millis(300),
        eng.wait(&eval_id(&pending), LONG),
    )
    .await;
    assert!(cut.is_err(), "the wait was still waiting");
    testkit::wait_for(
        "both dropped checks refunded",
        LONG,
        Duration::from_millis(50),
        || {
            let pool = pool.clone();
            async move { (charged(&pool, acct.id).await == 0).then_some(()) }
        },
    )
    .await;
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn pool_closed_releases_the_job(pool: PgPool) {
    let fake = FakeAnalyser::new(Duration::ZERO, Mode::PoolClosed);
    let acct = account(&pool, "closed").await;
    let check = engine(&pool, &acct, Plan::Free)
        .request(START_FEN, &mv(&["e2e4"]), Duration::ZERO)
        .await
        .unwrap();
    let queue = Queue::new(pool.clone());
    let jobs = queue
        .dequeue(
            &[KIND_INTERACTIVE_ANALYSE],
            "test",
            1,
            Duration::from_secs(30),
        )
        .await
        .unwrap();
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].priority, 1);
    assert_eq!(jobs[0].user_key, Some(acct.id));
    let handler = InteractiveAnalyseHandler {
        pool: pool.clone(),
        analyser: fake.clone(),
    };
    let res = handler
        .handle(
            jobs[0].clone(),
            JobContext {
                cancel: CancellationToken::new(),
            },
        )
        .await;
    match res {
        Err(queue::JobError::Release { delay, .. }) => assert_eq!(delay, Duration::from_secs(5)),
        other => panic!("expected Release, got {other:?}"),
    }
    let stored = row(&pool, &eval_id(&check)).await;
    assert_eq!(
        stored.status,
        EvalStatus::Pending,
        "the check waits for the pool"
    );
    assert!(stored.charged);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn quota_is_per_account(pool: PgPool) {
    let a = account(&pool, "quota-a").await;
    let b = account(&pool, "quota-b").await;
    seed_charged(&pool, a.id, 3).await;
    let ea = engine(&pool, &a, Plan::Free);
    let eb = engine(&pool, &b, Plan::Free);
    let limited = ea
        .request(START_FEN, &mv(&["e2e4"]), Duration::ZERO)
        .await
        .unwrap();
    assert!(matches!(limited, EngineCheck::Limit { .. }), "{limited:?}");
    let ok = eb
        .request(START_FEN, &mv(&["e2e4"]), Duration::ZERO)
        .await
        .unwrap();
    let id = eval_id(&ok);

    // Ids are account-scoped: A cannot wait on or refund B's check.
    let foreign = ea.wait(&id, Duration::ZERO).await.unwrap();
    assert!(
        matches!(foreign, EngineCheck::Unavailable { .. }),
        "{foreign:?}"
    );
    ea.abandon(&id).await.unwrap();
    assert_eq!(charged(&pool, b.id).await, 1);
    let own = eb.wait(&id, Duration::ZERO).await.unwrap();
    assert_eq!(own, EngineCheck::Pending { eval_id: id });
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn illegal_moves_are_unavailable_and_uncharged(pool: PgPool) {
    let acct = account(&pool, "illegal").await;
    let check = engine(&pool, &acct, Plan::Free)
        .request(START_FEN, &mv(&["e2e5"]), Duration::ZERO)
        .await
        .unwrap();
    assert!(
        matches!(check, EngineCheck::Unavailable { .. }),
        "{check:?}"
    );
    assert!(rows(&pool, acct.id).await.is_empty());
}

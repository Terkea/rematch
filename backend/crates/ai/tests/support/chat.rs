//! Shared fixtures of the Task 25 tests: accounts, stored coaching inputs, games and moves of the
//! account, and helpers to read events and messages back.

#![allow(dead_code)]

use std::sync::Arc;

use ai::chat::job::ChatDeps;
use ai::chat::threads::AccountAccess;
use ai::client::cost::{AI_MONTHLY_BUDGET_MICRO_USD, PricingTable};
use ai::client::fake::{FakeAiClient, FakeTurn};
use ai::client::port::AiClient;
use ai::db::runs::{self, NewRun, ReportMode, ReportTrigger};
use ai::entitlements::{Entitlements, Plan};
use ai::input::types::{CoachingInput, ExemplarPosition};
use ai::jobs::budget::BudgetConfig;
use ai::testing::make_input;
use futures::future::BoxFuture;
use queue::queue::Queue;
use queue::types::Job;
use serde_json::{Value, json};
use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

pub const OK2: &str = "Next time, look at checks and captures before you move.";
pub const OK3: &str = "Take a moment on each move in sharp positions.";

/// The test user of an account's threads (one per account unless a test says otherwise).
pub fn user(account_id: Uuid) -> Uuid {
    Uuid::from_u128(account_id.as_u128() ^ 0x5EED)
}

/// Every user is linked to every account.
pub struct AnyLink;

impl AccountAccess for AnyLink {
    fn is_linked<'a>(
        &'a self,
        _user_id: Uuid,
        _account_id: Uuid,
    ) -> BoxFuture<'a, anyhow::Result<bool>> {
        Box::pin(async { Ok(true) })
    }
}

/// Exactly these `(user, account)` links.
pub struct Links(pub Vec<(Uuid, Uuid)>);

impl AccountAccess for Links {
    fn is_linked<'a>(
        &'a self,
        user_id: Uuid,
        account_id: Uuid,
    ) -> BoxFuture<'a, anyhow::Result<bool>> {
        let ok = self.0.contains(&(user_id, account_id));
        Box::pin(async move { Ok(ok) })
    }
}

pub struct FixedPlan(pub Plan);

impl Entitlements for FixedPlan {
    fn plan<'a>(&'a self, _account_id: Uuid) -> BoxFuture<'a, Plan> {
        let p = self.0;
        Box::pin(async move { p })
    }

    fn budget_scope<'a>(&'a self, account_id: Uuid) -> BoxFuture<'a, Vec<Uuid>> {
        Box::pin(async move { vec![account_id] })
    }
}

/// A platform account and its test user ([`user`]).
pub async fn account(pool: &PgPool, name: &str) -> Uuid {
    let a = testkit::insert_account(pool, "chesscom", name).await;
    testkit::insert_user_with_id(pool, user(a), name).await;
    a
}

/// The b2-rapid fixture for `account_id`, stored as the snapshot of a report run.
pub async fn store_input(pool: &PgPool, account_id: Uuid) -> CoachingInput {
    let raw = make_input(json!({ "accountId": account_id }));
    let mut conn = pool.acquire().await.unwrap();
    let run = runs::insert_run(
        &mut conn,
        &NewRun {
            account_id,
            time_class: "rapid".into(),
            trigger: ReportTrigger::Manual,
            plan: Plan::Free,
            mode: ReportMode::Sync,
            prompt_version: "report@1".into(),
            is_first_report: true,
            ai_disabled_reason: None,
        },
    )
    .await
    .unwrap();
    runs::snapshot_input(&mut conn, run, &raw, "h", "p")
        .await
        .unwrap();
    serde_json::from_value(raw).unwrap()
}

/// A rapid game of the account; returns its row id.
pub async fn seed_game(pool: &PgPool, account_id: Uuid, game_id: &str) -> Uuid {
    testkit::insert_game(
        pool,
        account_id,
        testkit::GameSeed {
            platform_game_id: game_id.to_owned(),
            base_seconds: Some(600),
            increment_seconds: Some(0),
            status: "done",
            ..testkit::GameSeed::default()
        },
    )
    .await
}

/// One analysed move of a game.
pub async fn seed_move(
    pool: &PgPool,
    game: Uuid,
    ply: u32,
    fen: &str,
    san: &str,
    uci: &str,
    best_uci: Option<&str>,
) {
    let color = if fen.split(' ').nth(1) == Some("w") {
        "white"
    } else {
        "black"
    };
    sqlx::query(
        "INSERT INTO moves (game_id, ply, color, san, uci, fen_before, best_uci, win_before, \
         win_after, delta_w, accuracy, judgement, phase, clock_before, time_spent, decided) \
         VALUES ($1, $2, $3::color, $4, $5, $6, $7, 55.0, 30.0, 25.0, 40.0, 'blunder', \
         'middlegame', 120.0, 8.0, false)",
    )
    .bind(game)
    .bind(ply as i32)
    .bind(color)
    .bind(san)
    .bind(uci)
    .bind(fen)
    .bind(best_uci)
    .execute(pool)
    .await
    .unwrap();
}

/// Every exemplar position of the input (weaknesses, strengths, openings, changes).
pub fn exemplars(input: &CoachingInput) -> Vec<ExemplarPosition> {
    let mut out: Vec<ExemplarPosition> = Vec::new();
    out.extend(input.weaknesses.iter().flat_map(|w| w.exemplars.clone()));
    out.extend(input.strengths.iter().flat_map(|s| s.exemplars.clone()));
    out.extend(input.openings.iter().flat_map(|o| o.examples.clone()));
    out.extend(
        input
            .changes
            .iter()
            .flat_map(|c| c.examples.iter().map(|e| e.position.clone())),
    );
    out
}

/// Games and moves for every exemplar of the input, owned by `account_id`.
pub async fn seed_exemplar_positions(pool: &PgPool, account_id: Uuid, input: &CoachingInput) {
    let mut games = std::collections::HashMap::new();
    for e in exemplars(input) {
        let g = match games.get(&e.game_id) {
            Some(g) => *g,
            None => {
                let g = seed_game(pool, account_id, &e.game_id).await;
                games.insert(e.game_id.clone(), g);
                g
            }
        };
        seed_move(
            pool,
            g,
            e.ply,
            &e.fen,
            &e.played.san,
            &e.played.uci,
            e.best.as_ref().map(|b| b.uci.as_str()),
        )
        .await;
    }
}

/// `(type, payload)` of the account's user events in id order.
pub async fn events(pool: &PgPool, account_id: Uuid) -> Vec<(String, Value)> {
    sqlx::query_as("SELECT type, payload FROM user_events WHERE account_id = $1 ORDER BY id")
        .bind(account_id)
        .fetch_all(pool)
        .await
        .unwrap()
}

pub fn job(message_id: Uuid, attempts: i32) -> Job {
    let now = OffsetDateTime::now_utc();
    Job {
        id: 1,
        kind: "coach-chat".into(),
        payload: json!({ "messageId": message_id }),
        dedupe_key: None,
        user_key: None,
        priority: 10,
        attempts,
        max_attempts: 2,
        lease_token: Uuid::new_v4(),
        run_at: now,
        created_at: now,
        started_at: now,
    }
}

pub fn deps(pool: &PgPool, ai: Option<Arc<FakeAiClient>>, plan: Plan) -> ChatDeps {
    ChatDeps {
        pool: pool.clone(),
        queue: Queue::new(pool.clone()),
        ai: ai.map(|a| a as Arc<dyn AiClient>),
        entitlements: Arc::new(FixedPlan(plan)),
        access: Arc::new(AnyLink),
        budgets: BudgetConfig {
            per_plan: AI_MONTHLY_BUDGET_MICRO_USD,
            global_monthly: None,
        },
        pricing: Arc::new(PricingTable::seed()),
        now: Arc::new(OffsetDateTime::now_utc),
        turn_timeout: ai::jobs::kinds::CHAT_TURN_TIMEOUT,
    }
}

/// A clean one-message answer.
pub fn answer(paragraphs: &[&str]) -> FakeTurn {
    FakeTurn {
        deltas: vec![paragraphs.join("\n\n")],
        usage: ai::client::port::AiUsage {
            input_tokens: 1000,
            output_tokens: 100,
            cache_creation_tokens: 0,
            cache_read_tokens: 0,
        },
        ..FakeTurn::default()
    }
}

/// `(seq, role, status, failure_reason)` of a conversation's messages.
pub async fn statuses(
    pool: &PgPool,
    conversation: Uuid,
) -> Vec<(i32, String, String, Option<String>)> {
    sqlx::query_as(
        "SELECT seq, role::text, status::text, failure_reason FROM coach_messages \
         WHERE conversation_id = $1 ORDER BY seq",
    )
    .bind(conversation)
    .fetch_all(pool)
    .await
    .unwrap()
}

pub async fn usage_rows(
    pool: &PgPool,
    account_id: Uuid,
) -> Vec<(String, String, i64, Option<String>)> {
    sqlx::query_as(
        "SELECT feature::text, model, cost_micro_usd, ref_id FROM ai_usage \
         WHERE account_id = $1 ORDER BY id",
    )
    .bind(account_id)
    .fetch_all(pool)
    .await
    .unwrap()
}

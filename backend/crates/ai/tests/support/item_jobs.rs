//! Shared fixtures of the Task 19 tests (weekly reviews and change notes): accounts, deps over
//! the T12 fake client, jobs, and readers for rows, events, jobs and usage.

#![allow(dead_code)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use ai::client::cost::PricingTable;
use ai::client::fake::{FakeAiClient, FakeCallKind, FakeScript, FakeStructured};
use ai::client::port::{AiClient, AiError};
use ai::entitlements::{Entitlements, Plan};
use ai::jobs::budget::BudgetConfig;
use ai::jobs::weekly_job::AiItemDeps;
use futures::future::BoxFuture;
use queue::queue::Queue;
use queue::types::Job;
use serde_json::{Value, json};
use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

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

pub async fn account(pool: &PgPool, name: &str) -> Uuid {
    testkit::insert_account(pool, "chesscom", name).await
}

pub fn deps(pool: &PgPool, ai: Option<Arc<FakeAiClient>>) -> AiItemDeps {
    AiItemDeps {
        pool: pool.clone(),
        queue: Queue::new(pool.clone()),
        ai: ai.map(|a| a as Arc<dyn AiClient>),
        entitlements: Arc::new(FixedPlan(Plan::Pro)),
        budgets: BudgetConfig::default(),
        pricing: Arc::new(PricingTable::seed()),
        now: Arc::new(OffsetDateTime::now_utc),
    }
}

pub fn job(kind: &str, payload: Value) -> Job {
    let now = OffsetDateTime::now_utc();
    Job {
        id: 1,
        kind: kind.into(),
        payload,
        dedupe_key: None,
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

pub fn usage() -> ai::client::port::AiUsage {
    ai::client::port::AiUsage {
        input_tokens: 1000,
        output_tokens: 100,
        cache_creation_tokens: 0,
        cache_read_tokens: 0,
    }
}

/// A fake answering the n-th structured call (1-based) with `f(n)`.
pub fn fake(
    f: impl Fn(usize) -> Result<Value, AiError> + Send + Sync + 'static,
) -> Arc<FakeAiClient> {
    let n = AtomicUsize::new(0);
    Arc::new(FakeAiClient::new(FakeScript {
        structured: Some(Box::new(move |_, _| {
            f(n.fetch_add(1, Ordering::SeqCst) + 1).map(|v| FakeStructured {
                parsed: Some(v),
                usage: usage(),
                ..FakeStructured::default()
            })
        })),
        ..FakeScript::default()
    }))
}

pub fn structured_calls(f: &FakeAiClient) -> usize {
    f.calls()
        .into_iter()
        .filter(|c| c.kind == FakeCallKind::Structured)
        .count()
}

/// `(type, payload)` of the account's user events in id order.
pub async fn events(pool: &PgPool, account_id: Uuid) -> Vec<(String, Value)> {
    sqlx::query_as("SELECT type, payload FROM user_events WHERE account_id = $1 ORDER BY id")
        .bind(account_id)
        .fetch_all(pool)
        .await
        .unwrap()
}

/// `(feature, model, ref_id)` of the account's usage rows in id order.
pub async fn usage_rows(pool: &PgPool, account_id: Uuid) -> Vec<(String, String, Option<String>)> {
    sqlx::query_as(
        "SELECT feature::text, model, ref_id FROM ai_usage WHERE account_id = $1 ORDER BY id",
    )
    .bind(account_id)
    .fetch_all(pool)
    .await
    .unwrap()
}

/// `(kind, dedupe_key, payload)` of every job in id order.
pub async fn jobs(pool: &PgPool) -> Vec<(String, Option<String>, Value)> {
    sqlx::query_as("SELECT kind, dedupe_key, payload FROM jobs ORDER BY id")
        .fetch_all(pool)
        .await
        .unwrap()
}

pub fn empty() -> Value {
    json!([])
}

/// Delegates to `inner`, but structured call `from` (1-based) and later never return: a call
/// still in flight when the handler is cancelled.
pub struct Hang {
    pub inner: Arc<FakeAiClient>,
    pub from: usize,
    pub n: AtomicUsize,
}

impl Hang {
    pub fn new(inner: Arc<FakeAiClient>, from: usize) -> Arc<Hang> {
        Arc::new(Hang {
            inner,
            from,
            n: AtomicUsize::new(0),
        })
    }
}

impl AiClient for Hang {
    fn structured<'a>(
        &'a self,
        req: &'a ai::client::port::StructuredRequest,
    ) -> BoxFuture<'a, Result<ai::client::port::StructuredResult, AiError>> {
        let k = self.n.fetch_add(1, Ordering::SeqCst) + 1;
        Box::pin(async move {
            if k >= self.from {
                std::future::pending::<()>().await;
            }
            self.inner.structured(req).await
        })
    }

    fn stream_turn<'a>(
        &'a self,
        req: &'a ai::client::port::ChatTurnRequest,
        tools: Option<Arc<dyn ai::client::port::ToolHandler>>,
    ) -> futures::stream::BoxStream<'a, Result<ai::client::port::TurnEvent, AiError>> {
        self.inner.stream_turn(req, tools)
    }

    fn batch_submit<'a>(
        &'a self,
        items: &'a [ai::client::port::BatchItem],
    ) -> BoxFuture<'a, Result<String, AiError>> {
        self.inner.batch_submit(items)
    }

    fn batch_status<'a>(
        &'a self,
        id: &'a str,
    ) -> BoxFuture<'a, Result<ai::client::port::BatchStatus, AiError>> {
        self.inner.batch_status(id)
    }

    fn batch_results<'a>(
        &'a self,
        id: &'a str,
        resolver: &'a dyn ai::client::port::SchemaResolver,
    ) -> futures::stream::BoxStream<'a, Result<ai::client::port::BatchResultItem, AiError>> {
        self.inner.batch_results(id, resolver)
    }
}

/// `deps` with any client.
pub fn deps_with(pool: &PgPool, ai: Arc<dyn AiClient>) -> AiItemDeps {
    let mut d = deps(pool, None);
    d.ai = Some(ai);
    d
}

//! The AI coaching jobs (R3, ADRs 0057 to 0064), wired into [`start_workers`](crate::start).
//!
//! - **The client.** Interactive calls go to the Agent SDK sidecar, batches to the Messages
//!   Batches client (only with `ANTHROPIC_API_KEY`), behind one breaker (5 failures, 60 s). AI is
//!   enabled only when `AI_SIDECAR_URL` is set and the sidecar's `/health` reports a credential
//!   (`auth != "none"`). Without it every report is computed-only.
//! - **The log line** is exactly `ai: enabled (sidecar, batches on|off)` or
//!   `ai: disabled (<reason>)`. No key or token is ever logged (the env's `Debug` redacts them).
//! - **The runners.** One per AI kind ([`AI_RUNNER_KINDS`]), AI or not: without a client the
//!   report job still writes the computed report, and chat and the other narratives fail with
//!   their `ai_disabled` reasons. Every AI runner claims with [`PriorityFifo`] (the fair round
//!   robin refuses kinds other than `analyse-game`).
//! - **Entitlements** are [`ConfiguredEntitlements`] until M7 ([`ai_entitlements`]): every
//!   account gets the `AI_DEFAULT_PLAN` plan (beta by default, M5b ruling G1) with the
//!   `AI_BETA_*` limits. **Account access** for chat is the
//!   M4 `account_links` lookup ([`AccountLinksAccess`]). **The coaching input** is R2's
//!   builder ([`ProfileInputProvider`]), the weekly input R2's weekly builder
//!   (`coach_triggers::ProfileWeeklyProvider`, M5b ruling G2).

use std::sync::Arc;
use std::time::Duration;

use ai::chat::interactive::InteractiveAnalyseHandler;
use ai::chat::job::{ChatDeps, CoachChatHandler};
use ai::chat::threads::AccountLinksAccess;
use ai::client::batches::{BatchesClient, BatchesConfig};
use ai::client::breaker::{Breaker, BreakerConfig, with_breaker};
use ai::client::cost::PricingTable;
use ai::client::port::AiClient;
use ai::client::runtime::AiRuntime;
use ai::client::sidecar::{SidecarClient, SidecarConfig};
use ai::client::sidecar_wire::AuthKind;
use ai::client::tool_server::ToolServer;
use ai::entitlements::{ConfiguredEntitlements, Entitlements};
use ai::input::types::TimeClass;
use ai::jobs::batch_jobs::{
    AiBatchPollHandler, BatchDeps, ReportBatchScheduleHandler, WeeklyBatchScheduleHandler,
    WeeklyInputProvider,
};
use ai::jobs::budget::BudgetConfig;
use ai::jobs::change_job::ExplainChangeHandler;
use ai::jobs::kinds::{
    CHAT_TURN_TIMEOUT, KIND_AI_BATCH_POLL, KIND_AI_RECONCILE, KIND_COACH_CHAT, KIND_EXPLAIN_CHANGE,
    KIND_GENERATE_REPORT, KIND_INTERACTIVE_ANALYSE, KIND_REPORT_BATCH_SCHEDULE,
    KIND_WEEKLY_BATCH_SCHEDULE, KIND_WEEKLY_REVIEW,
};
use ai::jobs::providers::CoachingInputProvider;
use ai::jobs::reconcile::AiReconcileHandler;
use ai::jobs::report_job::{GenerateReportHandler, HEARTBEAT_EVERY, ReportDeps};
use ai::jobs::weekly_job::{AiItemDeps, WeeklyReviewHandler};
use anyhow::Context;
use engine::Analyser;
use futures::future::BoxFuture;
use queue::queue::Queue;
use queue::runner::{Handler, RunnerConfig};
use queue::strategy::PriorityFifo;
use serde_json::Value;
use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::config::WorkerEnv;

/// Every AI job kind, in the order its runner is spawned.
pub const AI_RUNNER_KINDS: [&str; 9] = [
    KIND_GENERATE_REPORT,
    KIND_WEEKLY_REVIEW,
    KIND_EXPLAIN_CHANGE,
    KIND_COACH_CHAT,
    KIND_INTERACTIVE_ANALYSE,
    KIND_REPORT_BATCH_SCHEDULE,
    KIND_WEEKLY_BATCH_SCHEDULE,
    KIND_AI_BATCH_POLL,
    KIND_AI_RECONCILE,
];

/// Connections the AI runners add beyond the analysis budget (see `start`'s module docs): one
/// NOTIFY listener per runner (9), and per job at once two for a report (its writes and the
/// heartbeat; 2 × 2), one for a weekly review or a note (2 + 2), two for a chat turn (the turn
/// lock's connection and one pooled; 4 × 2), one for an engine check, a batch job or the
/// reconciler (1 + 3 + 1). That is 9 + 4 + 4 + 8 + 5 = 30.
pub const AI_POOL_HEADROOM: usize = 30;

pub const REPORT_CONCURRENCY: usize = 2;
pub const WEEKLY_CONCURRENCY: usize = 2;
pub const CHANGE_CONCURRENCY: usize = 2;
pub const CHAT_CONCURRENCY: usize = 4;
pub const INTERACTIVE_CONCURRENCY: usize = 1;

/// A report generates five sections, two at a time, each with a possible repair.
pub const REPORT_HARD_TIMEOUT: Duration = Duration::from_secs(20 * 60);
/// A weekly review or a change note: one call and a possible repair.
pub const ITEM_HARD_TIMEOUT: Duration = Duration::from_secs(10 * 60);
/// Past the job's own turn bound ([`CHAT_TURN_TIMEOUT`], 10 min), so the handler normally ends
/// the turn and meters it itself. (The brief's 3 min predates the 10 min turn budget of T25.)
pub const CHAT_HARD_TIMEOUT: Duration = CHAT_TURN_TIMEOUT.saturating_add(Duration::from_secs(60));
/// Past the check's own 30 s engine deadline.
pub const INTERACTIVE_HARD_TIMEOUT: Duration = Duration::from_secs(2 * 60);
/// Past a batch submit (10 min) plus a results read (20 min).
pub const BATCH_HARD_TIMEOUT: Duration = Duration::from_secs(45 * 60);
/// Past the sweep's own deadline.
pub const AI_RECONCILE_HARD_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// How the worker gets its AI client.
#[derive(Clone, Default)]
pub enum AiSetup {
    /// No AI runners at all (the analysis CLI).
    None,
    /// From the environment: the sidecar, after its health check (the default).
    #[default]
    FromEnv,
    /// The AI runners with this client (`None`: AI disabled). Tests.
    With(Option<Arc<dyn AiClient>>),
}

impl std::fmt::Debug for AiSetup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            AiSetup::None => "None",
            AiSetup::FromEnv => "FromEnv",
            AiSetup::With(Some(_)) => "With(Some(..))",
            AiSetup::With(None) => "With(None)",
        })
    }
}

/// The client and the exact startup line.
pub struct AiClientChoice {
    pub client: Option<Arc<dyn AiClient>>,
    pub line: String,
}

fn disabled(reason: &str) -> AiClientChoice {
    AiClientChoice {
        client: None,
        line: format!("ai: disabled ({reason})"),
    }
}

/// Health checks before AI is disabled until the next start (review m5: a transient failure).
pub const HEALTH_ATTEMPTS: u32 = 3;
/// The pause between two health checks.
pub const HEALTH_RETRY_AFTER: Duration = Duration::from_secs(2);

/// The production client (module docs), or `None` with the reason.
pub async fn ai_client_from_env(env: &WorkerEnv) -> anyhow::Result<AiClientChoice> {
    let Some(sc) = &env.ai_sidecar else {
        return Ok(disabled("AI_SIDECAR_URL not set"));
    };
    let cfg = SidecarConfig::new(sc.url.clone(), sc.token.clone());
    let probe = SidecarClient::new(cfg.clone(), None).context("building the sidecar client")?;
    let mut health = probe.health().await;
    for _ in 1..HEALTH_ATTEMPTS {
        if health.is_ok() {
            break;
        }
        tokio::time::sleep(HEALTH_RETRY_AFTER).await;
        health = probe.health().await;
    }
    match health {
        Err(e) => {
            tracing::warn!(error = %e, attempts = HEALTH_ATTEMPTS, "the AI sidecar health check failed");
            return Ok(disabled("sidecar health check failed"));
        }
        Ok(h) if h.auth == AuthKind::None => return Ok(disabled("sidecar has no credential")),
        Ok(_) => {}
    }
    let tools = match &env.ai_tool_public_url {
        Some(url) => Some(
            ToolServer::start(env.ai_tool_bind, url.clone())
                .await
                .context("starting the AI tool endpoint (AI_TOOL_BIND)")?,
        ),
        None => {
            tracing::warn!("AI_TOOL_PUBLIC_URL is not set; chat answers without tools");
            None
        }
    };
    let sidecar = SidecarClient::new(cfg, tools).context("building the sidecar client")?;
    let batches = env
        .anthropic_api_key
        .clone()
        .map(|k| BatchesClient::new(BatchesConfig::new(k)))
        .transpose()
        .context("building the batches client")?;
    let line = format!(
        "ai: enabled (sidecar, batches {})",
        if batches.is_some() { "on" } else { "off" }
    );
    let runtime: Arc<dyn AiClient> = Arc::new(AiRuntime { sidecar, batches });
    let breaker = Arc::new(Breaker::new(BreakerConfig::default()));
    Ok(AiClientChoice {
        client: Some(with_breaker(runtime, breaker)),
        line,
    })
}

/// `AI_PRICING_FILE`, or the validated seed. A bad table is a startup error (ADR 0061).
pub fn pricing_from_env(env: &WorkerEnv) -> anyhow::Result<PricingTable> {
    match &env.ai_pricing_file {
        Some(path) => PricingTable::from_file(path)
            .with_context(|| format!("AI_PRICING_FILE {}", path.display())),
        None => {
            let seed = PricingTable::seed();
            seed.validate().context("the seed pricing table")?;
            Ok(seed)
        }
    }
}

/// The AI entitlements of this deployment: `AI_DEFAULT_PLAN` for every account (beta while the
/// deployment is a beta) with the configured beta limits.
pub fn ai_entitlements(env: &WorkerEnv) -> ConfiguredEntitlements {
    ConfiguredEntitlements {
        plan: env.ai_plan,
        beta: env.ai_beta,
    }
}

/// R2's builder as the report jobs' input. An account with no analysed game in the time class
/// has no input yet (the job fails `no_input` rather than write a report about nothing).
pub struct ProfileInputProvider {
    pub pool: PgPool,
}

fn core_time_class(tc: TimeClass) -> chess_core::TimeClass {
    match tc {
        TimeClass::Bullet => chess_core::TimeClass::Bullet,
        TimeClass::Blitz => chess_core::TimeClass::Blitz,
        TimeClass::Rapid => chess_core::TimeClass::Rapid,
        TimeClass::Classical => chess_core::TimeClass::Classical,
        TimeClass::Daily => chess_core::TimeClass::Daily,
    }
}

impl CoachingInputProvider for ProfileInputProvider {
    fn coaching_input<'a>(
        &'a self,
        account_id: Uuid,
        tc: TimeClass,
    ) -> BoxFuture<'a, anyhow::Result<Option<Value>>> {
        let pool = self.pool.clone();
        Box::pin(async move {
            // R2's builder future is not `Send` (it holds a `RefCell` across awaits), so it runs
            // to completion on a blocking thread, driven by this runtime's handle.
            let rt = tokio::runtime::Handle::current();
            let input = tokio::task::spawn_blocking(move || {
                rt.block_on(profile::coaching::builder::build_coaching_input(
                    &pool,
                    account_id,
                    core_time_class(tc),
                    OffsetDateTime::now_utc(),
                ))
            })
            .await
            .context("the coaching-input builder task")??;
            if input.basis.analysed_games == 0 {
                return Ok(None);
            }
            Ok(Some(serde_json::to_value(&input)?))
        })
    }
}

fn config(base: &RunnerConfig, concurrency: usize, hard_timeout: Duration) -> RunnerConfig {
    RunnerConfig {
        concurrency,
        hard_timeout: Some(hard_timeout),
        strategy: Arc::new(PriorityFifo),
        ..base.clone()
    }
}

/// The runner config of each AI kind.
pub fn ai_runner_config(base: &RunnerConfig, kind: &str) -> RunnerConfig {
    match kind {
        KIND_GENERATE_REPORT => config(base, REPORT_CONCURRENCY, REPORT_HARD_TIMEOUT),
        KIND_WEEKLY_REVIEW => config(base, WEEKLY_CONCURRENCY, ITEM_HARD_TIMEOUT),
        KIND_EXPLAIN_CHANGE => config(base, CHANGE_CONCURRENCY, ITEM_HARD_TIMEOUT),
        KIND_COACH_CHAT => config(base, CHAT_CONCURRENCY, CHAT_HARD_TIMEOUT),
        KIND_INTERACTIVE_ANALYSE => config(base, INTERACTIVE_CONCURRENCY, INTERACTIVE_HARD_TIMEOUT),
        KIND_AI_RECONCILE => config(base, 1, AI_RECONCILE_HARD_TIMEOUT),
        _ => config(base, 1, BATCH_HARD_TIMEOUT),
    }
}

/// What the AI runners share.
pub struct AiRunnerDeps {
    pub pool: PgPool,
    pub queue: Queue,
    pub ai: Option<Arc<dyn AiClient>>,
    /// The shared engine pool: interactive checks run on it (analysis keeps its own runner).
    pub analyser: Arc<dyn Analyser>,
    pub inputs: Arc<dyn CoachingInputProvider>,
    pub budgets: BudgetConfig,
    pub pricing: Arc<PricingTable>,
    /// [`ai_entitlements`] in production.
    pub entitlements: Arc<dyn Entitlements>,
    /// The weekly schedule's inputs: `coach_triggers::ProfileWeeklyProvider` in production.
    pub weekly: Arc<dyn WeeklyInputProvider>,
}

/// One handler and runner config per [`AI_RUNNER_KINDS`] entry, in that order. `start_workers`
/// spawns them through the same `Spawned::spawn` as every other runner, so its claims and its
/// shutdown cover them.
pub fn ai_runners(d: &AiRunnerDeps, base: &RunnerConfig) -> Vec<(Arc<dyn Handler>, RunnerConfig)> {
    let entitlements = d.entitlements.clone();
    let now: Arc<dyn Fn() -> OffsetDateTime + Send + Sync> = Arc::new(OffsetDateTime::now_utc);
    let item = || AiItemDeps {
        pool: d.pool.clone(),
        queue: d.queue.clone(),
        ai: d.ai.clone(),
        entitlements: entitlements.clone(),
        budgets: d.budgets,
        pricing: d.pricing.clone(),
        now: now.clone(),
    };
    let batch = || BatchDeps {
        pool: d.pool.clone(),
        queue: d.queue.clone(),
        ai: d.ai.clone(),
        inputs: d.inputs.clone(),
        weekly: d.weekly.clone(),
        entitlements: entitlements.clone(),
        budgets: d.budgets,
        pricing: d.pricing.clone(),
        now: now.clone(),
    };
    let handlers: Vec<Arc<dyn Handler>> = vec![
        Arc::new(GenerateReportHandler(ReportDeps {
            pool: d.pool.clone(),
            queue: d.queue.clone(),
            ai: d.ai.clone(),
            inputs: d.inputs.clone(),
            entitlements: entitlements.clone(),
            budgets: d.budgets,
            pricing: d.pricing.clone(),
            now: now.clone(),
            heartbeat_every: HEARTBEAT_EVERY,
        })),
        Arc::new(WeeklyReviewHandler(item())),
        Arc::new(ExplainChangeHandler(item())),
        Arc::new(CoachChatHandler(ChatDeps {
            pool: d.pool.clone(),
            queue: d.queue.clone(),
            ai: d.ai.clone(),
            entitlements: entitlements.clone(),
            access: Arc::new(AccountLinksAccess(d.pool.clone())),
            budgets: d.budgets,
            pricing: d.pricing.clone(),
            now: now.clone(),
            turn_timeout: CHAT_TURN_TIMEOUT,
        })),
        Arc::new(InteractiveAnalyseHandler {
            pool: d.pool.clone(),
            analyser: d.analyser.clone(),
        }),
        Arc::new(ReportBatchScheduleHandler(batch())),
        Arc::new(WeeklyBatchScheduleHandler(batch())),
        Arc::new(AiBatchPollHandler(batch())),
        Arc::new(AiReconcileHandler {
            pool: d.pool.clone(),
            queue: d.queue.clone(),
        }),
    ];
    handlers
        .into_iter()
        .map(|h| {
            let cfg = ai_runner_config(base, h.kind());
            (h, cfg)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use queue::strategy::SelectStrategy;

    #[test]
    fn ai_runner_configs() {
        let base = RunnerConfig::default();
        let fifo = PriorityFifo.order_by();
        for kind in AI_RUNNER_KINDS {
            let c = ai_runner_config(&base, kind);
            assert_eq!(c.strategy.order_by(), fifo, "{kind}");
            assert_eq!(c.worker_id, base.worker_id);
            assert!(c.hard_timeout.is_some(), "{kind}");
        }
        let at = |kind| {
            let c = ai_runner_config(&base, kind);
            (c.concurrency, c.hard_timeout.unwrap())
        };
        assert_eq!(at(KIND_GENERATE_REPORT), (2, REPORT_HARD_TIMEOUT));
        assert_eq!(at(KIND_WEEKLY_REVIEW).0, 2);
        assert_eq!(at(KIND_EXPLAIN_CHANGE).0, 2);
        assert_eq!(at(KIND_COACH_CHAT), (4, CHAT_HARD_TIMEOUT));
        assert_eq!(at(KIND_INTERACTIVE_ANALYSE).0, 1);
        for kind in [
            KIND_REPORT_BATCH_SCHEDULE,
            KIND_WEEKLY_BATCH_SCHEDULE,
            KIND_AI_BATCH_POLL,
            KIND_AI_RECONCILE,
        ] {
            assert_eq!(at(kind).0, 1, "{kind}");
        }
        // The runner never cuts a chat turn its own budget still allows.
        assert!(CHAT_HARD_TIMEOUT > CHAT_TURN_TIMEOUT);
        assert!(
            BATCH_HARD_TIMEOUT
                > ai::jobs::batch_jobs::BATCH_SUBMIT_TIMEOUT
                    + ai::jobs::batch_jobs::BATCH_RESULTS_TIMEOUT
        );
    }

    /// M5b Task B (G1): the AI plan is `AI_DEFAULT_PLAN`, beta by default, with the configured
    /// beta limits.
    #[test]
    fn ai_entitlements_follow_the_ai_default_plan() {
        use ai::entitlements::{BetaLimits, ConfiguredEntitlements, Plan as AiPlan};
        let limits = BetaLimits {
            chat_per_week: 9,
            manual_reports_per_day: 4,
        };
        let env = WorkerEnv {
            ai_beta: limits,
            ..WorkerEnv::new("postgres://db/x", "/sf")
        };
        assert_eq!(
            ai_entitlements(&env),
            ConfiguredEntitlements {
                plan: AiPlan::Beta,
                beta: limits
            }
        );
        for plan in [AiPlan::Free, AiPlan::Pro, AiPlan::Beta] {
            let env = WorkerEnv {
                ai_plan: plan,
                ..WorkerEnv::new("postgres://db/x", "/sf")
            };
            assert_eq!(ai_entitlements(&env).plan, plan);
        }
    }

    #[test]
    fn the_ai_setup_debug_shows_no_client() {
        assert_eq!(format!("{:?}", AiSetup::default()), "FromEnv");
        assert_eq!(format!("{:?}", AiSetup::With(None)), "With(None)");
    }
}

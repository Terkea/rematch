//! Interactive engine checks for the coach chat (Task 24, ADR-R3-6 "Interactive engine checks").
//!
//! [`DbInteractiveEngine`] is the chat's [`InteractiveEngine`] for one account and one assistant
//! message. A request is served in this order:
//!
//! 1. a `done` eval of the same account, FEN and protocol within [`REUSE_WINDOW`] is reused as a
//!    new, uncharged row;
//! 2. otherwise one check is reserved atomically against the plan's engine quotas
//!    ([`interactive::reserve_eval`]), or [`EngineCheck::Limit`] is returned with nothing written;
//! 3. the reserved row gets an `interactive-analyse` job (priority 1, `user_key` = account) on
//!    R1's queue, and the row is polled every [`POLL_EVERY`] up to the caller's wait.
//!
//! [`InteractiveAnalyseHandler`] runs that job on the engine pool and writes the lines into the
//! row: never into `position_evals`. A failure the player did not cause is refunded.
//!
//! **Cancellation (T23 m6).** A request or wait whose future is dropped while it polls (the turn
//! was cancelled) abandons its check from a drop guard, as `abandon` does. Abandoning cancels
//! as well as refunds ([`interactive::abandon_eval`]): a pending row becomes `failed`, and the
//! job's late lines are dropped, so a refunded check never becomes a free reusable result (review
//! I1). A row already `done` keeps its charge. Only charged originals are reused, so the 24 h
//! window runs from the original result. A check whose job is lost is failed and refunded by
//! `ai-reconcile`.

use std::sync::Arc;
use std::time::Duration;

use chess_core::board::Position;
use chess_core::{Color, EngineScore};
use engine::{AnalyseOptions, Analyser, AnalysisResult, EngineError};
use futures::future::BoxFuture;
use queue::queue::Queue;
use queue::runner::{Handler, JobContext};
use queue::{Job, JobError};
use serde_json::{Value, json};
use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

use super::data_source::{EngineCheck, InteractiveEngine};
use crate::db::interactive::{self, EvalRow, EvalStatus, NewEval};
use crate::entitlements::{Entitlements, engine_quotas};
use crate::input::types::Score;
use crate::jobs::kinds::{
    InteractiveAnalysePayload, KIND_INTERACTIVE_ANALYSE, enqueue_interactive,
};
use crate::validator::evidence::{EngineEvidence, EngineLine};
use crate::validator::pv::{fen_after, pv_to_san};

/// The protocol stored with every interactive eval (Stockfish 19, 1M nodes, MultiPV 3).
pub const INTERACTIVE_PROTOCOL: &str = "sf19-n1m-mpv3-interactive-v1";
pub const INTERACTIVE_NODES: u64 = 1_000_000;
pub const INTERACTIVE_MULTIPV: u32 = 3;
pub const INTERACTIVE_TIMEOUT: Duration = Duration::from_secs(30);
/// A finished eval of the same (account, FEN, protocol) is reused, free, within this window.
pub const REUSE_WINDOW: Duration = interactive::REUSE_WITHIN;
/// How often a waiting request re-reads its row.
pub const POLL_EVERY: Duration = Duration::from_millis(100);
/// The release delay of a job that found the engine pool closed (a shutdown).
pub const POOL_CLOSED_DELAY: Duration = Duration::from_secs(5);

fn options() -> AnalyseOptions {
    AnalyseOptions {
        nodes: INTERACTIVE_NODES,
        multipv: INTERACTIVE_MULTIPV,
        timeout: INTERACTIVE_TIMEOUT,
    }
}

fn eval_id(id: Uuid) -> String {
    id.simple().to_string()
}

fn unavailable(reason: impl Into<String>) -> EngineCheck {
    EngineCheck::Unavailable {
        reason: reason.into(),
    }
}

/// The normalised FEN after `moves` from `base_fen`, or why there is none.
fn position_after(base_fen: &str, moves: &[String]) -> Result<String, String> {
    let mut cur = Position::from_fen(base_fen)
        .map_err(|e| format!("invalid FEN: {e}"))?
        .fen();
    for m in moves {
        cur = fen_after(&cur, m).ok_or_else(|| format!("illegal move {m}"))?;
    }
    Ok(cur)
}

/// The chat's engine checks for one account, charged to its quota and linked to one assistant
/// message. Eval ids are 32 lowercase hex (`Uuid::simple`); `wait` and `abandon` only see this
/// account's evals.
pub struct DbInteractiveEngine {
    pool: PgPool,
    queue: Queue,
    entitlements: Arc<dyn Entitlements>,
    account_id: Uuid,
    message_id: Uuid,
    now: OffsetDateTime,
}

impl DbInteractiveEngine {
    /// `now` picks the quota windows (the turn's start).
    pub fn new(
        pool: PgPool,
        queue: Queue,
        entitlements: Arc<dyn Entitlements>,
        account_id: Uuid,
        message_id: Uuid,
        now: OffsetDateTime,
    ) -> Self {
        Self {
            pool,
            queue,
            entitlements,
            account_id,
            message_id,
            now,
        }
    }

    async fn do_request(
        &self,
        base_fen: &str,
        moves: &[String],
        wait: Duration,
    ) -> anyhow::Result<EngineCheck> {
        let fen = match position_after(base_fen, moves) {
            Ok(f) => f,
            Err(reason) => return Ok(unavailable(reason)),
        };
        let mut conn = self.pool.acquire().await?;
        if let Some(src) = interactive::reusable(
            &mut conn,
            self.account_id,
            &fen,
            INTERACTIVE_PROTOCOL,
            REUSE_WINDOW,
        )
        .await?
        {
            let row =
                interactive::insert_reused(&mut conn, &src, Some(self.message_id), base_fen, moves)
                    .await?;
            return Ok(done(&row));
        }
        let plan = self.entitlements.plan(self.account_id).await;
        let new = NewEval {
            account_id: self.account_id,
            message_id: Some(self.message_id),
            protocol: INTERACTIVE_PROTOCOL.to_owned(),
            base_fen: base_fen.to_owned(),
            moves: moves.to_vec(),
            fen,
        };
        let row = match interactive::reserve_eval(&mut conn, &new, engine_quotas(plan), self.now)
            .await?
        {
            Ok(row) => row,
            Err((quota, resets_at)) => {
                return Ok(EngineCheck::Limit {
                    window: quota.window,
                    resets_at,
                });
            }
        };
        drop(conn);
        // From here until a result is returned, a dropped future refunds the check.
        let guard = RefundOnDrop::new(self.pool.clone(), self.account_id, row.id);
        if let Err(e) = enqueue_interactive(&self.queue, row.id, self.account_id).await {
            let mut conn = self.pool.acquire().await?;
            interactive::fail_eval(&mut conn, row.id, "enqueue failed", true).await?;
            guard.disarm();
            return Err(anyhow::Error::new(e).context("enqueue interactive-analyse"));
        }
        self.poll(row.id, wait, guard).await
    }

    /// Polls the row until it finishes or `wait` passes. `guard` is disarmed on every return
    /// with a result; an error or a dropped future refunds the check.
    async fn poll(
        &self,
        id: Uuid,
        wait: Duration,
        guard: RefundOnDrop,
    ) -> anyhow::Result<EngineCheck> {
        let deadline = tokio::time::Instant::now() + wait;
        loop {
            let row = {
                let mut conn = self.pool.acquire().await?;
                interactive::get_account_eval(&mut conn, self.account_id, id).await?
            };
            let Some(row) = row else {
                guard.disarm();
                return Ok(unavailable("unknown eval"));
            };
            match row.status {
                EvalStatus::Done => {
                    guard.disarm();
                    return Ok(done(&row));
                }
                EvalStatus::Failed => {
                    guard.disarm();
                    return Ok(unavailable(row.error.unwrap_or_else(|| "failed".into())));
                }
                EvalStatus::Pending => {}
            }
            let now = tokio::time::Instant::now();
            if now >= deadline {
                guard.disarm();
                return Ok(EngineCheck::Pending {
                    eval_id: eval_id(id),
                });
            }
            tokio::time::sleep(POLL_EVERY.min(deadline - now)).await;
        }
    }
}

fn done(row: &EvalRow) -> EngineCheck {
    EngineCheck::Done {
        eval_id: eval_id(row.id),
        evidence: engine_evidence_from_row(row),
    }
}

impl InteractiveEngine for DbInteractiveEngine {
    fn request<'a>(
        &'a self,
        base_fen: &'a str,
        moves: &'a [String],
        wait: Duration,
    ) -> BoxFuture<'a, anyhow::Result<EngineCheck>> {
        Box::pin(self.do_request(base_fen, moves, wait))
    }

    fn wait<'a>(
        &'a self,
        eval_id: &'a str,
        wait: Duration,
    ) -> BoxFuture<'a, anyhow::Result<EngineCheck>> {
        Box::pin(async move {
            let Ok(id) = Uuid::try_parse(eval_id) else {
                return Ok(unavailable("invalid eval id"));
            };
            let guard = RefundOnDrop::new(self.pool.clone(), self.account_id, id);
            self.poll(id, wait, guard).await
        })
    }

    fn abandon<'a>(&'a self, eval_id: &'a str) -> BoxFuture<'a, anyhow::Result<()>> {
        Box::pin(async move {
            let Ok(id) = Uuid::try_parse(eval_id) else {
                return Ok(());
            };
            let mut conn = self.pool.acquire().await?;
            interactive::abandon_eval(&mut conn, self.account_id, id).await?;
            Ok(())
        })
    }
}

/// Refunds one check when dropped armed (a cancelled turn, or an error mid-poll). The refund
/// runs on a spawned task, because `Drop` cannot await.
struct RefundOnDrop {
    pool: PgPool,
    account_id: Uuid,
    id: Uuid,
    armed: bool,
}

impl RefundOnDrop {
    fn new(pool: PgPool, account_id: Uuid, id: Uuid) -> Self {
        Self {
            pool,
            account_id,
            id,
            armed: true,
        }
    }

    fn disarm(mut self) {
        self.armed = false;
    }
}

impl Drop for RefundOnDrop {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let Ok(rt) = tokio::runtime::Handle::try_current() else {
            tracing::warn!(eval_id = %self.id, "no runtime to refund a dropped engine check");
            return;
        };
        let (pool, account_id, id) = (self.pool.clone(), self.account_id, self.id);
        rt.spawn(async move {
            let res = async {
                let mut conn = pool.acquire().await?;
                interactive::abandon_eval(&mut conn, account_id, id).await
            }
            .await;
            if let Err(e) = res {
                tracing::warn!(eval_id = %id, error = %e, "refund of a dropped engine check failed");
            }
        });
    }
}

// --- Evidence ---

/// A side-to-move score from White's point of view.
fn white_pov(score: EngineScore, side: Color) -> Score {
    let flip = side == Color::Black;
    match score {
        EngineScore::Cp(n) => Score::Cp(if flip { n.saturating_neg() } else { n }),
        EngineScore::Mate(n) => Score::Mate(if flip { n.saturating_neg() } else { n }),
    }
}

/// The stored `lines` of an analysis of `fen`: best first, at most [`INTERACTIVE_MULTIPV`],
/// scores from White's point of view, with SAN.
fn lines_json(fen: &str, result: &AnalysisResult) -> Value {
    let side = Position::from_fen(fen).map_or(Color::White, |p| p.side_to_move());
    let mut lines: Vec<_> = result.lines.iter().collect();
    lines.sort_by_key(|l| l.multipv);
    let out: Vec<Value> = lines
        .into_iter()
        .take(INTERACTIVE_MULTIPV as usize)
        .map(|l| {
            json!({
                "multipv": l.multipv,
                "score": white_pov(l.score, side),
                "pv": l.pv,
                "san": pv_to_san(fen, &l.pv),
                "depth": l.depth,
                "nodes": l.nodes,
            })
        })
        .collect();
    Value::Array(out)
}

/// The `{{eng:}}` evidence of a stored eval: the tried moves in SAN and its lines (a pending or
/// failed row has none).
pub fn engine_evidence_from_row(row: &EvalRow) -> EngineEvidence {
    let side_to_move = Position::from_fen(&row.fen).map_or(Color::White, |p| p.side_to_move());
    let lines = row
        .lines
        .as_ref()
        .and_then(Value::as_array)
        .map(|ls| {
            ls.iter()
                .filter_map(|l| {
                    let score: Score = serde_json::from_value(l.get("score")?.clone()).ok()?;
                    let san: Vec<String> = serde_json::from_value(l.get("san")?.clone()).ok()?;
                    Some(EngineLine {
                        score,
                        side_to_move,
                        san,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    EngineEvidence {
        tried_san: pv_to_san(&row.base_fen, &row.moves),
        lines,
    }
}

// --- The job ---

/// R1 `Handler` for `interactive-analyse`: analyses one pending eval on the engine pool and
/// stores its lines in `interactive_evals` (never `position_evals`).
pub struct InteractiveAnalyseHandler {
    pub pool: PgPool,
    pub analyser: Arc<dyn Analyser>,
}

fn retryable(what: &str, e: impl std::fmt::Display) -> JobError {
    JobError::Retryable {
        message: format!("{what}: {e}"),
        code: None,
    }
}

impl InteractiveAnalyseHandler {
    async fn run(&self, job: Job, ctx: JobContext) -> Result<Value, JobError> {
        let payload: InteractiveAnalysePayload =
            serde_json::from_value(job.payload).map_err(|e| JobError::Unrecoverable {
                message: format!("bad interactive-analyse payload: {e}"),
                code: None,
            })?;
        let id = payload.eval_id;
        let row = {
            let mut conn = self.pool.acquire().await.map_err(|e| retryable("db", e))?;
            interactive::get_eval(&mut conn, id)
                .await
                .map_err(|e| retryable("load eval", e))?
        };
        let Some(row) = row else {
            return Ok(json!({"skipped": "missing"}));
        };
        if row.status != EvalStatus::Pending {
            return Ok(json!({"skipped": row.status.as_str()}));
        }
        let result = tokio::select! {
            r = self.analyser.analyse(&row.fen, options()) => r,
            () = ctx.cancel.cancelled() => {
                return Err(JobError::Release {
                    delay: POOL_CLOSED_DELAY,
                    reason: "cancelled".into(),
                });
            }
        };
        let mut conn = self.pool.acquire().await.map_err(|e| retryable("db", e))?;
        match result {
            Ok(a) => {
                let lines = lines_json(&row.fen, &a);
                interactive::complete_eval(&mut conn, id, &lines)
                    .await
                    .map_err(|e| retryable("store lines", e))?;
                Ok(json!({"lines": lines.as_array().map_or(0, Vec::len)}))
            }
            Err(EngineError::PoolClosed) => Err(JobError::Release {
                delay: POOL_CLOSED_DELAY,
                reason: "engine pool closed".into(),
            }),
            Err(e) => {
                let msg = e.to_string();
                interactive::fail_eval(&mut conn, id, &msg, true)
                    .await
                    .map_err(|e| retryable("fail eval", e))?;
                Ok(json!({"failed": msg}))
            }
        }
    }
}

impl Handler for InteractiveAnalyseHandler {
    fn kind(&self) -> &'static str {
        KIND_INTERACTIVE_ANALYSE
    }

    fn handle(&self, job: Job, ctx: JobContext) -> BoxFuture<'_, Result<Value, JobError>> {
        Box::pin(self.run(job, ctx))
    }

    /// A dead-lettered check is failed and refunded.
    fn on_dead<'a>(&'a self, job: &'a Job, reason: &'a str) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            let Ok(p) = serde_json::from_value::<InteractiveAnalysePayload>(job.payload.clone())
            else {
                return;
            };
            let res = async {
                let mut conn = self.pool.acquire().await?;
                interactive::fail_eval(&mut conn, p.eval_id, reason, true).await
            }
            .await;
            if let Err(e) = res {
                tracing::warn!(eval_id = %p.eval_id, error = %e, "failing a dead engine check failed");
            }
        })
    }
}

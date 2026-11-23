//! The `coach-chat` job (Task 25): answers one pending assistant message of the coach chat.
//!
//! 1. **Load** the assistant message and its conversation. A missing one (the conversation was
//!    deleted) or a finished one returns `Ok`.
//! 2. **Turn lock.** One turn per conversation at a time: a session advisory lock
//!    ([`TURN_LOCK_CLASS`], the conversation) on a dedicated connection that is closed, never
//!    pooled, when the handler ends. A second handler (a redelivered job, or another pending
//!    answer of the same thread) gets `Release` and runs later. Under the lock, an answer waits
//!    for any earlier unfinished answer of the thread, so turns run in `seq` order.
//! 3. **Allowance, reserved atomically** (T23 review m7). Under a transaction lock per account of
//!    the budget scope ([`CHAT_QUOTA_LOCK_CLASS`]), `check_chat_allowance` counts the scope's
//!    assistant answers in each window (`streaming`, or `done` by the AI) and, if allowed, moves
//!    the message to `streaming` in the same transaction. Two parallel questions can never both
//!    take the last message of a window. A denial fails the message with its reason and emits
//!    `chat.message.failed`, with no AI call.
//! 4. **No data:** without a coaching input, [`CHAT_NO_DATA_TEXT`] is stored as done (it does not
//!    count against the allowance). **No AI:** `failed "unavailable"`.
//! 5. **History.** Fewer than `replay_turns` answered turns since `summary_through_seq`: the
//!    stored `api_messages`, verbatim (or, after an earlier rollover, the stored summary plus the
//!    recent turns). Otherwise `summarise_thread` rolls the turns since the last rollover and the
//!    old summary into a new summary, and the turn starts a new SDK session.
//! 6. **Resume** (T23 I1): the SDK session is resumed only when the previous assistant turn is
//!    stored `session_in_sync = true` and the history did not roll over; otherwise the full
//!    Postgres history goes with `resume: false` (the sidecar replaces the stale session). The
//!    first tool-call index is the conversation's stored `next_call_index` (T23 I2).
//! 7. **Anchors** are resolved through the account-scoped data source; a foreign or unknown one
//!    is ignored with a `warn` (its kind only, never its ids).
//! 8. **Run** `run_chat_turn` within [`CHAT_TURN_TIMEOUT`](crate::jobs::kinds::CHAT_TURN_TIMEOUT); chunks are emitted in short
//!    transactions.
//! 9. **Finish** in one transaction on the lock connection, after re-checking (`FOR SHARE`) that
//!    the conversation still exists: the answer, `session_in_sync`, `next_call_index`, the engine
//!    evals' message link, metering and `chat.message.ready`. If the conversation was deleted
//!    mid-turn, the paid calls are still metered and the turn's orphaned session entries are
//!    deleted.
//! 10. **Title** after the first answered turn, from `summarise_thread` or the fallback title.
//! 11. **Errors.** `Unavailable` (or the turn timeout) puts the message back to `pending` and
//!     releases the job for [`CHAT_UNAVAILABLE_RELEASE`] on the turn's first AI attempt
//!     (`coach_messages.turn_attempts`, because a release does not use a queue attempt); on the
//!     last it fails with `unavailable`. `Budget` fails with `budget`.

use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use futures::future::BoxFuture;
use queue::queue::Queue;
use queue::runner::{Handler, JobContext};
use queue::types::{Job, JobError};
use serde_json::{Value, json};
use sqlx::pool::PoolConnection;
use sqlx::{Connection, PgConnection, PgPool, Postgres};
use time::OffsetDateTime;
use uuid::Uuid;

use super::anchor::ChatAnchor;
use super::context::anchor_details;
use super::data_source::CoachDataSource;
use super::db_source::{ChatScope, DbCoachDataSource, latest_coaching_input};
use super::engine::{
    ChatChunkOut, ChatHistory, ChatOutcomeKind, ChatTurnArgs, ChatTurnOutcome, TurnSink,
    build_chat_request_messages, resume_next_turn, run_chat_turn_into,
};
use super::execute::OwnedToolDeps;
use super::interactive::DbInteractiveEngine;
use super::limits::{CHAT_LIMITS, CHAT_NO_DATA_TEXT, check_chat_allowance_with};
use super::summary::{fallback_title, summarise_thread};
use super::threads::AccountAccess;
use crate::client::model::{AiModel, Models};
use crate::client::port::{AiClient, AiError, Message, ModelUsage, UnavailableReason};
use crate::db::chat::{self, ChatRole, ChatStatus, MessageFinish, MessageRow};
use crate::db::usage::AiFeature;
use crate::entitlements::{Entitlements, QuotaWindow, window_start};
use crate::events::{
    CHAT_CHUNK, CHAT_MESSAGE_FAILED, CHAT_MESSAGE_READY, ChatChunk, ChatFailed, ChatReady,
    emit_for_user,
};
use crate::generate::section::CallRecord;
use crate::input::types::{CoachingInput, TimeClass};
use crate::jobs::budget::{BudgetConfig, PricingTable, budget_guard, meter_keyed};
use crate::jobs::kinds::{CHAT_MAX_ATTEMPTS, CoachChatPayload, KIND_COACH_CHAT};
use crate::output::codes::Citation;
use crate::output::disclosure::make_disclosure;
use crate::prompts::CHAT_PROMPT_VERSION;
use crate::segments::Segment;
use crate::validator::evidence::build_evidence;

/// The longest one sidecar call of a turn may take (its `timeoutMs`): a turn makes up to three
/// sequential calls (main, follow-up, repair), and all of them must fit in [`CHAT_TURN_TIMEOUT`](crate::jobs::kinds::CHAT_TURN_TIMEOUT)
/// so `ai-reconcile` never fails a live turn (T23 review m5). The runtime's
/// `SidecarConfig::chat_timeout` must not exceed it.
pub const CHAT_CALL_TIMEOUT: Duration = Duration::from_secs(180);

/// The bound on the rollover summary call inside a turn (review I2). With the three chat calls it
/// fits in [`CHAT_TURN_TIMEOUT`](crate::jobs::kinds::CHAT_TURN_TIMEOUT); a summary that takes longer is skipped (the old summary is kept).
pub const SUMMARY_CALL_TIMEOUT: Duration = Duration::from_secs(60);

/// How far a redelivered turn (its worker died mid-turn) skips the conversation's call-index
/// counter: the lost attempt's tool results may still sit in an SDK session or a log, and their
/// `tools.<k>` paths must never be reused.
pub const CALL_INDEX_RETRY_GAP: u32 = 1000;

/// The release after an unavailable first attempt.
pub const CHAT_UNAVAILABLE_RELEASE: Duration = Duration::from_secs(60);

/// The release when another turn of the conversation is running.
pub const CHAT_BUSY_RELEASE: Duration = Duration::from_secs(2);

/// The advisory-lock class of the per-conversation turn lock (session level, two-int form;
/// the sidecar's own turn lock uses class 2026112110).
pub const TURN_LOCK_CLASS: i32 = 30_003;

/// The advisory-lock class of the chat-allowance reservation (transaction level, per account).
pub const CHAT_QUOTA_LOCK_CLASS: i32 = 30_004;

const UNLOCK_TIMEOUT: Duration = Duration::from_secs(5);
const LOCK_KEEPALIVES_SQL: &str = "SELECT set_config('tcp_keepalives_idle', '60', false), \
    set_config('tcp_keepalives_interval', '10', false), \
    set_config('tcp_keepalives_count', '3', false)";

const REASON_UNAVAILABLE: &str = "unavailable";
const REASON_BUDGET: &str = "budget";
const REASON_NOT_LINKED: &str = "not_linked";

/// What the `coach-chat` handler needs. `pricing` prices the metered calls: the same
/// [`PricingTable`] the report job uses (`jobs::budget`, Task 18).
#[derive(Clone)]
pub struct ChatDeps {
    pub pool: PgPool,
    pub queue: Queue,
    pub ai: Option<Arc<dyn AiClient>>,
    pub entitlements: Arc<dyn Entitlements>,
    /// Whether the thread's owner is still linked to its account (checked at the reservation).
    pub access: Arc<dyn AccountAccess>,
    pub budgets: BudgetConfig,
    pub pricing: Arc<PricingTable>,
    pub now: Arc<dyn Fn() -> OffsetDateTime + Send + Sync>,
    /// The whole turn's bound: [`CHAT_TURN_TIMEOUT`](crate::jobs::kinds::CHAT_TURN_TIMEOUT) in production.
    pub turn_timeout: Duration,
}

/// R1 `Handler` for `coach-chat`.
pub struct CoachChatHandler(pub ChatDeps);

fn retryable(what: &str, e: impl std::fmt::Display) -> JobError {
    JobError::Retryable {
        message: format!("{what}: {e}"),
        code: None,
    }
}

/// The metering key prefix of one AI attempt of a turn: a call is keyed by message, attempt and
/// call number, so re-metering the same calls (a failed finish, a retried write) adds nothing,
/// while a retry's own calls (genuinely paid again) get new keys.
fn turn_key(message_id: Uuid, attempt: i16) -> String {
    format!("chat:{message_id}:{attempt}")
}

fn skipped(why: &str) -> Value {
    json!({ "skipped": why })
}

// --- The turn lock ---

/// The conversation's session advisory lock on a dedicated connection. The connection is
/// closed (never returned to the pool) when dropped, so the lock never leaks into the pool and
/// a dropped handler still releases it.
struct TurnLock {
    conn: PoolConnection<Postgres>,
    conversation_id: Uuid,
}

impl TurnLock {
    async fn acquire(
        pool: &PgPool,
        conversation_id: Uuid,
    ) -> Result<Option<TurnLock>, sqlx::Error> {
        let mut conn = pool.acquire().await?;
        conn.close_on_drop();
        if let Err(e) = sqlx::query(LOCK_KEEPALIVES_SQL).execute(&mut *conn).await {
            tracing::warn!(error = %e, "setting chat lock keepalives failed");
        }
        let locked: bool =
            sqlx::query_scalar("SELECT pg_try_advisory_lock($1, hashtext($2::text))")
                .bind(TURN_LOCK_CLASS)
                .bind(conversation_id)
                .fetch_one(&mut *conn)
                .await?;
        Ok(locked.then_some(TurnLock {
            conn,
            conversation_id,
        }))
    }

    /// Unlocks at once (bounded); closing the connection on drop releases it anyway.
    async fn release(mut self) {
        let unlock =
            sqlx::query_scalar::<_, bool>("SELECT pg_advisory_unlock($1, hashtext($2::text))")
                .bind(TURN_LOCK_CLASS)
                .bind(self.conversation_id)
                .fetch_one(&mut *self.conn);
        match tokio::time::timeout(UNLOCK_TIMEOUT, unlock).await {
            Ok(Ok(_)) => {}
            Ok(Err(e)) => tracing::warn!(error = %e, "unlocking a chat turn failed"),
            Err(_) => tracing::warn!("unlocking a chat turn timed out"),
        }
    }
}

// --- Small DB helpers ---

/// The message's `turn_attempts` (0 when it is gone), for the `attempt` of its chat events.
pub(crate) async fn turn_attempt(conn: &mut PgConnection, message_id: Uuid) -> sqlx::Result<i16> {
    Ok(
        sqlx::query_scalar("SELECT turn_attempts FROM coach_messages WHERE id = $1")
            .bind(message_id)
            .fetch_optional(conn)
            .await?
            .unwrap_or(0),
    )
}

/// Fails an unfinished answer and emits `chat.message.failed`, in one transaction. `false` when
/// the message had already finished (or is gone).
async fn fail_and_emit(
    conn: &mut PgConnection,
    (account_id, user_id): (Uuid, Uuid),
    message_id: Uuid,
    reason: &str,
) -> Result<bool, JobError> {
    let mut tx = conn.begin().await.map_err(|e| retryable("db", e))?;
    let failed = chat::fail_message(&mut tx, message_id, reason)
        .await
        .map_err(|e| retryable("fail message", e))?;
    if failed {
        let attempt = turn_attempt(&mut tx, message_id)
            .await
            .map_err(|e| retryable("attempt", e))?;
        emit_for_user(
            &mut tx,
            account_id,
            user_id,
            CHAT_MESSAGE_FAILED,
            &ChatFailed {
                message_id,
                attempt,
                reason: reason.to_owned(),
            },
        )
        .await
        .map_err(|e| retryable("emit", e))?;
    }
    tx.commit().await.map_err(|e| retryable("commit", e))?;
    Ok(failed)
}

/// `(session_in_sync, turn_attempts)` of a message.
async fn turn_state(conn: &mut PgConnection, id: Uuid) -> Result<(Option<bool>, i16), sqlx::Error> {
    sqlx::query_as("SELECT session_in_sync, turn_attempts FROM coach_messages WHERE id = $1")
        .bind(id)
        .fetch_one(conn)
        .await
}

/// The scope's assistant answers per window (excluding `except`): `streaming` ones, and `done`
/// ones the AI wrote (fixed no-data copy has no disclosure and is free).
async fn used_per_window(
    conn: &mut PgConnection,
    scope: &[Uuid],
    except: Uuid,
    now: OffsetDateTime,
) -> Result<[u32; 3], sqlx::Error> {
    let (day, week, month) = (
        window_start(QuotaWindow::Day, now),
        window_start(QuotaWindow::Week, now),
        window_start(QuotaWindow::Month, now),
    );
    let (d, w, m): (i64, i64, i64) = sqlx::query_as(
        "SELECT count(*) FILTER (WHERE m.created_at >= $2), \
         count(*) FILTER (WHERE m.created_at >= $3), \
         count(*) FILTER (WHERE m.created_at >= $4) \
         FROM coach_messages m JOIN coach_conversations c ON c.id = m.conversation_id \
         WHERE c.account_id = ANY($1) AND m.role = 'assistant' AND m.id <> $5 \
         AND (m.status = 'streaming' OR (m.status = 'done' AND m.ai_disclosure IS NOT NULL)) \
         AND m.created_at >= least($2, $3, $4)",
    )
    .bind(scope)
    .bind(day)
    .bind(week)
    .bind(month)
    .bind(except)
    .fetch_one(conn)
    .await?;
    let c = |n: i64| u32::try_from(n).unwrap_or(u32::MAX);
    Ok([c(d), c(w), c(m)])
}

fn citations_of(segments: &[Segment]) -> Vec<Citation> {
    let mut out: Vec<Citation> = Vec::new();
    for s in segments {
        let (Segment::Move { game_id, ply, .. }
        | Segment::Best { game_id, ply, .. }
        | Segment::Pv { game_id, ply, .. }) = s
        else {
            continue;
        };
        let c = Citation {
            game_id: game_id.clone(),
            ply: *ply,
        };
        if !out.contains(&c) {
            out.push(c);
        }
    }
    out
}

fn anchor_kind(a: &ChatAnchor) -> &'static str {
    match a {
        ChatAnchor::ReportSection { .. } => "report_section",
        ChatAnchor::Weakness { .. } => "weakness",
        ChatAnchor::Position { .. } => "position",
        ChatAnchor::Move { .. } => "move",
        ChatAnchor::OpeningLine { .. } => "opening_line",
        ChatAnchor::Game { .. } => "game",
    }
}

fn parse_anchor(v: Option<&Value>) -> Option<ChatAnchor> {
    let v = v?;
    match serde_json::from_value(v.clone()) {
        Ok(a) => Some(a),
        Err(_) => {
            tracing::warn!("a stored chat anchor does not parse; ignored");
            None
        }
    }
}

fn unavailable() -> AiError {
    AiError::Unavailable {
        reason: UnavailableReason::Server,
        retry_after: None,
    }
}

/// Why a turn did not produce an answer.
enum TurnError {
    Ai(AiError),
    Db(anyhow::Error),
}

impl From<AiError> for TurnError {
    fn from(e: AiError) -> Self {
        TurnError::Ai(e)
    }
}

impl From<sqlx::Error> for TurnError {
    fn from(e: sqlx::Error) -> Self {
        TurnError::Db(e.into())
    }
}

/// One finished prior turn: `(assistant seq, question, answer text, api messages)`.
struct PriorTurn {
    seq: i32,
    question: String,
    answer: String,
    api: Vec<Message>,
}

/// Everything loaded under the turn lock.
struct Loaded {
    account_id: Uuid,
    conversation: chat::ConversationRow,
    message: MessageRow,
    question: String,
    question_anchor: Option<ChatAnchor>,
    prior: Vec<PriorTurn>,
    /// The previous assistant row of the thread (any status): `(in sync, session usage)`.
    previous: Option<(bool, Option<Value>)>,
    time_class: TimeClass,
    /// The first tool-call index of this attempt (set by the reservation).
    next_call_index: i32,
    /// This AI attempt of the turn (1 = the first; set by the reservation).
    attempt: i16,
}

impl CoachChatHandler {
    async fn run(&self, job: Job) -> Result<Value, JobError> {
        let d = &self.0;
        let payload: CoachChatPayload =
            serde_json::from_value(job.payload).map_err(|e| JobError::Unrecoverable {
                message: format!("bad coach-chat payload: {e}"),
                code: None,
            })?;
        let id = payload.message_id;
        let (msg, conv) = {
            let mut conn = d.pool.acquire().await.map_err(|e| retryable("db", e))?;
            let Some(msg) = chat::get_message(&mut conn, id)
                .await
                .map_err(|e| retryable("load message", e))?
            else {
                return Ok(skipped("missing"));
            };
            let Some(conv) = chat::get_conversation(&mut conn, msg.conversation_id)
                .await
                .map_err(|e| retryable("load conversation", e))?
            else {
                return Ok(skipped("missing"));
            };
            (msg, conv)
        };
        if msg.role != ChatRole::Assistant
            || matches!(msg.status, ChatStatus::Done | ChatStatus::Failed)
        {
            return Ok(skipped("finished"));
        }
        let Some(mut lock) = TurnLock::acquire(&d.pool, conv.id)
            .await
            .map_err(|e| retryable("turn lock", e))?
        else {
            return Err(JobError::Release {
                delay: CHAT_BUSY_RELEASE,
                reason: "conversation_busy".into(),
            });
        };
        let r = self.locked(&mut lock, id, conv.account_id).await;
        lock.release().await;
        r
    }

    /// Loads the thread under the lock; `Err(Ok(v))` is an early finish.
    async fn load(
        &self,
        conn: &mut PgConnection,
        id: Uuid,
    ) -> Result<Result<Loaded, Value>, JobError> {
        let Some(message) = chat::get_message(conn, id)
            .await
            .map_err(|e| retryable("load message", e))?
        else {
            return Ok(Err(skipped("missing")));
        };
        if matches!(message.status, ChatStatus::Done | ChatStatus::Failed) {
            return Ok(Err(skipped("finished")));
        }
        let Some(conversation) = chat::get_conversation(conn, message.conversation_id)
            .await
            .map_err(|e| retryable("load conversation", e))?
        else {
            return Ok(Err(skipped("missing")));
        };
        let rows = chat::list_messages(conn, conversation.id)
            .await
            .map_err(|e| retryable("load thread", e))?;
        if rows.iter().any(|m| {
            m.role == ChatRole::Assistant
                && m.seq < message.seq
                && matches!(m.status, ChatStatus::Pending | ChatStatus::Streaming)
        }) {
            // An earlier question of this thread is answered first.
            return Err(JobError::Release {
                delay: CHAT_BUSY_RELEASE,
                reason: "earlier_answer_pending".into(),
            });
        }
        let user = rows
            .iter()
            .find(|m| m.seq == message.seq - 1 && m.role == ChatRole::User);
        let (question, question_anchor) = match user {
            Some(u) => (
                u.text.clone().unwrap_or_default(),
                parse_anchor(u.anchor.as_ref()),
            ),
            None => (String::new(), None),
        };
        let mut prior = Vec::new();
        for a in rows
            .iter()
            .filter(|m| m.role == ChatRole::Assistant && m.seq < message.seq)
            .filter(|m| m.status == ChatStatus::Done)
        {
            let q = rows
                .iter()
                .find(|m| m.seq == a.seq - 1 && m.role == ChatRole::User)
                .and_then(|m| m.text.clone())
                .unwrap_or_default();
            let api = a
                .api_messages
                .as_ref()
                .and_then(|v| serde_json::from_value::<Vec<Message>>(v.clone()).ok())
                .unwrap_or_default();
            prior.push(PriorTurn {
                seq: a.seq,
                question: q,
                answer: a.text.clone().unwrap_or_default(),
                api,
            });
        }
        let previous = match rows
            .iter()
            .filter(|m| m.role == ChatRole::Assistant && m.seq < message.seq)
            .max_by_key(|m| m.seq)
        {
            Some(p) => {
                let (in_sync, _) = turn_state(conn, p.id)
                    .await
                    .map_err(|e| retryable("load turn state", e))?;
                Some((
                    p.status == ChatStatus::Done && in_sync == Some(true),
                    p.session_model_usage.clone(),
                ))
            }
            None => None,
        };
        let time_class =
            TimeClass::from_str(&conversation.time_class).map_err(|_| JobError::Unrecoverable {
                message: "conversation time class does not parse".into(),
                code: None,
            })?;
        let next_call_index: i32 =
            sqlx::query_scalar("SELECT next_call_index FROM coach_conversations WHERE id = $1")
                .bind(conversation.id)
                .fetch_one(&mut *conn)
                .await
                .map_err(|e| retryable("load call index", e))?;
        Ok(Ok(Loaded {
            next_call_index,
            attempt: 0,
            account_id: conversation.account_id,
            conversation,
            message,
            question,
            question_anchor,
            prior,
            previous,
            time_class,
        }))
    }

    async fn locked(
        &self,
        lock: &mut TurnLock,
        id: Uuid,
        account_hint: Uuid,
    ) -> Result<Value, JobError> {
        let d = &self.0;
        let now = (d.now)();
        let conn: &mut PgConnection = &mut lock.conn;
        let mut t = match self.load(conn, id).await? {
            Ok(t) => t,
            Err(v) => return Ok(v),
        };
        debug_assert_eq!(t.account_id, account_hint);
        let account_id = t.account_id;
        let plan = d.entitlements.plan(account_id).await;
        let quotas = d.entitlements.chat_quotas(plan);
        let mut scope = d.entitlements.budget_scope(account_id).await;
        if !scope.contains(&account_id) {
            scope.push(account_id);
        }
        scope.sort();
        scope.dedup();
        let who = (account_id, t.conversation.user_id);

        // --- The poster must still be linked to the account (ruling 3) ---
        let linked = d
            .access
            .is_linked(t.conversation.user_id, account_id)
            .await
            .map_err(|e| retryable("account access", format!("{e:#}")))?;
        if !linked {
            fail_and_emit(conn, who, id, REASON_NOT_LINKED).await?;
            return Ok(json!({ "failed": REASON_NOT_LINKED }));
        }

        // --- Allowance, reserved atomically with the move to `streaming` ---
        let mut tx = conn.begin().await.map_err(|e| retryable("db", e))?;
        if t.message.status == ChatStatus::Pending {
            for a in &scope {
                sqlx::query("SELECT pg_advisory_xact_lock($1, hashtext($2::text))")
                    .bind(CHAT_QUOTA_LOCK_CLASS)
                    .bind(a)
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| retryable("quota lock", e))?;
            }
            // New statements after the locks, so they see a reservation committed meanwhile.
            let used = used_per_window(&mut tx, &scope, id, now)
                .await
                .map_err(|e| retryable("count answers", e))?;
            let turns = u32::try_from(t.prior.len()).unwrap_or(u32::MAX);
            let allowed = check_chat_allowance_with(
                &quotas,
                |w| match w {
                    QuotaWindow::Day => used[0],
                    QuotaWindow::Week => used[1],
                    QuotaWindow::Month => used[2],
                },
                turns,
                t.question.chars().count(),
                now,
            );
            if let Err((reason, _)) = allowed {
                tx.rollback().await.map_err(|e| retryable("rollback", e))?;
                let reason = serde_json::to_value(reason)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_owned))
                    .unwrap_or_else(|| "denied".to_owned());
                fail_and_emit(conn, who, id, &reason).await?;
                return Ok(json!({ "denied": reason }));
            }
        }
        let attempts: i16 = sqlx::query_scalar(
            "UPDATE coach_messages SET status = 'streaming', \
             streaming_at = CASE WHEN status = 'pending' THEN now() ELSE streaming_at END, \
             turn_attempts = turn_attempts + 1 \
             WHERE id = $1 AND status IN ('pending', 'streaming') RETURNING turn_attempts",
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| retryable("reserve", e))?
        .unwrap_or(i16::MAX);
        // A message already `streaming` lost its worker mid-turn: nobody recorded how far its
        // call indexes went, so skip ahead (review I1). Otherwise the stored counter is exact
        // (a failed attempt stores its own before it is retried).
        let skip = if t.message.status == ChatStatus::Streaming {
            i32::try_from(CALL_INDEX_RETRY_GAP).unwrap_or(i32::MAX)
        } else {
            0
        };
        let first: i32 = sqlx::query_scalar(
            "UPDATE coach_conversations SET next_call_index = next_call_index + $2 \
             WHERE id = $1 RETURNING next_call_index",
        )
        .bind(t.conversation.id)
        .bind(skip)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| retryable("call index", e))?;
        tx.commit().await.map_err(|e| retryable("commit", e))?;
        t.attempt = attempts;
        t.next_call_index = first;

        // --- No data, no AI ---
        let input = latest_coaching_input(&d.pool, account_id, t.time_class)
            .await
            .map_err(|e| retryable("load coaching input", e))?;
        let Some(stored) = input else {
            return self.no_data(conn, who, id).await;
        };
        // Which input the answer is written from (ADR 0086), stored with it.
        let used = (
            stored.snapshot_id(),
            crate::generate::context::input_hash(&stored.input),
        );
        let input = stored.input;
        let Some(ai) = d.ai.clone() else {
            fail_and_emit(conn, who, id, REASON_UNAVAILABLE).await?;
            return Ok(json!({ "failed": REASON_UNAVAILABLE }));
        };

        // --- The turn ---
        let guard = budget_guard(d.pool.clone(), scope.clone(), plan, d.budgets, now);
        // The sink lives outside the timed future, so a timeout keeps the paid calls and the
        // call-index counter (review I2).
        let sink = TurnSink::default();
        let turn = tokio::time::timeout(
            d.turn_timeout,
            self.answer(ai.as_ref(), &guard, &t, Arc::new(input), now, &sink),
        )
        .await
        .unwrap_or_else(|_| Err(TurnError::Ai(unavailable())));
        let key = turn_key(id, attempts);
        let outcome = match turn {
            Ok(o) => o,
            Err(e) => {
                // Every paid call is metered, also when the turn failed or timed out, and the
                // next attempt starts after this one's call indexes (review I1).
                self.meter_now(account_id, id, &sink.calls(), &key, Models::CHAT)
                    .await;
                if let Some(n) = sink.next_call_index() {
                    sqlx::query(
                        "UPDATE coach_conversations SET next_call_index = \
                         greatest(next_call_index, $2) WHERE id = $1",
                    )
                    .bind(t.conversation.id)
                    .bind(i32::try_from(n).unwrap_or(i32::MAX))
                    .execute(&mut *conn)
                    .await
                    .map_err(|e| retryable("call index", e))?;
                }
                return match e {
                    TurnError::Db(e) => Err(retryable("chat turn", format!("{e:#}"))),
                    TurnError::Ai(e) => self.ai_failed(conn, who, id, attempts, e).await,
                };
            }
        };

        // --- Finish ---
        let finished = match self.finish(conn, &t, &outcome, &used, now).await {
            Ok(f) => f,
            Err(e) => {
                // The finish rolled back its usage rows; the calls were paid (review m1).
                self.meter_now(account_id, id, &outcome.calls, &key, Models::CHAT)
                    .await;
                return Err(e);
            }
        };
        if !finished {
            return Ok(skipped("deleted"));
        }
        if t.conversation.title.is_none() {
            self.set_title(ai.as_ref(), &guard, &t, &outcome).await;
        }
        Ok(json!({ "outcome": outcome.outcome }))
    }

    async fn no_data(
        &self,
        conn: &mut PgConnection,
        (account_id, user_id): (Uuid, Uuid),
        id: Uuid,
    ) -> Result<Value, JobError> {
        let segments = vec![Segment::Text {
            text: CHAT_NO_DATA_TEXT.to_owned(),
        }];
        let mut tx = conn.begin().await.map_err(|e| retryable("db", e))?;
        let done = chat::finish_message(
            &mut tx,
            id,
            &MessageFinish {
                text: CHAT_NO_DATA_TEXT.to_owned(),
                segments: serde_json::to_value(&segments).expect("segments serialise"),
                citations: json!([]),
                api_messages: None,
                dropped_count: 0,
                ai_disclosure: None,
                session: None,
                session_model_usage: None,
            },
        )
        .await
        .map_err(|e| retryable("finish", e))?;
        if done {
            let attempt = turn_attempt(&mut tx, id)
                .await
                .map_err(|e| retryable("attempt", e))?;
            emit_for_user(
                &mut tx,
                account_id,
                user_id,
                CHAT_MESSAGE_READY,
                &ChatReady {
                    message_id: id,
                    attempt,
                    replace: false,
                },
            )
            .await
            .map_err(|e| retryable("emit", e))?;
        }
        tx.commit().await.map_err(|e| retryable("commit", e))?;
        Ok(skipped("no_data"))
    }

    async fn ai_failed(
        &self,
        conn: &mut PgConnection,
        who: (Uuid, Uuid),
        id: Uuid,
        attempts: i16,
        e: AiError,
    ) -> Result<Value, JobError> {
        match e {
            AiError::Unavailable { .. } if i32::from(attempts) < CHAT_MAX_ATTEMPTS => {
                // Back to pending: the retry reserves the allowance again.
                sqlx::query(
                    "UPDATE coach_messages SET status = 'pending' \
                     WHERE id = $1 AND status = 'streaming'",
                )
                .bind(id)
                .execute(&mut *conn)
                .await
                .map_err(|e| retryable("release", e))?;
                Err(JobError::Release {
                    delay: CHAT_UNAVAILABLE_RELEASE,
                    reason: "ai_unavailable".into(),
                })
            }
            AiError::Budget => {
                fail_and_emit(conn, who, id, REASON_BUDGET).await?;
                Ok(json!({ "failed": REASON_BUDGET }))
            }
            other => {
                if let AiError::Auth { status } = other {
                    tracing::error!(status, "AI credentials rejected for a chat turn");
                } else if !other.is_unavailable() {
                    tracing::warn!(error = %other, "chat turn failed");
                }
                fail_and_emit(conn, who, id, REASON_UNAVAILABLE).await?;
                Ok(json!({ "failed": REASON_UNAVAILABLE }))
            }
        }
    }

    /// Meters `calls` in their own transaction (summaries, or a turn whose conversation went).
    async fn meter_now(
        &self,
        account_id: Uuid,
        id: Uuid,
        calls: &[CallRecord],
        key: &str,
        model: AiModel,
    ) {
        if calls.is_empty() {
            return;
        }
        let r = async {
            let mut tx = self.0.pool.begin().await?;
            meter_keyed(
                &mut tx,
                &self.0.pricing,
                account_id,
                AiFeature::Chat,
                calls,
                &id.to_string(),
                model,
                key,
            )
            .await?;
            tx.commit().await
        }
        .await;
        if let Err(e) = r {
            tracing::error!(error = %e, "metering chat calls failed");
        }
    }

    async fn history(
        &self,
        ai: &dyn AiClient,
        guard: &(dyn Fn() -> BoxFuture<'static, Result<(), AiError>> + Send + Sync),
        t: &Loaded,
    ) -> Result<(ChatHistory, bool), TurnError> {
        let through = t.conversation.summary_through_seq;
        let since: Vec<&PriorTurn> = t.prior.iter().filter(|p| p.seq > through).collect();
        let replay = CHAT_LIMITS.replay_turns as usize;
        let recent = || -> Vec<(String, String)> {
            let start = t.prior.len().saturating_sub(replay);
            t.prior[start..]
                .iter()
                .map(|p| (p.question.clone(), p.answer.clone()))
                .collect()
        };
        if since.len() < replay {
            return Ok(match &t.conversation.summary {
                None => (
                    ChatHistory::Verbatim(t.prior.iter().flat_map(|p| p.api.clone()).collect()),
                    false,
                ),
                Some(s) => (
                    ChatHistory::Rollover {
                        summary: Some(s.clone()),
                        recent: recent(),
                    },
                    false,
                ),
            });
        }
        let mut transcript: Vec<(String, String)> = Vec::new();
        if let Some(old) = &t.conversation.summary {
            transcript.push((
                "Summary of the earlier conversation".to_owned(),
                old.clone(),
            ));
        }
        transcript.extend(since.iter().map(|p| (p.question.clone(), p.answer.clone())));
        let s = match tokio::time::timeout(
            SUMMARY_CALL_TIMEOUT,
            summarise_thread(ai, guard, &transcript),
        )
        .await
        {
            Ok(s) => s?,
            Err(_) => {
                // Bounded (review I2): keep the old summary; the turn still starts a new session.
                tracing::warn!("the rollover summary timed out; the old summary is kept");
                return Ok((
                    ChatHistory::Rollover {
                        summary: t.conversation.summary.clone(),
                        recent: recent(),
                    },
                    true,
                ));
            }
        };
        let summary_key = format!("{}:summary", turn_key(t.message.id, t.attempt));
        self.meter_now(
            t.account_id,
            t.message.id,
            &s.calls,
            &summary_key,
            Models::EXPLAIN,
        )
        .await;
        if let Some(new) = &s.summary {
            let last = since.last().map_or(through, |p| p.seq);
            let mut conn = self.0.pool.acquire().await?;
            chat::set_summary(&mut conn, t.conversation.id, new, last).await?;
        }
        Ok((
            ChatHistory::Rollover {
                summary: s.summary.or_else(|| t.conversation.summary.clone()),
                recent: recent(),
            },
            true,
        ))
    }

    async fn answer(
        &self,
        ai: &dyn AiClient,
        guard: &(dyn Fn() -> BoxFuture<'static, Result<(), AiError>> + Send + Sync),
        t: &Loaded,
        input: Arc<CoachingInput>,
        now: OffsetDateTime,
        sink: &TurnSink,
    ) -> Result<ChatTurnOutcome, TurnError> {
        let d = &self.0;
        let id = t.message.id;
        let account_id = t.account_id;
        let (history, rolled_over) = self.history(ai, guard, t).await?;

        let ds = Arc::new(DbCoachDataSource::new(
            d.pool.clone(),
            ChatScope {
                account_id,
                time_class: t.time_class,
                input: input.clone(),
            },
        ));
        let mut evidence = build_evidence(&input);
        let details = |anchor: Option<ChatAnchor>| {
            let ds = ds.clone();
            async move {
                let anchor = anchor?;
                match ds.resolve_anchor(&anchor).await {
                    Ok(Some(v)) => Some((anchor, v)),
                    Ok(None) => {
                        tracing::warn!(
                            kind = anchor_kind(&anchor),
                            "foreign or unknown chat anchor ignored"
                        );
                        None
                    }
                    Err(e) => {
                        tracing::warn!(kind = anchor_kind(&anchor), error = %format!("{e:#}"), "resolving a chat anchor failed");
                        None
                    }
                }
            }
        };
        let conv_anchor = details(parse_anchor(t.conversation.anchor.as_ref())).await;
        let msg_anchor = details(t.question_anchor.clone()).await;
        let conv_details = conv_anchor.map(|(a, v)| anchor_details(&mut evidence, &a, &v));
        let msg_details = msg_anchor.map(|(a, v)| anchor_details(&mut evidence, &a, &v));

        let messages = build_chat_request_messages(
            &input,
            conv_details.as_ref(),
            &history,
            &t.question,
            msg_details.as_ref(),
        );
        let previous_in_sync = t.previous.as_ref().map(|(s, _)| *s);
        // A retried attempt is never in sync (review I1): the SDK session may hold the failed
        // attempt's question, answer and tool results, so it is replaced from Postgres.
        let resume = t.attempt <= 1 && resume_next_turn(previous_in_sync, rolled_over);
        let previous_usage: Option<Vec<ModelUsage>> = if resume {
            t.previous
                .as_ref()
                .and_then(|(_, u)| u.clone())
                .and_then(|v| serde_json::from_value(v).ok())
        } else {
            None
        };
        let engine = Arc::new(DbInteractiveEngine::new(
            d.pool.clone(),
            d.queue.clone(),
            d.entitlements.clone(),
            account_id,
            id,
            now,
        ));
        let deps = OwnedToolDeps { ds, engine };
        let pool = d.pool.clone();
        let owner = t.conversation.user_id;
        let attempt = t.attempt;
        let on_chunk = move |c: ChatChunkOut| -> BoxFuture<'static, anyhow::Result<()>> {
            let pool = pool.clone();
            Box::pin(async move {
                let mut tx = pool.begin().await?;
                emit_for_user(
                    &mut tx,
                    account_id,
                    owner,
                    CHAT_CHUNK,
                    &ChatChunk {
                        message_id: id,
                        attempt,
                        seq: c.seq,
                        text: c.text,
                        citations: citations_of(&c.segments),
                        segments: c.segments,
                        interim: c.interim,
                    },
                )
                .await?;
                tx.commit().await?;
                Ok(())
            })
        };
        let first_call_index = u32::try_from(t.next_call_index).unwrap_or(0);
        let out = run_chat_turn_into(
            ai,
            guard,
            &deps,
            ChatTurnArgs {
                evidence,
                messages,
                conversation_id: t.conversation.id,
                resume,
                previous_session_usage: previous_usage.as_deref(),
                first_call_index,
            },
            &on_chunk,
            sink,
        )
        .await;
        // The caller meters the sink's calls on an error.
        out.map_err(|e| TurnError::Ai(e.error))
    }

    /// Stores the answer; `false` when the conversation (or the message) is gone.
    async fn finish(
        &self,
        conn: &mut PgConnection,
        t: &Loaded,
        o: &ChatTurnOutcome,
        (snapshot, input_hash): &(Option<Uuid>, String),
        now: OffsetDateTime,
    ) -> Result<bool, JobError> {
        let d = &self.0;
        let id = t.message.id;
        let conv = t.conversation.id;
        let mut tx = conn.begin().await.map_err(|e| retryable("db", e))?;
        // Waits for a delete in progress; a deleted conversation is not found.
        let exists: Option<i32> =
            sqlx::query_scalar("SELECT 1 FROM coach_conversations WHERE id = $1 FOR SHARE")
                .bind(conv)
                .fetch_optional(&mut *tx)
                .await
                .map_err(|e| retryable("check conversation", e))?;
        let finished = exists.is_some()
            && chat::finish_message(
                &mut tx,
                id,
                &MessageFinish {
                    text: o.text.clone(),
                    segments: serde_json::to_value(&o.segments).expect("segments serialise"),
                    citations: serde_json::to_value(&o.citations).expect("citations serialise"),
                    api_messages: Some(
                        serde_json::to_value(&o.new_messages).expect("messages serialise"),
                    ),
                    dropped_count: i32::try_from(o.dropped).unwrap_or(i32::MAX),
                    ai_disclosure: Some(
                        serde_json::to_value(make_disclosure(
                            Models::CHAT.as_str(),
                            CHAT_PROMPT_VERSION,
                            now,
                        ))
                        .expect("disclosure serialises"),
                    ),
                    session: o.session.map(|p| match p {
                        crate::client::port::SessionPath::New => crate::db::SessionPath::New,
                        crate::client::port::SessionPath::Resumed => {
                            crate::db::SessionPath::Resumed
                        }
                        crate::client::port::SessionPath::Rebuilt => {
                            crate::db::SessionPath::Rebuilt
                        }
                    }),
                    session_model_usage: Some(
                        serde_json::to_value(&o.session_model_usage).expect("usage serialises"),
                    ),
                },
            )
            .await
            .map_err(|e| retryable("finish", e))?;
        if !finished {
            tx.rollback().await.map_err(|e| retryable("rollback", e))?;
            // The calls were paid for; usage rows carry no content.
            self.meter_now(
                t.account_id,
                id,
                &o.calls,
                &turn_key(id, t.attempt),
                Models::CHAT,
            )
            .await;
            sqlx::query(
                "DELETE FROM ai_sdk_session_entries WHERE session_id = $1 \
                 AND NOT EXISTS (SELECT 1 FROM coach_conversations WHERE id = $1)",
            )
            .bind(conv)
            .execute(&mut *conn)
            .await
            .map_err(|e| retryable("orphan entries", e))?;
            return Ok(false);
        }
        sqlx::query(
            "UPDATE coach_messages SET session_in_sync = $2, coaching_input_id = $3,              input_hash = $4 WHERE id = $1",
        )
            .bind(id)
            .bind(o.session_in_sync)
            .bind(*snapshot)
            .bind(input_hash)
            .execute(&mut *tx)
            .await
            .map_err(|e| retryable("in sync", e))?;
        sqlx::query(
            "UPDATE coach_conversations SET next_call_index = greatest(next_call_index, $2) \
             WHERE id = $1",
        )
        .bind(conv)
        .bind(i32::try_from(o.next_call_index).unwrap_or(i32::MAX))
        .execute(&mut *tx)
        .await
        .map_err(|e| retryable("call index", e))?;
        let evals: Vec<Uuid> = o
            .engine_evals
            .iter()
            .filter_map(|e| Uuid::try_parse(e).ok())
            .collect();
        if !evals.is_empty() {
            // Only this account's evals (the engine is account-scoped; this is belt and braces).
            sqlx::query(
                "UPDATE interactive_evals SET message_id = $2 WHERE id = ANY($1) AND account_id = $3",
            )
            .bind(&evals)
            .bind(id)
            .bind(t.account_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| retryable("link evals", e))?;
        }
        meter_keyed(
            &mut tx,
            &d.pricing,
            t.account_id,
            AiFeature::Chat,
            &o.calls,
            &id.to_string(),
            Models::CHAT,
            &turn_key(id, t.attempt),
        )
        .await
        .map_err(|e| retryable("meter", e))?;
        emit_for_user(
            &mut tx,
            t.account_id,
            t.conversation.user_id,
            CHAT_MESSAGE_READY,
            &ChatReady {
                message_id: id,
                attempt: t.attempt,
                replace: o.replaced,
            },
        )
        .await
        .map_err(|e| retryable("emit", e))?;
        tx.commit().await.map_err(|e| retryable("commit", e))?;
        Ok(true)
    }

    /// The conversation's title after its first answered turn: the model's (validated) title, or
    /// the anchor's fallback title. Best effort: a failed call gives the fallback.
    async fn set_title(
        &self,
        ai: &dyn AiClient,
        guard: &(dyn Fn() -> BoxFuture<'static, Result<(), AiError>> + Send + Sync),
        t: &Loaded,
        o: &ChatTurnOutcome,
    ) {
        let anchor = parse_anchor(t.conversation.anchor.as_ref());
        let mut title = None;
        if o.outcome == ChatOutcomeKind::Answered {
            let transcript = vec![(t.question.clone(), o.text.clone())];
            match tokio::time::timeout(
                SUMMARY_CALL_TIMEOUT,
                summarise_thread(ai, guard, &transcript),
            )
            .await
            {
                Ok(Ok(s)) => {
                    let key = format!("chat:{}:title", t.message.id);
                    self.meter_now(t.account_id, t.message.id, &s.calls, &key, Models::EXPLAIN)
                        .await;
                    title = s.title;
                }
                Ok(Err(e)) => tracing::warn!(error = %e, "titling a conversation failed"),
                Err(_) => tracing::warn!("titling a conversation timed out"),
            }
        }
        let title = title.unwrap_or_else(|| fallback_title(anchor.as_ref()).to_owned());
        let r = sqlx::query(
            "UPDATE coach_conversations SET title = $2 WHERE id = $1 AND title IS NULL",
        )
        .bind(t.conversation.id)
        .bind(&title)
        .execute(&self.0.pool)
        .await;
        if let Err(e) = r {
            tracing::warn!(error = %e, "storing a conversation title failed");
        }
    }
}

impl Handler for CoachChatHandler {
    fn kind(&self) -> &'static str {
        KIND_COACH_CHAT
    }

    fn handle(&self, job: Job, _ctx: JobContext) -> BoxFuture<'_, Result<Value, JobError>> {
        Box::pin(self.run(job))
    }

    /// A dead-lettered turn fails with `unavailable` and its event.
    fn on_dead<'a>(&'a self, job: &'a Job, _reason: &'a str) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            let Ok(p) = serde_json::from_value::<CoachChatPayload>(job.payload.clone()) else {
                return;
            };
            let r = async {
                let mut conn = self
                    .0
                    .pool
                    .acquire()
                    .await
                    .map_err(|e| retryable("db", e))?;
                let who: Option<(Uuid, Uuid)> = sqlx::query_as(
                    "SELECT c.account_id, c.user_id FROM coach_messages m \
                     JOIN coach_conversations c ON c.id = m.conversation_id WHERE m.id = $1",
                )
                .bind(p.message_id)
                .fetch_optional(&mut *conn)
                .await
                .map_err(|e| retryable("db", e))?;
                if let Some(who) = who {
                    fail_and_emit(&mut conn, who, p.message_id, REASON_UNAVAILABLE).await?;
                }
                Ok::<(), JobError>(())
            }
            .await;
            if let Err(e) = r {
                tracing::warn!(error = %e, "failing a dead chat turn failed");
            }
        })
    }
}

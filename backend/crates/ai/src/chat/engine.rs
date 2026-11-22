//! The "Ask your coach" chat engine: one player question in, one validated answer out
//! (ADR-R3-8).
//!
//! A turn is one SDK `stream_turn` with the chat tools (ADR-R3-6): the sidecar runs the tool
//! rounds and calls each tool back through this turn's [`ToolHandler`], which wraps
//! `execute_tool` over ONE [`ToolSession`] (the report evidence, the anchors, and what the tools
//! add). The streamed text is split into paragraphs; every paragraph is validated against that
//! session's evidence before the player sees it:
//!
//! 1. A passing paragraph is published (`on_chunk`) while the stream is not held. The first
//!    failing paragraph holds the stream: nothing after it is published.
//! 2. A paragraph that is exactly `{{offtopic}}` ends the answer with [`CHAT_OFF_TOPIC_TEXT`].
//! 3. A refusal gives [`CHAT_REFUSAL_TEXT`].
//! 4. `max_turns` (the tool rounds ran out) gets one follow-up turn without tools: the
//!    flattened transcript, the tool results as data, and "Answer now with what you have."
//! 5. A held answer gets one repair turn (no tools, nothing published); its failing paragraphs
//!    are dropped and the answer is replaced.
//! 6. With nothing left, the answer is [`CHAT_FALLBACK_TEXT`]. Otherwise a drop note, and an
//!    engine-limit note when a check hit the quota, are appended (server copy).
//!
//! The follow-up and repair turns run under a fresh session id (`resume: false`), so they never
//! enter the conversation's SDK session; their session-store rows are swept as orphans
//! (Task 21). The conversation's `session_model_usage` comes only from the main turn's `Done`.
//!
//! Stand-ins until lane D lands on this base (replace at merge, see the Task 23 report):
//! [`CallRecord`] and [`BeforeCall`] are T14's; [`ChatPrompts`] carries T13's `core/system` +
//! `core/tokens` and `repair/repair` texts, which the caller loads with `load_prompt`.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use futures::StreamExt;
use futures::future::BoxFuture;
use futures::stream::BoxStream;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::mpsc;
use uuid::Uuid;

use super::context::{anchor_block, context_block, tagged_json};
use super::data_source::EngineCheck;
use super::execute::{
    OwnedToolDeps, ToolExec, ToolExecutor, ToolOutput, ToolSession, engine_result_output,
};
use super::limits::{
    CHAT_ENGINE_PENDING_TEXT, CHAT_FALLBACK_TEXT, CHAT_LIMITS, CHAT_OFF_TOPIC_TEXT,
    CHAT_REFUSAL_TEXT, ENGINE_WAIT_INLINE, ENGINE_WAIT_MAX, dropped_note, engine_limit_note,
};
use super::paragraphs::split_paragraphs;
use super::tools::chat_tools;
use crate::client::flatten::flatten_for_sidecar;
use crate::client::model::{Effort, Models};
use crate::client::port::{
    AiClient, AiError, AiUsage, ChatTurnRequest, Message, ModelUsage, Role, SessionPath, TextBlock,
    ToolChoice, ToolHandler, ToolReply, TurnEvent, TurnResult, metered_turn_usage,
};
use crate::entitlements::QuotaWindow;
use crate::input::types::CoachingInput;
use crate::output::codes::{Citation, ValidationCode, ValidationError};
use crate::output::schema::ITEM_TEXT_MAX_CHARS;
use crate::segments::Segment;
use crate::validator::evidence::Evidence;
use crate::validator::language::{banned_phrases, engine_claims, no_marketing, no_quantity_words};
use crate::validator::text::{TextRules, validate_text};

/// `prompts/chat/system.v1.md` (registered in T13's prompt table at merge). The file holds no
/// `{{…}}` at all: T13's `render_template` treats `{{name}}` (`[a-z_]+`) as a placeholder, so a
/// literal `{{offtopic}}` there would be a missing variable. The marker is added in code
/// ([`OFFTOPIC_LINE`]), never through a template.
pub const CHAT_SYSTEM_PROMPT: &str = include_str!("../../prompts/chat/system.v1.md");

/// The whole-paragraph off-topic marker (the `offtopic` token of the validator's grammar).
pub const OFFTOPIC_MARKER: &str = "{{offtopic}}";

/// The last line of `system[1]`, which the prompt file refers to as "the off-topic marker
/// given at the end of these instructions".
pub const OFFTOPIC_LINE: &str = "The off-topic marker is exactly: {{offtopic}}";

/// The mid-conversation system message of the follow-up turn after the round cap.
pub const ANSWER_NOW_TEXT: &str = "Answer now with what you have.";

const OFFTOPIC: &str = OFFTOPIC_MARKER;

/// The prompts the chat shares with the report (T13). `core` is `core/system` + `"\n\n"` +
/// `core/tokens`; `repair` is `repair/repair`, with one `{{errors}}` placeholder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatPrompts {
    pub core: String,
    pub repair: String,
}

/// `system[0]` = the core prompt (cached), `system[1]` = `chat/system` + `"\n\n"` +
/// [`OFFTOPIC_LINE`] (cached).
pub fn chat_system_blocks(p: &ChatPrompts) -> Vec<TextBlock> {
    vec![
        TextBlock::cached(p.core.clone()),
        TextBlock::cached(format!(
            "{}\n\n{OFFTOPIC_LINE}",
            CHAT_SYSTEM_PROMPT.trim_end()
        )),
    ]
}

/// T14's `CallRecord` (stand-in): one metered model entry of one call.
#[derive(Clone, Debug, PartialEq)]
pub struct CallRecord {
    pub model: String,
    pub usage: AiUsage,
    pub batch: bool,
    /// The SDK's estimate, for comparison only (on the call's first record).
    pub sdk_cost_usd: Option<f64>,
}

/// T14's `BeforeCall` (stand-in): the budget hook, run before every model call.
pub type BeforeCall<'a> = &'a (dyn Fn() -> BoxFuture<'static, Result<(), AiError>> + Send + Sync);

/// Publishes one chunk (`chat.chunk`). An error is logged and the turn goes on; the final
/// message then replaces what was streamed.
pub type OnChunk<'a> =
    &'a (dyn Fn(ChatChunkOut) -> BoxFuture<'static, anyhow::Result<()>> + Send + Sync);

/// The history before the new question.
#[derive(Clone, Debug, PartialEq)]
pub enum ChatHistory {
    /// Earlier messages, replayed unchanged (append-only).
    Verbatim(Vec<Message>),
    /// After a rollover: a summary of the older turns and the recent turns as plain text. No
    /// earlier assistant block is carried, so no thinking block is replayed after an edit.
    Rollover {
        summary: Option<String>,
        /// `(question, answer token text)`.
        recent: Vec<(String, String)>,
    },
}

/// User or model text placed inside one of our tags: `&`, `<` and `>` are escaped, so the text
/// can never open or close a tag.
pub(crate) fn escape_tag_text(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn text_json(s: impl Into<String>) -> Value {
    TextBlock::new(s).to_json()
}

/// The request messages of a turn:
/// 1. a user message (cached at its last block): the coaching context, the conversation anchor
///    if any, and for a rollover the thread summary and the recent turns;
/// 2. the verbatim history, unchanged;
/// 3. the question: `[anchor_block(message_anchor)?, "<question>…</question>"]`.
///
/// The anchors are `anchor_details` values. Question, summary and recent turns are escaped.
pub fn build_chat_request_messages(
    input: &CoachingInput,
    conversation_anchor: Option<&Value>,
    history: &ChatHistory,
    question: &str,
    message_anchor: Option<&Value>,
) -> Vec<Message> {
    let mut first = vec![TextBlock::new(context_block(input))];
    if let Some(a) = conversation_anchor {
        first.push(TextBlock::new(anchor_block(a)));
    }
    if let ChatHistory::Rollover { summary, recent } = history {
        if let Some(s) = summary {
            first.push(TextBlock::new(format!(
                "<thread_summary>{}</thread_summary>",
                escape_tag_text(s)
            )));
        }
        if !recent.is_empty() {
            let turns = recent
                .iter()
                .map(|(q, a)| format!("Player: {q}\nCoach: {a}"))
                .collect::<Vec<_>>()
                .join("\n\n");
            first.push(TextBlock::new(format!(
                "<recent_turns>{}</recent_turns>",
                escape_tag_text(&turns)
            )));
        }
    }
    if let Some(last) = first.last_mut() {
        last.cache = true;
    }
    let mut out = vec![Message {
        role: Role::User,
        content: first.iter().map(TextBlock::to_json).collect(),
    }];
    if let ChatHistory::Verbatim(h) = history {
        out.extend(h.iter().cloned());
    }
    let mut q = Vec::new();
    if let Some(a) = message_anchor {
        q.push(text_json(anchor_block(a)));
    }
    q.push(text_json(format!(
        "<question>{}</question>",
        escape_tag_text(question)
    )));
    out.push(Message {
        role: Role::User,
        content: q,
    });
    out
}

/// One published piece of the answer (`chat.chunk`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatChunkOut {
    pub seq: u32,
    pub text: String,
    pub segments: Vec<Segment>,
    /// A status line (the engine is checking), not part of the stored answer.
    pub interim: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatOutcomeKind {
    Answered,
    Refused,
    OffTopic,
    Fallback,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChatTurnOutcome {
    /// The stored answer: token text of the kept paragraphs plus server notes, or fixed copy.
    pub text: String,
    pub segments: Vec<Segment>,
    /// Positions referenced by tokens in the kept paragraphs, each once, in order.
    pub citations: Vec<Citation>,
    /// Paragraphs dropped after the repair.
    pub dropped: u32,
    /// The streamed chunks must be replaced by the final message (`chat.message.ready`).
    pub replaced: bool,
    /// `[question, assistant(text)]`, to append to a verbatim history.
    pub new_messages: Vec<Message>,
    pub calls: Vec<CallRecord>,
    pub outcome: ChatOutcomeKind,
    /// Engine checks that returned a result this turn (to link their rows to the message).
    pub engine_evals: Vec<String>,
    /// The main turn's `Done.session_model_usage`: store it for the next turn's metering.
    pub session_model_usage: Vec<ModelUsage>,
    pub session: Option<SessionPath>,
    /// The stored answer is exactly what the conversation's SDK session recorded. When false,
    /// the next turn must not resume: see [`resume_next_turn`].
    pub session_in_sync: bool,
    /// The first tool-call index of the next turn (store it with the conversation).
    pub next_call_index: u32,
}

/// One turn's inputs besides the client, the budget hook, the tools and `on_chunk`.
pub struct ChatTurnArgs<'a> {
    pub prompts: &'a ChatPrompts,
    /// `build_evidence(input)` plus the anchors (`anchor_details` with this evidence).
    pub evidence: Evidence,
    /// [`build_chat_request_messages`]; the last message is the question.
    pub messages: Vec<Message>,
    /// The Postgres conversation id, which is also the SDK session id.
    pub conversation_id: Uuid,
    /// True on every turn after the first of the SDK session: only the question is sent, and
    /// the earlier messages, flattened, are the rebuild context.
    pub resume: bool,
    /// The previous turn's stored `session_model_usage` (metering of a resumed turn).
    pub previous_session_usage: Option<&'a [ModelUsage]>,
    /// The first tool-call index of this turn: the previous outcome's `next_call_index`, 0 for
    /// a new conversation. Call indexes are unique across the conversation, so a `tools.<k>`
    /// token of an earlier turn never resolves to a different result.
    pub first_call_index: u32,
}

/// Whether the next turn resumes the SDK session: only when the previous answer is in sync
/// with it and the history does not roll over. Otherwise the next turn sends the full Postgres
/// history with `resume: false` (the sidecar then replaces the stale session under the same id)
/// and no previous session usage. `None`: no previous turn.
pub fn resume_next_turn(previous_in_sync: Option<bool>, rollover: bool) -> bool {
    previous_in_sync == Some(true) && !rollover
}

// --- The turn's tool handler ---

#[derive(Default)]
struct TurnToolState {
    /// `(name, input, result content)` in call order, for the follow-up and repair turns.
    transcript: Vec<(String, Value, String)>,
    limit: Option<QuotaWindow>,
    evals: Vec<String>,
}

struct TurnTools {
    deps: OwnedToolDeps,
    session: Arc<ToolSession>,
    state: Arc<Mutex<TurnToolState>>,
    interim: mpsc::UnboundedSender<()>,
    interim_sent: AtomicBool,
}

fn lock(state: &Mutex<TurnToolState>) -> MutexGuard<'_, TurnToolState> {
    state.lock().unwrap_or_else(|p| p.into_inner())
}

impl TurnTools {
    /// The engine-pending flow: one interim line per turn, then the remaining wait; a check
    /// still pending (or failing) is abandoned, which refunds its quota.
    async fn wait_engine(&self, eval_id: &str) -> ToolOutput {
        if !self.interim_sent.swap(true, Ordering::SeqCst) {
            let _ = self.interim.send(());
        }
        let wait = ENGINE_WAIT_MAX.saturating_sub(ENGINE_WAIT_INLINE);
        let check = match self.deps.engine.wait(eval_id, wait).await {
            Ok(EngineCheck::Pending { eval_id }) => {
                self.abandon(&eval_id).await;
                EngineCheck::Pending { eval_id }
            }
            Ok(check) => check,
            Err(e) => {
                tracing::warn!(error = %format!("{e:#}"), "interactive engine wait failed");
                self.abandon(eval_id).await;
                EngineCheck::Unavailable {
                    reason: "wait failed".to_owned(),
                }
            }
        };
        engine_result_output(&self.session, check)
    }

    async fn abandon(&self, eval_id: &str) {
        if let Err(e) = self.deps.engine.abandon(eval_id).await {
            tracing::warn!(error = %format!("{e:#}"), "abandoning an engine check failed");
        }
    }

    /// Records the call for the follow-up turns, the engine-limit note and the eval links.
    fn note(&self, name: &str, input: &Value, out: &ToolOutput) {
        let mut st = lock(&self.state);
        st.transcript
            .push((name.to_owned(), input.clone(), out.content.clone()));
        if out.is_error || name != "analyse_position" {
            return;
        }
        let Ok(v) = serde_json::from_str::<Value>(&out.content) else {
            return;
        };
        if v["reason"] == "limit" {
            st.limit = match v["window"].as_str() {
                Some("day") => Some(QuotaWindow::Day),
                Some("month") => Some(QuotaWindow::Month),
                _ => Some(QuotaWindow::Week),
            };
        }
        if let Some(id) = v["evalId"].as_str()
            && !st.evals.iter().any(|e| e == id)
        {
            st.evals.push(id.to_owned());
        }
    }
}

impl ToolHandler for TurnTools {
    fn call<'a>(&'a self, name: &'a str, input: Value) -> BoxFuture<'a, ToolReply> {
        Box::pin(async move {
            // The data source and engine are scoped to the conversation's account, and the
            // executor validates every field before anything is read.
            let out = match self.deps.execute(&self.session, name, Some(&input)).await {
                ToolExec::Ready(o) => o,
                ToolExec::EnginePending { eval_id } => self.wait_engine(&eval_id).await,
            };
            self.note(name, &input, &out);
            ToolReply {
                content: out.content,
                is_error: out.is_error,
            }
        })
    }
}

// --- Paragraph streaming ---

fn chat_rules() -> TextRules {
    TextRules {
        require_citation: false,
        line_keys: None,
        implicit_citations: true,
        extra: vec![
            banned_phrases(),
            no_marketing(),
            engine_claims(),
            no_quantity_words(),
        ],
    }
}

struct Para {
    text: String,
    msg: Option<String>,
    result: Result<Vec<Segment>, Vec<ValidationError>>,
    published: bool,
}

/// Feeds one or more turns' events into paragraphs, validating and (optionally) publishing.
struct Streamer<'a> {
    session: &'a ToolSession,
    rules: TextRules,
    on_chunk: OnChunk<'a>,
    publish: bool,
    seq: u32,
    count: usize,
    held: bool,
    offtopic: bool,
    msg: Option<String>,
    buf: String,
    saw_text: bool,
    paras: Vec<Para>,
    /// A published paragraph was retracted, or a publish failed.
    stale: bool,
    /// Any message was retracted (the SDK session then differs from the stored text).
    retracted: bool,
}

impl<'a> Streamer<'a> {
    fn new(session: &'a ToolSession, on_chunk: OnChunk<'a>, publish: bool, seq: u32) -> Self {
        Streamer {
            session,
            rules: chat_rules(),
            on_chunk,
            publish,
            seq,
            count: 0,
            held: false,
            offtopic: false,
            msg: None,
            buf: String::new(),
            saw_text: false,
            paras: Vec::new(),
            stale: false,
            retracted: false,
        }
    }

    fn validate(&self, text: &str, path: &str) -> Result<Vec<Segment>, Vec<ValidationError>> {
        if text.chars().count() > ITEM_TEXT_MAX_CHARS {
            return Err(vec![ValidationError {
                code: ValidationCode::SchemaInvalid,
                path: path.to_owned(),
                detail: format!("a paragraph has at most {ITEM_TEXT_MAX_CHARS} characters"),
            }]);
        }
        self.session
            .with_evidence(|ev| validate_text(text, &[], ev, &self.rules, path))
    }

    async fn send(&mut self, text: String, segments: Vec<Segment>, interim: bool) {
        let chunk = ChatChunkOut {
            seq: self.seq,
            text,
            segments,
            interim,
        };
        self.seq += 1;
        if let Err(e) = (self.on_chunk)(chunk).await {
            tracing::warn!(error = %format!("{e:#}"), "publishing a chat chunk failed");
            self.stale = true;
        }
    }

    async fn interim(&mut self) {
        if self.publish {
            let text = CHAT_ENGINE_PENDING_TEXT.to_owned();
            let segments = vec![Segment::Text { text: text.clone() }];
            self.send(text, segments, true).await;
        }
    }

    async fn paragraph(&mut self, text: String) {
        if self.offtopic {
            return;
        }
        if text == OFFTOPIC {
            self.offtopic = true;
            return;
        }
        let path = format!("chat.{}", self.count);
        self.count += 1;
        let result = self.validate(&text, &path);
        let mut published = false;
        match &result {
            Ok(segments) if self.publish && !self.held => {
                self.send(text.clone(), segments.clone(), false).await;
                published = true;
            }
            Ok(_) => {}
            Err(_) => self.held = true,
        }
        self.paras.push(Para {
            text,
            msg: self.msg.clone(),
            result,
            published,
        });
    }

    async fn delta(&mut self, d: &str) {
        self.saw_text = true;
        if self.offtopic {
            return;
        }
        self.buf.push_str(d);
        let (paras, rest) = split_paragraphs(&self.buf);
        self.buf = rest;
        for p in paras {
            self.paragraph(p).await;
        }
    }

    /// The unfinished last paragraph of a turn cut off by `max_tokens`: it is never shown, and
    /// it holds the stream, so the answer is repaired (review m2).
    fn truncate(&mut self) {
        let rest = std::mem::take(&mut self.buf);
        let rest = rest.trim();
        if rest.is_empty() || self.offtopic {
            return;
        }
        let path = format!("chat.{}", self.count);
        self.count += 1;
        self.held = true;
        self.paras.push(Para {
            text: rest.to_owned(),
            msg: self.msg.clone(),
            result: Err(vec![ValidationError {
                code: ValidationCode::Truncated,
                path,
                detail: "the answer was cut off in this paragraph".to_owned(),
            }]),
            published: false,
        });
    }

    async fn flush(&mut self) {
        let rest = std::mem::take(&mut self.buf);
        let rest = rest.trim();
        if !rest.is_empty() {
            self.paragraph(rest.to_owned()).await;
        }
    }

    /// Handles one event; `Some` at the end of the turn.
    async fn event(&mut self, e: TurnEvent) -> Option<TurnResult> {
        match e {
            TurnEvent::Message(id) => {
                self.flush().await;
                self.msg = Some(id);
            }
            TurnEvent::TextDelta(d) => self.delta(&d).await,
            TurnEvent::Retract { message_ids, .. } => {
                self.retracted = true;
                if self.msg.as_ref().is_some_and(|m| message_ids.contains(m)) {
                    self.buf.clear();
                }
                let before = self.paras.len();
                let mut stale = false;
                self.paras.retain(|p| {
                    let gone = p.msg.as_ref().is_some_and(|m| message_ids.contains(m));
                    stale |= gone && p.published;
                    !gone
                });
                self.stale |= stale;
                // Validation state follows the kept paragraphs.
                if self.paras.len() != before {
                    self.held = self.paras.iter().any(|p| p.result.is_err());
                }
            }
            TurnEvent::Done(r) => {
                if !self.saw_text {
                    // Nothing streamed: the authoritative final text is all there is.
                    let text: Vec<String> = r
                        .content
                        .iter()
                        .filter_map(|b| b.get("text").and_then(Value::as_str))
                        .map(str::to_owned)
                        .collect();
                    for t in text {
                        self.delta(&t).await;
                    }
                }
                if r.stop_reason.as_deref() == Some("max_tokens") {
                    self.truncate();
                } else {
                    self.flush().await;
                }
                self.msg = None;
                return Some(r);
            }
        }
        None
    }

    /// Runs one turn's stream to its `Done`, publishing interim lines as the tools ask.
    async fn drive(
        &mut self,
        mut stream: BoxStream<'_, Result<TurnEvent, AiError>>,
        interim: &mut mpsc::UnboundedReceiver<()>,
    ) -> Result<TurnResult, AiError> {
        self.saw_text = false;
        loop {
            tokio::select! {
                biased;
                Some(()) = interim.recv() => self.interim().await,
                ev = stream.next() => {
                    while interim.try_recv().is_ok() {
                        self.interim().await;
                    }
                    match ev {
                        None => {
                            return Err(AiError::Response(
                                "chat stream ended without a result".to_owned(),
                            ));
                        }
                        Some(Err(e)) => return Err(e),
                        Some(Ok(e)) => {
                            if let Some(done) = self.event(e).await {
                                return Ok(done);
                            }
                        }
                    }
                }
            }
        }
    }

    fn answer_text(&self) -> String {
        self.paras
            .iter()
            .map(|p| p.text.as_str())
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    fn errors(&self) -> Vec<&ValidationError> {
        self.paras
            .iter()
            .filter_map(|p| p.result.as_ref().err())
            .flatten()
            .collect()
    }
}

// --- Requests ---

fn records(usage: &[ModelUsage], sdk_cost_usd: Option<f64>) -> Vec<CallRecord> {
    usage
        .iter()
        .enumerate()
        .map(|(i, m)| CallRecord {
            model: m.model.clone(),
            usage: m.usage,
            batch: false,
            sdk_cost_usd: if i == 0 { sdk_cost_usd } else { None },
        })
        .collect()
}

fn invalid_request() -> AiError {
    AiError::Request {
        status: 400,
        api_error_type: Some("invalid_request_error".to_owned()),
    }
}

/// The earlier messages, flattened as the sidecar would show them (no thinking blocks).
fn rebuild_context(messages: &[Message]) -> Result<Option<String>, AiError> {
    let (_, blocks) = flatten_for_sidecar(&[], messages)?;
    let tail = messages.last().map_or(0, |m| {
        m.content
            .iter()
            .filter(|b| b.get("type").and_then(Value::as_str) == Some("text"))
            .count()
    });
    let text = blocks[..blocks.len().saturating_sub(tail)]
        .iter()
        .filter_map(|b| b.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n\n");
    Ok((!text.is_empty()).then_some(text))
}

/// The turn's tool results as data for a follow-up turn, or `None` without any.
fn tool_results_block(transcript: &[(String, Value, String)]) -> Option<Value> {
    if transcript.is_empty() {
        return None;
    }
    let results: Vec<Value> = transcript
        .iter()
        .map(|(name, input, content)| {
            let result = serde_json::from_str::<Value>(content)
                .unwrap_or_else(|_| Value::String(content.clone()));
            json!({"tool": name, "input": input, "result": result})
        })
        .collect();
    Some(text_json(format!(
        "<tool_results>{}</tool_results>",
        tagged_json(&Value::Array(results))
    )))
}

/// A one-round turn without tools, under a fresh session id.
fn one_shot(system: Vec<TextBlock>, messages: Vec<Message>) -> ChatTurnRequest {
    ChatTurnRequest {
        model: Models::CHAT,
        system,
        tools: Vec::new(),
        tool_choice: ToolChoice::None,
        messages,
        max_tokens: CHAT_LIMITS.max_tokens,
        effort: Effort::Low,
        max_turns: 1,
        conversation_id: Uuid::new_v4(),
        resume: false,
        rebuild_context: None,
    }
}

/// The history, the "answer now" instruction, then the question with the tool results.
fn answer_now_messages(messages: &[Message], tools: Option<Value>) -> Vec<Message> {
    let (question, history) = messages.split_last().expect("checked non-empty");
    let mut out = history.to_vec();
    out.push(Message {
        role: Role::System,
        content: vec![text_json(ANSWER_NOW_TEXT)],
    });
    let mut q = question.clone();
    q.content.extend(tools);
    out.push(q);
    out
}

/// The messages, the tool results, the answer, then the repair instruction with its errors.
fn repair_messages(
    messages: &[Message],
    tools: Option<Value>,
    answer: &str,
    repair: &str,
    errors: &[&ValidationError],
) -> Vec<Message> {
    let mut out = messages.to_vec();
    if let Some(t) = tools {
        out.push(Message {
            role: Role::User,
            content: vec![t],
        });
    }
    out.push(Message {
        role: Role::Assistant,
        content: vec![text_json(answer)],
    });
    let lines = errors
        .iter()
        .map(|e| format!("- {}: {}", e.code.as_str(), e.detail))
        .collect::<Vec<_>>()
        .join("\n");
    out.push(Message {
        role: Role::User,
        content: vec![text_json(repair.replace("{{errors}}", &lines))],
    });
    out
}

fn closed_channel() -> mpsc::UnboundedReceiver<()> {
    mpsc::unbounded_channel().1
}

// --- The turn ---

/// Merges adjacent text segments.
fn push_segments(out: &mut Vec<Segment>, segments: Vec<Segment>) {
    for s in segments {
        match (out.last_mut(), s) {
            (Some(Segment::Text { text: a }), Segment::Text { text: b }) => a.push_str(&b),
            (_, s) => out.push(s),
        }
    }
}

fn citations_of(segments: &[Segment]) -> Vec<Citation> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for s in segments {
        let (Segment::Move { game_id, ply, .. }
        | Segment::Best { game_id, ply, .. }
        | Segment::Pv { game_id, ply, .. }) = s
        else {
            continue;
        };
        if seen.insert((game_id.clone(), *ply)) {
            out.push(Citation {
                game_id: game_id.clone(),
                ply: *ply,
            });
        }
    }
    out
}

/// Runs one chat turn (see the module docs). `Unavailable`, `Budget` and request errors of any
/// call propagate; the caller then stores [`super::limits::CHAT_UNAVAILABLE_TEXT`] or similar.
pub async fn run_chat_turn(
    ai: &dyn AiClient,
    before_call: BeforeCall<'_>,
    deps: &OwnedToolDeps,
    args: ChatTurnArgs<'_>,
    on_chunk: OnChunk<'_>,
) -> Result<ChatTurnOutcome, AiError> {
    let ChatTurnArgs {
        prompts,
        evidence,
        messages,
        conversation_id,
        resume,
        previous_session_usage,
        first_call_index,
    } = args;
    let question = match messages.last() {
        Some(m) if m.role == Role::User => m.clone(),
        _ => return Err(invalid_request()),
    };
    let session = Arc::new(ToolSession::starting_at(evidence, first_call_index));
    let state = Arc::new(Mutex::new(TurnToolState::default()));
    let (tx, mut rx) = mpsc::unbounded_channel();
    let handler: Arc<dyn ToolHandler> = Arc::new(TurnTools {
        deps: deps.clone(),
        session: session.clone(),
        state: state.clone(),
        interim: tx,
        interim_sent: AtomicBool::new(false),
    });
    let system = chat_system_blocks(prompts);
    let (sent, rebuild) = if resume {
        (vec![question.clone()], rebuild_context(&messages)?)
    } else {
        (messages.clone(), None)
    };
    let req = ChatTurnRequest {
        model: Models::CHAT,
        system: system.clone(),
        tools: chat_tools(),
        tool_choice: ToolChoice::Auto,
        messages: sent,
        max_tokens: CHAT_LIMITS.max_tokens,
        effort: Effort::Low,
        max_turns: CHAT_LIMITS.max_tool_rounds + 1,
        conversation_id,
        resume,
        rebuild_context: rebuild,
    };

    let mut calls = Vec::new();
    before_call().await?;
    let mut s = Streamer::new(&session, on_chunk, true, 0);
    let main = s
        .drive(ai.stream_turn(&req, Some(handler)), &mut rx)
        .await?;
    calls.extend(records(
        &metered_turn_usage(&main, previous_session_usage),
        main.sdk_cost_usd,
    ));
    let session_model_usage = main.session_model_usage.clone();
    let session_path = main.session;
    let mut refused = main.stop_reason.as_deref() == Some("refusal");

    let followed_up = !refused && !s.offtopic && main.stop_reason.as_deref() == Some("max_turns");
    if followed_up {
        let tools = tool_results_block(&lock(&state).transcript);
        let follow = one_shot(system.clone(), answer_now_messages(&messages, tools));
        before_call().await?;
        let r = s
            .drive(ai.stream_turn(&follow, None), &mut closed_channel())
            .await?;
        calls.extend(records(&r.model_usage, r.sdk_cost_usd));
        refused = r.stop_reason.as_deref() == Some("refusal");
    }

    let published: Vec<String> = s
        .paras
        .iter()
        .filter(|p| p.published)
        .map(|p| p.text.clone())
        .collect();
    let mut stale = s.stale;
    let mut repaired = false;
    let mut dropped = 0u32;
    let mut outcome = if refused {
        ChatOutcomeKind::Refused
    } else if s.offtopic {
        ChatOutcomeKind::OffTopic
    } else {
        ChatOutcomeKind::Answered
    };
    let mut kept: Vec<(String, Vec<Segment>)> = Vec::new();

    if outcome == ChatOutcomeKind::Answered {
        let source = if s.held {
            repaired = true;
            let tools = tool_results_block(&lock(&state).transcript);
            let fix = one_shot(
                system.clone(),
                repair_messages(
                    &messages,
                    tools,
                    &s.answer_text(),
                    &prompts.repair,
                    &s.errors(),
                ),
            );
            before_call().await?;
            let mut r = Streamer::new(&session, on_chunk, false, s.seq);
            let done = r
                .drive(ai.stream_turn(&fix, None), &mut closed_channel())
                .await?;
            calls.extend(records(&done.model_usage, done.sdk_cost_usd));
            if done.stop_reason.as_deref() == Some("refusal") {
                outcome = ChatOutcomeKind::Refused;
            } else if r.offtopic {
                outcome = ChatOutcomeKind::OffTopic;
            }
            r.paras
        } else {
            std::mem::take(&mut s.paras)
        };
        for p in source {
            match p.result {
                Ok(segments) => kept.push((p.text, segments)),
                Err(_) => dropped += 1,
            }
        }
        if outcome == ChatOutcomeKind::Answered && kept.is_empty() {
            outcome = ChatOutcomeKind::Fallback;
        }
    }

    let limit = lock(&state).limit;
    let mut paragraphs: Vec<(String, Vec<Segment>)> = match outcome {
        ChatOutcomeKind::Answered => std::mem::take(&mut kept),
        ChatOutcomeKind::Refused => vec![fixed(CHAT_REFUSAL_TEXT)],
        ChatOutcomeKind::OffTopic => vec![fixed(CHAT_OFF_TOPIC_TEXT)],
        ChatOutcomeKind::Fallback => vec![fixed(CHAT_FALLBACK_TEXT)],
    };
    let answered: Vec<String> = if outcome == ChatOutcomeKind::Answered {
        paragraphs.iter().map(|(t, _)| t.clone()).collect()
    } else {
        Vec::new()
    };
    let mut citations = Vec::new();
    for (_, segs) in &paragraphs {
        for c in citations_of(segs) {
            if !citations.contains(&c) {
                citations.push(c);
            }
        }
    }
    if outcome == ChatOutcomeKind::Answered && dropped > 0 {
        paragraphs.push(fixed(&dropped_note(dropped)));
    }
    if matches!(
        outcome,
        ChatOutcomeKind::Answered | ChatOutcomeKind::Fallback
    ) && let Some(w) = limit
    {
        paragraphs.push(fixed(&engine_limit_note(w)));
    }
    stale |= repaired || !answered.starts_with(&published);
    // In sync: the stored paragraphs are exactly what the main turn left in the conversation's
    // SDK session (server notes aside). An off-topic reply is not counted as in sync: the
    // session holds the marker, the record the fixed copy.
    let session_in_sync =
        outcome == ChatOutcomeKind::Answered && !repaired && !followed_up && !s.retracted && !stale;

    let text = paragraphs
        .iter()
        .map(|(t, _)| t.as_str())
        .collect::<Vec<_>>()
        .join("\n\n");
    let mut segments = Vec::new();
    for (i, (_, segs)) in paragraphs.into_iter().enumerate() {
        if i > 0 {
            push_segments(
                &mut segments,
                vec![Segment::Text {
                    text: "\n\n".to_owned(),
                }],
            );
        }
        push_segments(&mut segments, segs);
    }
    let engine_evals = lock(&state).evals.clone();
    Ok(ChatTurnOutcome {
        new_messages: vec![
            question,
            Message {
                role: Role::Assistant,
                content: vec![text_json(text.clone())],
            },
        ],
        text,
        segments,
        citations,
        dropped,
        replaced: stale,
        calls,
        outcome,
        engine_evals,
        session_model_usage,
        session: session_path,
        session_in_sync,
        next_call_index: session.next_call_index(),
    })
}

fn fixed(text: &str) -> (String, Vec<Segment>) {
    (
        text.to_owned(),
        vec![Segment::Text {
            text: text.to_owned(),
        }],
    )
}

//! A scripted [`AiClient`] for tests (port of `fake.ts`, extended to the sidecar contract of
//! ADR 0062 so the generation and chat tasks test against the real semantics):
//!
//! - structured and batch outputs go through [`parse_model_output`] with the request's schema,
//!   so refusal, `max_tokens`, invalid JSON and schema mismatch are scriptable, and
//!   [`FakeStructured::structured_output_failed`] models the SDK giving up after its
//!   structured-output retries;
//! - a turn runs its tool loop inside the call: the fake awaits the passed [`ToolHandler`] for
//!   each scripted tool call, in order, then streams the deltas, then `Done`;
//! - per-turn `model_usage` is separate from the SDK's running `session_model_usage`, which the
//!   fake keeps per conversation (new, resumed or rebuilt), so [`metered_turn_usage`] works on it;
//! - streamed text can be retracted (a superseded message, a refusal);
//! - a second concurrent turn of one conversation gets 409 `turn_in_progress`, and a resume
//!   without rebuild context gets the client's local 400, as the real client does;
//! - disabled batches give `Unavailable(NoApiKey)`, like `AiRuntime` without an API key.
//!
//! Mistakes in the script (no script left, tool calls without a handler) fail loudly with an
//! [`AiError::Response`] whose message starts with `fake:`.
//!
//! [`metered_turn_usage`]: super::port::metered_turn_usage

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::fmt;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use futures::future::BoxFuture;
use futures::stream::{self, BoxStream, StreamExt};
use serde_json::{Value, json};
use uuid::Uuid;

use super::errors::classify_batch_error_type;
use super::parse::parse_model_output;
use super::port::{
    AiClient, AiError, AiUsage, BatchItem, BatchOutcome, BatchResultItem, BatchStatus,
    ChatTurnRequest, ModelUsage, OutputSchema, ParseError, SchemaResolver, SessionPath,
    StructuredRequest, StructuredResult, ToolChoice, ToolHandler, ToolReply, ToolUse, TurnEvent,
    TurnResult, UnavailableReason, sum_usage,
};

/// A scripted structured output. `text` wins over `parsed`, which is shorthand for its JSON text;
/// with neither there is no text block. `stop_reason` defaults to `end_turn`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FakeStructured {
    pub parsed: Option<Value>,
    pub text: Option<String>,
    pub stop_reason: Option<String>,
    /// Used only when `model_usage` is `None`.
    pub usage: AiUsage,
    /// `None` = `[(requested model, usage)]`. The result's `usage` is always the sum.
    pub model_usage: Option<Vec<ModelUsage>>,
    /// The SDK's `error_max_structured_output_retries`: `SchemaMismatch` without issues (after a
    /// refusal or `max_tokens`, which win, as in the sidecar client).
    pub structured_output_failed: bool,
}

/// Text that was streamed as its own API message and then withdrawn by a `Retract`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FakeRetracted {
    pub deltas: Vec<String>,
    /// `superseded`, `model_refusal_fallback` or `refusal`.
    pub reason: String,
}

/// A scripted chat turn. In order: the tool calls (through the handler), the `retracted`
/// message and its `Retract`, the deltas, then `Done` (or `error`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FakeTurn {
    /// `(name, input)`; each name must be one of the request's tools.
    pub tool_calls: Vec<(String, Value)>,
    pub deltas: Vec<String>,
    /// Defaults to `end_turn`. With `refusal`, the streamed deltas are retracted (reason
    /// `refusal`) and `Done` has no content.
    pub stop_reason: Option<String>,
    /// Used only when `model_usage` is `None`.
    pub usage: AiUsage,
    /// This turn's own usage; `None` = `[(requested model, usage)]`.
    pub model_usage: Option<Vec<ModelUsage>>,
    pub retracted: Option<FakeRetracted>,
    /// A terminal `error` event after the deltas, instead of `Done`.
    pub error: Option<AiError>,
    /// Overrides the session path (default: `New`, or `Resumed` when the fake holds a session
    /// for the conversation, else `Rebuilt`).
    pub session: Option<SessionPath>,
    /// Overrides the running total (default: the previous total plus this turn for `Resumed`,
    /// this turn alone otherwise). It is stored as the conversation's new total.
    pub session_model_usage: Option<Vec<ModelUsage>>,
}

/// A scripted batch result line.
#[derive(Clone, Debug, PartialEq)]
pub struct FakeBatchItem {
    pub custom_id: String,
    pub outcome: BatchOutcome,
    /// The model text of a succeeded item, parsed with the resolver's schema.
    pub text: Option<String>,
    pub stop_reason: Option<String>,
    /// For errored items.
    pub error_type: Option<String>,
    pub usage: AiUsage,
    /// The response's model; `None` = the model submitted for this custom id, else `fake`.
    pub model: Option<String>,
}

impl Default for FakeBatchItem {
    fn default() -> Self {
        FakeBatchItem {
            custom_id: String::new(),
            outcome: BatchOutcome::Succeeded,
            text: None,
            stop_reason: None,
            error_type: None,
            usage: AiUsage::default(),
            model: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FakeBatch {
    pub status: BatchStatus,
    pub results: Vec<FakeBatchItem>,
}

pub type StructuredScript =
    Box<dyn Fn(&StructuredRequest, usize) -> Result<FakeStructured, AiError> + Send + Sync>;
pub type TurnScript =
    Box<dyn Fn(&ChatTurnRequest, usize) -> Result<FakeTurn, AiError> + Send + Sync>;
/// Runs before a submit is accepted; an `Err` is returned instead (nothing is created).
pub type SubmitHook = Box<dyn Fn(&[BatchItem], usize) -> Result<(), AiError> + Send + Sync>;
/// Runs before a status lookup; an `Err` is returned instead.
pub type StatusHook = Box<dyn Fn(&str, usize) -> Result<(), AiError> + Send + Sync>;

pub struct FakeScript {
    /// Called with the request and the 0-based structured call number.
    pub structured: Option<StructuredScript>,
    /// One script per turn, by 0-based turn number. Turns rejected before the model runs (409,
    /// the local 400) consume none.
    pub turns: Vec<TurnScript>,
    /// By batch id; submitted batches are `fakebatch_<n>` (1-based).
    pub batches: HashMap<String, FakeBatch>,
    /// Awaited inside every call except `batch_results` (use paused tokio time in tests).
    pub delay: Option<Duration>,
    /// Default true. False makes the batch calls `Unavailable(NoApiKey)`.
    pub batches_enabled: bool,
    pub batch_submit: Option<SubmitHook>,
    pub batch_status: Option<StatusHook>,
}

impl Default for FakeScript {
    fn default() -> Self {
        FakeScript {
            structured: None,
            turns: Vec::new(),
            batches: HashMap::new(),
            delay: None,
            batches_enabled: true,
            batch_submit: None,
            batch_status: None,
        }
    }
}

impl fmt::Debug for FakeScript {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FakeScript")
            .field("structured", &self.structured.is_some())
            .field("turns", &self.turns.len())
            .field("batches", &self.batches)
            .field("delay", &self.delay)
            .field("batches_enabled", &self.batches_enabled)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FakeCallKind {
    Structured,
    Turn,
    BatchSubmit,
    BatchStatus,
    BatchResults,
}

/// One recorded call. `tool_replies` holds a turn's `(name, input, reply)` in call order.
#[derive(Clone, Debug, PartialEq)]
pub struct FakeCall {
    pub kind: FakeCallKind,
    pub request_json: Value,
    pub tool_replies: Vec<(String, Value, ToolReply)>,
}

#[derive(Default)]
struct State {
    calls: Vec<FakeCall>,
    structured_calls: usize,
    turn_calls: usize,
    submit_calls: usize,
    status_calls: usize,
    batch_counter: usize,
    tool_use_counter: usize,
    /// custom id → requested model, from submitted batches.
    submitted_models: HashMap<String, String>,
    /// conversation id → the SDK's running session total.
    sessions: HashMap<Uuid, Vec<ModelUsage>>,
    active_turns: HashSet<Uuid>,
}

pub struct FakeAiClient {
    script: FakeScript,
    state: Mutex<State>,
    in_flight: AtomicUsize,
    max_in_flight: AtomicUsize,
}

impl fmt::Debug for FakeAiClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FakeAiClient")
            .field("script", &self.script)
            .finish_non_exhaustive()
    }
}

fn loud(msg: impl Into<String>) -> AiError {
    AiError::Response(format!("fake: {}", msg.into()))
}

fn no_api_key() -> AiError {
    AiError::Unavailable {
        reason: UnavailableReason::NoApiKey,
        retry_after: None,
    }
}

fn not_found() -> AiError {
    AiError::Request {
        status: 404,
        api_error_type: Some("not_found_error".to_owned()),
    }
}

fn text_content(text: &str) -> Vec<Value> {
    if text.is_empty() {
        Vec::new()
    } else {
        vec![json!({"type": "text", "text": text})]
    }
}

fn default_usage(
    model_usage: Option<Vec<ModelUsage>>,
    model: &str,
    usage: AiUsage,
) -> Vec<ModelUsage> {
    model_usage.unwrap_or_else(|| {
        vec![ModelUsage {
            model: model.to_owned(),
            usage,
        }]
    })
}

/// The counter-wise sum per model, sorted by model.
fn merge_usage(a: &[ModelUsage], b: &[ModelUsage]) -> Vec<ModelUsage> {
    let mut m: BTreeMap<&str, AiUsage> = BTreeMap::new();
    for e in a.iter().chain(b) {
        let u = m.entry(e.model.as_str()).or_default();
        *u = u.plus(e.usage);
    }
    m.into_iter()
        .map(|(model, usage)| ModelUsage {
            model: model.to_owned(),
            usage,
        })
        .collect()
}

fn structured_result(
    schema: Option<&OutputSchema>,
    model: &str,
    out: FakeStructured,
) -> StructuredResult {
    let text = out.text.or_else(|| out.parsed.map(|v| v.to_string()));
    let content = text.as_deref().map(text_content).unwrap_or_default();
    let stop_reason = out.stop_reason.unwrap_or_else(|| "end_turn".to_owned());
    let (parsed, parse_error, parse_issues) = match stop_reason.as_str() {
        "refusal" | "max_tokens" => parse_model_output(&content, Some(&stop_reason), schema),
        _ if out.structured_output_failed => (None, Some(ParseError::SchemaMismatch), None),
        _ => parse_model_output(&content, Some(&stop_reason), schema),
    };
    let model_usage = default_usage(out.model_usage, model, out.usage);
    StructuredResult {
        parsed,
        parse_error,
        parse_issues,
        content,
        stop_reason: Some(stop_reason),
        usage: sum_usage(&model_usage),
        model: model.to_owned(),
        request_id: None,
        model_usage,
        sdk_cost_usd: None,
    }
}

fn structured_json(req: &StructuredRequest) -> Value {
    json!({
        "model": req.model,
        "system": req.system,
        "messages": req.messages,
        "max_tokens": req.max_tokens,
        "effort": req.effort,
        "schema": { "id": req.schema.id, "version": req.schema.version },
    })
}

fn turn_json(req: &ChatTurnRequest) -> Value {
    let tools: Vec<Value> = req
        .tools
        .iter()
        .map(|t| json!({"name": t.name, "description": t.description, "input_schema": t.input_schema}))
        .collect();
    json!({
        "model": req.model,
        "system": req.system,
        "tools": tools,
        "tool_choice": req.tool_choice,
        "messages": req.messages,
        "max_tokens": req.max_tokens,
        "effort": req.effort,
        "max_turns": req.max_turns,
        "conversation_id": req.conversation_id,
        "resume": req.resume,
        "rebuild_context": req.rebuild_context,
    })
}

/// Counts a call as in flight until dropped.
struct InFlight<'a>(&'a FakeAiClient);

impl Drop for InFlight<'_> {
    fn drop(&mut self) {
        self.0.in_flight.fetch_sub(1, Ordering::SeqCst);
    }
}

/// The per-conversation turn lock, released on drop.
struct TurnLock<'a> {
    client: &'a FakeAiClient,
    conversation: Uuid,
}

impl Drop for TurnLock<'_> {
    fn drop(&mut self) {
        self.client.lock().active_turns.remove(&self.conversation);
    }
}

/// A started turn: its queued events, and the guards held until the terminal event.
struct TurnRun<'a> {
    events: VecDeque<Result<TurnEvent, AiError>>,
    guards: Option<(InFlight<'a>, TurnLock<'a>)>,
}

impl<'a> TurnRun<'a> {
    fn into_stream(self) -> BoxStream<'a, Result<TurnEvent, AiError>> {
        stream::unfold(self, |mut r| async move {
            let item = r.events.pop_front()?;
            if r.events.is_empty() {
                r.guards = None;
            }
            Some((item, r))
        })
        .boxed()
    }
}

impl FakeAiClient {
    pub fn new(script: FakeScript) -> Self {
        FakeAiClient {
            script,
            state: Mutex::new(State::default()),
            in_flight: AtomicUsize::new(0),
            max_in_flight: AtomicUsize::new(0),
        }
    }

    /// Every call so far, in order.
    pub fn calls(&self) -> Vec<FakeCall> {
        self.lock().calls.clone()
    }

    /// The most calls that were in flight at once.
    pub fn max_in_flight(&self) -> usize {
        self.max_in_flight.load(Ordering::SeqCst)
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn record(&self, kind: FakeCallKind, request_json: Value) -> usize {
        let mut s = self.lock();
        s.calls.push(FakeCall {
            kind,
            request_json,
            tool_replies: Vec::new(),
        });
        s.calls.len() - 1
    }

    fn enter(&self) -> InFlight<'_> {
        let now = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        self.max_in_flight.fetch_max(now, Ordering::SeqCst);
        InFlight(self)
    }

    async fn pause(&self) {
        if let Some(d) = self.script.delay {
            tokio::time::sleep(d).await;
        }
    }

    async fn start_turn<'a>(
        &'a self,
        req: &'a ChatTurnRequest,
        tools: Option<Arc<dyn ToolHandler>>,
    ) -> Result<TurnRun<'a>, AiError> {
        let idx = self.record(FakeCallKind::Turn, turn_json(req));
        let in_flight = self.enter();
        if req.resume && req.rebuild_context.as_deref().is_none_or(str::is_empty) {
            return Err(AiError::Request {
                status: 400,
                api_error_type: Some("invalid_request_error".to_owned()),
            });
        }
        let conversation = req.conversation_id;
        let n = {
            let mut s = self.lock();
            if !s.active_turns.insert(conversation) {
                return Err(AiError::Request {
                    status: 409,
                    api_error_type: Some("turn_in_progress".to_owned()),
                });
            }
            let n = s.turn_calls;
            s.turn_calls += 1;
            n
        };
        let turn_lock = TurnLock {
            client: self,
            conversation,
        };
        let script = self
            .script
            .turns
            .get(n)
            .ok_or_else(|| loud(format!("no script for turn {n}")))?;
        let turn = script(req, n)?;

        let mut tool_uses = Vec::new();
        if !turn.tool_calls.is_empty() {
            let handler = match (&tools, req.tool_choice) {
                (Some(h), ToolChoice::Auto) => h.clone(),
                (None, _) => return Err(loud("scripted tool calls but no tool handler")),
                (Some(_), ToolChoice::None) => {
                    return Err(loud("scripted tool calls with tool_choice none"));
                }
            };
            for (name, _) in &turn.tool_calls {
                if !req.tools.iter().any(|t| t.name == name) {
                    return Err(loud(format!("tool {name:?} was not offered")));
                }
            }
            for (name, input) in &turn.tool_calls {
                let reply = handler.call(name, input.clone()).await;
                let mut s = self.lock();
                s.tool_use_counter += 1;
                let id = format!("fake_tu_{}", s.tool_use_counter);
                s.calls[idx]
                    .tool_replies
                    .push((name.clone(), input.clone(), reply));
                tool_uses.push(ToolUse {
                    id,
                    name: name.clone(),
                    input: Some(input.clone()),
                });
            }
        }
        self.pause().await;

        let mut events = VecDeque::new();
        let mut k = 0;
        let mut message = |events: &mut VecDeque<_>, deltas: &[String]| -> Option<String> {
            if deltas.is_empty() {
                return None;
            }
            let id = format!("fake_msg_{}_{k}", n + 1);
            k += 1;
            events.push_back(Ok(TurnEvent::Message(id.clone())));
            for d in deltas {
                events.push_back(Ok(TurnEvent::TextDelta(d.clone())));
            }
            Some(id)
        };
        if let Some(r) = &turn.retracted
            && let Some(id) = message(&mut events, &r.deltas)
        {
            events.push_back(Ok(TurnEvent::Retract {
                message_ids: vec![id],
                reason: r.reason.clone(),
            }));
        }
        let main = message(&mut events, &turn.deltas);
        let stop_reason = turn.stop_reason.unwrap_or_else(|| "end_turn".to_owned());
        let refused = stop_reason == "refusal";
        if refused && let Some(id) = main {
            events.push_back(Ok(TurnEvent::Retract {
                message_ids: vec![id],
                reason: "refusal".to_owned(),
            }));
        }
        if let Some(e) = turn.error {
            events.push_back(Err(e));
        } else {
            let model = req.model.as_str();
            let model_usage = default_usage(turn.model_usage, model, turn.usage);
            let mut s = self.lock();
            let previous = s.sessions.get(&conversation);
            let session = turn.session.unwrap_or(match (req.resume, previous) {
                (false, _) => SessionPath::New,
                (true, Some(_)) => SessionPath::Resumed,
                (true, None) => SessionPath::Rebuilt,
            });
            let total = turn
                .session_model_usage
                .unwrap_or_else(|| match (session, previous) {
                    (SessionPath::Resumed, Some(prev)) => merge_usage(prev, &model_usage),
                    _ => merge_usage(&[], &model_usage),
                });
            s.sessions.insert(conversation, total.clone());
            let text = if refused {
                String::new()
            } else {
                turn.deltas.concat()
            };
            events.push_back(Ok(TurnEvent::Done(TurnResult {
                content: text_content(&text),
                stop_reason: Some(stop_reason),
                tool_uses,
                usage: sum_usage(&model_usage),
                model: model.to_owned(),
                model_usage,
                sdk_cost_usd: None,
                session_model_usage: total,
                session: Some(session),
            })));
        }
        Ok(TurnRun {
            events,
            guards: Some((in_flight, turn_lock)),
        })
    }
}

impl AiClient for FakeAiClient {
    fn structured<'a>(
        &'a self,
        req: &'a StructuredRequest,
    ) -> BoxFuture<'a, Result<StructuredResult, AiError>> {
        Box::pin(async move {
            self.record(FakeCallKind::Structured, structured_json(req));
            let _in_flight = self.enter();
            self.pause().await;
            let script = self
                .script
                .structured
                .as_ref()
                .ok_or_else(|| loud("no structured script"))?;
            let n = {
                let mut s = self.lock();
                s.structured_calls += 1;
                s.structured_calls - 1
            };
            let out = script(req, n)?;
            Ok(structured_result(Some(req.schema), req.model.as_str(), out))
        })
    }

    fn stream_turn<'a>(
        &'a self,
        req: &'a ChatTurnRequest,
        tools: Option<Arc<dyn ToolHandler>>,
    ) -> BoxStream<'a, Result<TurnEvent, AiError>> {
        stream::once(self.start_turn(req, tools))
            .flat_map(|started| match started {
                Ok(run) => run.into_stream(),
                Err(e) => stream::once(async move { Err(e) }).boxed(),
            })
            .boxed()
    }

    fn batches_enabled(&self) -> bool {
        self.script.batches_enabled
    }

    fn batch_submit<'a>(
        &'a self,
        items: &'a [BatchItem],
    ) -> BoxFuture<'a, Result<String, AiError>> {
        Box::pin(async move {
            let request: Vec<Value> = items
                .iter()
                .map(|i| json!({"custom_id": i.custom_id, "request": structured_json(&i.req)}))
                .collect();
            self.record(FakeCallKind::BatchSubmit, Value::Array(request));
            let _in_flight = self.enter();
            self.pause().await;
            if !self.script.batches_enabled {
                return Err(no_api_key());
            }
            let n = {
                let mut s = self.lock();
                s.submit_calls += 1;
                s.submit_calls - 1
            };
            if let Some(hook) = &self.script.batch_submit {
                hook(items, n)?;
            }
            let mut s = self.lock();
            s.batch_counter += 1;
            for i in items {
                s.submitted_models
                    .insert(i.custom_id.clone(), i.req.model.as_str().to_owned());
            }
            Ok(format!("fakebatch_{}", s.batch_counter))
        })
    }

    fn batch_status<'a>(&'a self, id: &'a str) -> BoxFuture<'a, Result<BatchStatus, AiError>> {
        Box::pin(async move {
            self.record(FakeCallKind::BatchStatus, json!({ "id": id }));
            let _in_flight = self.enter();
            self.pause().await;
            if !self.script.batches_enabled {
                return Err(no_api_key());
            }
            let n = {
                let mut s = self.lock();
                s.status_calls += 1;
                s.status_calls - 1
            };
            if let Some(hook) = &self.script.batch_status {
                hook(id, n)?;
            }
            self.script
                .batches
                .get(id)
                .map(|b| b.status)
                .ok_or_else(not_found)
        })
    }

    fn batch_results<'a>(
        &'a self,
        id: &'a str,
        resolver: &'a dyn SchemaResolver,
    ) -> BoxStream<'a, Result<BatchResultItem, AiError>> {
        self.record(FakeCallKind::BatchResults, json!({ "id": id }));
        if !self.script.batches_enabled {
            return stream::once(async { Err(no_api_key()) }).boxed();
        }
        let Some(batch) = self.script.batches.get(id) else {
            return stream::once(async { Err(not_found()) }).boxed();
        };
        stream::iter(
            batch
                .results
                .iter()
                .map(move |i| Ok(self.result_item(i, resolver))),
        )
        .boxed()
    }
}

impl FakeAiClient {
    fn result_item(&self, i: &FakeBatchItem, resolver: &dyn SchemaResolver) -> BatchResultItem {
        if i.outcome != BatchOutcome::Succeeded {
            return BatchResultItem {
                custom_id: i.custom_id.clone(),
                outcome: i.outcome,
                result: None,
                error_type: i.error_type.clone(),
                error_kind: (i.outcome == BatchOutcome::Errored)
                    .then(|| classify_batch_error_type(i.error_type.as_deref())),
                parse_error: None,
            };
        }
        let model = i.model.clone().unwrap_or_else(|| {
            self.lock()
                .submitted_models
                .get(&i.custom_id)
                .cloned()
                .unwrap_or_else(|| "fake".to_owned())
        });
        let result = structured_result(
            resolver.resolve(&i.custom_id),
            &model,
            FakeStructured {
                text: i.text.clone(),
                stop_reason: i.stop_reason.clone(),
                usage: i.usage,
                ..FakeStructured::default()
            },
        );
        BatchResultItem {
            custom_id: i.custom_id.clone(),
            outcome: BatchOutcome::Succeeded,
            error_type: None,
            error_kind: None,
            parse_error: result.parse_error,
            result: Some(result),
        }
    }
}

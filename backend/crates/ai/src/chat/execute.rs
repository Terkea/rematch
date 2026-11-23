//! Runs one chat tool call. Tool inputs are untrusted model output: every field is validated
//! here (sizes, counts, ranges, charsets) before anything is read, and nothing panics.
//!
//! - Every id goes to the account-scoped [`CoachDataSource`]; an id of another account is not
//!   found. The executor also rejects a returned record that is not the one it asked for, so a
//!   faulty source cannot leak another position into the evidence.
//! - Each call has two phases: an async fetch that never touches the evidence, then a short
//!   synchronous write under the [`ToolSession`] lock (no lock is held across an await, so
//!   parallel tool calls and their engine waits overlap).
//! - The session assigns call indexes itself (a monotonic counter), so number and date paths are
//!   `tools.<index>.<field>`. A `{{num:}}` or `{{date:}}` token is emitted only when the value
//!   stored at its path is this call's value: a stale value is never made citable.
//! - Results are JSON with `refs`: ready-made tokens that resolve against what the call adds
//!   (positions, opening lines, paths, and `{{eng:}}` results). Only engine results this
//!   account's engine returned become `{{eng:}}` evidence.
//! - Evidence rules (Task 7): a position already in the evidence keeps its FEN, moves and lines
//!   (new allowed moves are merged in); an opening line already indexed from the report is not
//!   replaced; number and date paths keep their first value.
//! - Errors never echo model input (only a validated UCI move in `illegal move <uci>`) and never
//!   carry an internal error message; the error chain is logged server-side only.
//! - At most [`MAX_ENGINE_CHECKS_PER_TURN`] engine checks run per turn, before the account's
//!   quota (Task 24) is even consulted.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use chess_core::Color;
use chess_core::board::Position;
use chess_core::fen::fen_key;
use futures::future::BoxFuture;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value, json};
use time::Date;
use time::macros::format_description;

use super::data_source::{
    CoachDataSource, EngineCheck, ExampleSort, ExamplesQuery, FilterCounts, GameResult,
    GameSummary, GamesQuery, InteractiveEngine, Judgement, LOSING_TO, MoveFilter, Phase,
    PositionRecord, Standing, WINNING_FROM,
};
use super::tools::{
    MAX_CLOCK_SECONDS, MAX_EXAMPLES, MAX_FEN_CHARS, MAX_GAMES, MAX_ID_CHARS, MAX_LAST_GAMES,
    MAX_MOVES, MAX_PLY,
};
use crate::entitlements::QuotaWindow;
use crate::input::types::{ExemplarPosition, MoveRef, PlayerColor};
use crate::validator::evidence::{
    EngineEvidence, Evidence, PositionArgs, PvFrom, PvSides, iso_prefix,
};
use crate::validator::pv::{fen_after, is_uci, pv_to_san, strip_san};

/// How long `analyse_position` waits for the engine before it reports `EnginePending`.
/// (Task 23's `limits.rs` re-exports this value with `ENGINE_WAIT_MAX`.)
pub const ENGINE_WAIT_INLINE: Duration = Duration::from_secs(5);

/// Engine checks one chat turn may start, whatever the model asks for. Further calls get
/// `{"available":false,"reason":"per_turn"}` without reaching the engine or the quota.
pub const MAX_ENGINE_CHECKS_PER_TURN: u32 = 2;

/// Engine lines kept per result (MultiPV 3).
const MAX_ENGINE_LINES: usize = 3;
/// SAN moves kept per engine line.
const MAX_LINE_MOVES: usize = 64;

/// A tool result for the model. `content` is JSON.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolOutput {
    pub content: String,
    pub is_error: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ToolExec {
    Ready(ToolOutput),
    /// `analyse_position` is still running: the caller waits (`InteractiveEngine::wait`) and
    /// renders the result with [`engine_result_output`], or abandons it.
    EnginePending {
        eval_id: String,
    },
}

/// What one turn's tools read from, both scoped to the conversation's account.
#[derive(Clone, Copy)]
pub struct ToolDeps<'a> {
    pub ds: &'a dyn CoachDataSource,
    pub engine: &'a dyn InteractiveEngine,
}

struct SessionState {
    ev: Evidence,
    next_call: u32,
}

/// One turn's tool state: the evidence the tools add to, the call-index counter and the
/// per-turn engine-check count. Shared by reference (or `Arc`) between parallel tool calls; the
/// lock is only taken for synchronous evidence writes.
pub struct ToolSession {
    state: Mutex<SessionState>,
    engine_checks: AtomicU32,
}

impl ToolSession {
    /// Starts a turn over `ev` (the report evidence plus anchors). Call indexes start at 0.
    pub fn new(ev: Evidence) -> Self {
        Self::starting_at(ev, 0)
    }

    /// Starts a turn whose call indexes begin at `first_call` (Task 23 fix round 1A): a chat
    /// continues the conversation's counter, so a `tools.<k>` path of an earlier turn is never
    /// reused for a different result and an old token fails closed.
    pub fn starting_at(ev: Evidence, first_call: u32) -> Self {
        ToolSession {
            state: Mutex::new(SessionState {
                ev,
                next_call: first_call,
            }),
            engine_checks: AtomicU32::new(0),
        }
    }

    /// The index the next call would get (store it for the conversation's next turn).
    pub fn next_call_index(&self) -> u32 {
        self.lock().next_call
    }

    fn lock(&self) -> MutexGuard<'_, SessionState> {
        // A panic elsewhere never makes the evidence unusable: take it as it is.
        self.state.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Reads the evidence (for paragraph validation).
    pub fn with_evidence<R>(&self, f: impl FnOnce(&Evidence) -> R) -> R {
        f(&self.lock().ev)
    }

    /// A copy of the evidence as it is now.
    pub fn evidence(&self) -> Evidence {
        self.lock().ev.clone()
    }

    pub fn into_evidence(self) -> Evidence {
        self.state
            .into_inner()
            .unwrap_or_else(|p| p.into_inner())
            .ev
    }

    /// Engine checks started this turn.
    pub fn engine_checks(&self) -> u32 {
        self.engine_checks.load(Ordering::SeqCst)
    }

    /// Takes one of the turn's engine checks, or `false` when they are used up.
    fn reserve_engine_check(&self) -> bool {
        self.engine_checks
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |c| {
                (c < MAX_ENGINE_CHECKS_PER_TURN).then_some(c + 1)
            })
            .is_ok()
    }

    /// The write phase of one call: a fresh call index and the evidence, under the lock.
    fn write<R>(&self, f: impl FnOnce(&mut Evidence, u32) -> R) -> R {
        let mut st = self.lock();
        let k = st.next_call;
        st.next_call = st.next_call.saturating_add(1);
        f(&mut st.ev, k)
    }
}

/// The executor behind a trait, so the turn's tool handler (Task 23 wraps it in T11's
/// `ToolHandler`) can hold borrowed ([`ToolDeps`]) or owned ([`OwnedToolDeps`]) dependencies.
pub trait ToolExecutor: Send + Sync {
    fn execute<'a>(
        &'a self,
        session: &'a ToolSession,
        name: &'a str,
        input: Option<&'a Value>,
    ) -> BoxFuture<'a, ToolExec>;
}

impl ToolExecutor for ToolDeps<'_> {
    fn execute<'a>(
        &'a self,
        session: &'a ToolSession,
        name: &'a str,
        input: Option<&'a Value>,
    ) -> BoxFuture<'a, ToolExec> {
        execute_tool(self, session, name, input)
    }
}

/// [`ToolDeps`] with owned handles, for a `'static` tool handler.
#[derive(Clone)]
pub struct OwnedToolDeps {
    pub ds: Arc<dyn CoachDataSource>,
    pub engine: Arc<dyn InteractiveEngine>,
}

impl ToolExecutor for OwnedToolDeps {
    fn execute<'a>(
        &'a self,
        session: &'a ToolSession,
        name: &'a str,
        input: Option<&'a Value>,
    ) -> BoxFuture<'a, ToolExec> {
        Box::pin(async move {
            let deps = ToolDeps {
                ds: self.ds.as_ref(),
                engine: self.engine.as_ref(),
            };
            run(&deps, session, name, input).await
        })
    }
}

/// Runs the tool `name` with the model's `input` (`None`: the input could not be parsed).
pub fn execute_tool<'a, 'd: 'a>(
    deps: &'a ToolDeps<'d>,
    session: &'a ToolSession,
    name: &'a str,
    input: Option<&'a Value>,
) -> BoxFuture<'a, ToolExec> {
    Box::pin(run(deps, session, name, input))
}

/// Renders an engine-check outcome after a pending wait. A still-pending check is `engine
/// busy` (the caller abandons it).
pub fn engine_result_output(session: &ToolSession, check: EngineCheck) -> ToolOutput {
    match check {
        EngineCheck::Done { eval_id, evidence } => {
            session.write(|ev, _| engine_done(ev, &eval_id, evidence))
        }
        EngineCheck::Pending { .. } => ok(json!({"available": false, "reason": "engine busy"})),
        EngineCheck::Limit { window, .. } => engine_limit(window),
        EngineCheck::Unavailable { .. } => engine_unavailable(),
    }
}

// --- Outputs ---

type Out = Result<Value, String>;

fn ok(v: Value) -> ToolOutput {
    ToolOutput {
        content: v.to_string(),
        is_error: false,
    }
}

fn err(message: &str) -> ToolOutput {
    ToolOutput {
        content: json!({ "error": message }).to_string(),
        is_error: true,
    }
}

fn not_found() -> Value {
    json!({"found": false})
}

fn engine_unavailable() -> ToolOutput {
    ok(json!({"available": false, "reason": "engine unavailable"}))
}

fn engine_limit(window: QuotaWindow) -> ToolOutput {
    let window = match window {
        QuotaWindow::Day => "day",
        QuotaWindow::Week => "week",
        QuotaWindow::Month => "month",
    };
    ok(json!({"available": false, "reason": "limit", "window": window}))
}

/// A data-source failure: the chain is logged server-side; the model sees only "tool
/// unavailable".
fn source_failed(tool: &'static str, e: &anyhow::Error) -> String {
    tracing::warn!(tool, error = %format!("{e:#}"), "chat tool data source failed");
    "tool unavailable".to_owned()
}

async fn run(deps: &ToolDeps<'_>, s: &ToolSession, name: &str, input: Option<&Value>) -> ToolExec {
    let Some(input) = input else {
        return ToolExec::Ready(err("input could not be parsed"));
    };
    let result = match name {
        "analyse_position" => return analyse_position(deps, s, input).await,
        "find_examples" => find_examples(deps, s, input).await,
        "get_eval" => get_eval(deps, s, input).await,
        "get_games" => get_games(deps, s, input).await,
        "get_opening_line" => get_opening_line(deps, s, input).await,
        "get_position" => get_position(deps, s, input).await,
        "get_weakness" => get_weakness(deps, s, input).await,
        _ => Err("unknown tool".to_owned()),
    };
    ToolExec::Ready(match result {
        Ok(v) => ok(v),
        Err(e) => err(&e),
    })
}

// --- Input parsing and validation ---

/// Strict deserialisation of an object with exactly the keys in `fields` (each may be absent
/// if optional). An unexpected key is reported with a fixed message, never by name; a bad
/// value names only its known field, never the value.
fn parse<T: DeserializeOwned>(input: &Value, fields: &[&str]) -> Result<T, String> {
    // Derived structs also accept a JSON array of the fields: only an object is an input.
    let Some(o) = input.as_object() else {
        return Err("invalid input".to_owned());
    };
    if o.keys().any(|k| !fields.contains(&k.as_str())) {
        return Err("invalid input: unexpected field".to_owned());
    }
    serde_path_to_error::deserialize::<_, T>(input).map_err(|e| {
        let path = e.path().to_string();
        match fields.iter().find(|f| **f == path) {
            Some(f) => format!("invalid input at {f}"),
            None => "invalid input".to_owned(),
        }
    })
}

/// The token grammar's game-id charset (`[A-Za-z0-9_.-]`).
fn is_id_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-')
}

/// The token grammar's line-key charset (`[A-Za-z0-9_.:-]`).
fn is_line_char(c: char) -> bool {
    is_id_char(c) || c == ':'
}

fn check_id(field: &str, s: &str) -> Result<(), String> {
    if s.is_empty() || s.len() > MAX_ID_CHARS || !s.chars().all(is_id_char) {
        return Err(format!(
            "invalid input: {field} must be 1 to {MAX_ID_CHARS} characters of letters, digits and _ . -"
        ));
    }
    Ok(())
}

fn check_line_key(s: &str) -> Result<(), String> {
    if s.is_empty() || s.len() > MAX_ID_CHARS || !s.chars().all(is_line_char) {
        return Err(format!(
            "invalid input: lineKey must be 1 to {MAX_ID_CHARS} characters of letters, digits and _ . : -"
        ));
    }
    Ok(())
}

fn check_ply(ply: u32) -> Result<(), String> {
    if (1..=MAX_PLY).contains(&ply) {
        Ok(())
    } else {
        Err(format!("invalid input: ply must be 1 to {MAX_PLY}"))
    }
}

fn check_limit(limit: u32, max: u32) -> Result<(), String> {
    if (1..=max).contains(&limit) {
        Ok(())
    } else {
        Err(format!("invalid input: limit must be 1 to {max}"))
    }
}

fn check_moves(moves: &[String]) -> Result<(), String> {
    if moves.len() > MAX_MOVES {
        return Err(format!(
            "invalid input: moves allows at most {MAX_MOVES} moves"
        ));
    }
    if !moves.iter().all(|m| is_uci(m)) {
        return Err("invalid input: moves must be UCI moves such as e2e4".to_owned());
    }
    Ok(())
}

/// Plays validated UCI moves from `fen`. `Err` is the tool error.
fn replay(fen: &str, moves: &[String]) -> Result<String, String> {
    let mut cur = fen.to_owned();
    for m in moves {
        cur = fen_after(&cur, m).ok_or_else(|| format!("illegal move {m}"))?;
    }
    Ok(cur)
}

// --- Evidence writers (the synchronous phase) ---

/// Keys whose values are moves, lines or positions: never number or date paths.
const SKIP_KEYS: &[&str] = &[
    "fen",
    "fenAfterPlayed",
    "pv",
    "bestPv",
    "refutationPv",
    "bestPvSan",
    "movesSan",
    "tags",
    "played",
    "best",
    "exemplars",
    "examples",
    "refs",
];

type Refs = Map<String, Value>;

/// Number and date paths under `base` for every number and ISO date in `v` (plus the derived
/// `moveNumber` of a `ply` and `pawns` of a `cp`, as the report's path index has). Returns the
/// ready-made `{{num:}}` and `{{date:}}` tokens keyed by their path relative to `base`, only for
/// paths that hold this call's value.
fn index_paths(ev: &mut Evidence, base: &str, v: &Value) -> (Refs, Refs) {
    let mut nums = Map::new();
    let mut dates = Map::new();
    walk(ev, base, "", v, &mut nums, &mut dates);
    (nums, dates)
}

fn put_num(ev: &mut Evidence, nums: &mut Refs, base: &str, rel: String, n: f64) {
    let path = format!("{base}.{rel}");
    ev.put_number_path(&path, n);
    if ev
        .number_paths
        .get(&path)
        .is_some_and(|stored| *stored == n)
    {
        nums.insert(rel, Value::String(format!("{{{{num:{path}}}}}")));
    }
}

fn put_date(ev: &mut Evidence, dates: &mut Refs, base: &str, rel: &str, d: &str) {
    let path = format!("{base}.{rel}");
    ev.put_date_path(&path, d.to_owned());
    if ev.date_paths.get(&path).is_some_and(|stored| stored == d) {
        dates.insert(
            rel.to_owned(),
            Value::String(format!("{{{{date:{path}}}}}")),
        );
    }
}

fn walk(ev: &mut Evidence, base: &str, rel: &str, v: &Value, nums: &mut Refs, dates: &mut Refs) {
    let child = |k: &str| {
        if rel.is_empty() {
            k.to_owned()
        } else {
            format!("{rel}.{k}")
        }
    };
    match v {
        Value::Object(o) => {
            for (k, x) in o {
                if SKIP_KEYS.contains(&k.as_str()) {
                    continue;
                }
                if let Some(n) = x.as_f64() {
                    put_num(ev, nums, base, child(k), n);
                    match k.as_str() {
                        "ply" => put_num(ev, nums, base, child("moveNumber"), (n / 2.0).ceil()),
                        "cp" => put_num(ev, nums, base, child("pawns"), n / 100.0),
                        _ => {}
                    }
                } else {
                    walk(ev, base, &child(k), x, nums, dates);
                }
            }
        }
        Value::Array(xs) => {
            for (i, x) in xs.iter().enumerate() {
                if let Some(n) = x.as_f64() {
                    put_num(ev, nums, base, child(&i.to_string()), n);
                } else {
                    walk(ev, base, &child(&i.to_string()), x, nums, dates);
                }
            }
        }
        Value::String(s) if !rel.is_empty() && iso_prefix(s).is_some() => {
            put_date(ev, dates, base, rel, &s[..10]);
        }
        _ => {}
    }
}

/// A position to index: its id, the FEN before `played`, and the best line from that FEN.
pub(crate) struct PosRef<'a> {
    pub game_id: &'a str,
    pub ply: u32,
    pub fen: &'a str,
    pub played: &'a MoveRef,
    pub best: Option<&'a MoveRef>,
    pub best_pv: &'a [String],
}

impl<'a> PosRef<'a> {
    pub(crate) fn of(rec: &'a PositionRecord) -> Self {
        PosRef {
            game_id: &rec.game_id,
            ply: rec.ply,
            fen: &rec.fen,
            played: &rec.played,
            best: rec.best.as_ref(),
            best_pv: &rec.pv,
        }
    }
}

/// Adds a position and returns its `move`/`best`/`pv` (and `refutation`) tokens, or `None`
/// for a record whose id or ply the token grammar cannot carry. A position already in the
/// evidence keeps its line; its tokens render the indexed position.
pub(crate) fn add_position(
    ev: &mut Evidence,
    p: PosRef<'_>,
    refutation_pv: Option<&[String]>,
) -> Option<Value> {
    let PosRef {
        game_id,
        ply,
        fen,
        played,
        best,
        best_pv,
    } = p;
    check_id("gameId", game_id).ok()?;
    check_ply(ply).ok()?;
    let key = format!("{game_id}#{ply}");
    let existed = ev.positions.contains_key(&key);
    let best_line = PvFrom {
        from_fen: fen.to_owned(),
        uci: best_pv.to_vec(),
    };
    let refutation = refutation_pv.and_then(|uci| {
        Some(PvFrom {
            from_fen: fen_after(fen, &played.uci)?,
            uci: uci.to_vec(),
        })
    });
    let mut pvs = vec![best_line.clone()];
    pvs.extend(refutation.clone());
    ev.add_position(PositionArgs {
        game_id: game_id.to_owned(),
        ply,
        fen: fen.to_owned(),
        played: Some(played.clone()),
        best: best.cloned(),
        pvs,
        pv_sides: if existed {
            PvSides::default()
        } else {
            PvSides {
                best: Some(best_line),
                refutation,
            }
        },
    });
    let pe = ev.positions.get(&key)?;
    let tokens = |side: &str, n: usize| -> Vec<String> {
        (1..=n.min(999))
            .map(|i| format!("{{{{pv:{game_id}:{ply}:{side}:{i}}}}}"))
            .collect()
    };
    let mut refs = json!({
        "move": pe.played.as_ref().map(|_| format!("{{{{move:{game_id}:{ply}}}}}")),
        "best": pe.best.as_ref().map(|_| format!("{{{{best:{game_id}:{ply}}}}}")),
        "pv": tokens("best", pe.best_pv_san.len()),
    });
    if !pe.refutation_pv_san.is_empty() {
        refs["refutation"] = json!(tokens("refutation", pe.refutation_pv_san.len()));
    }
    Some(refs)
}

/// A data-source position as JSON (with `bestPvSan`). Pure: computed in the fetch phase.
fn position_json(rec: &PositionRecord) -> Option<Value> {
    let mut position = serde_json::to_value(rec).ok()?;
    position["bestPvSan"] = json!(pv_to_san(&rec.fen, &rec.pv));
    Some(position)
}

/// Indexes a position result (`{position, refs}`) under `base`.
fn position_result(
    ev: &mut Evidence,
    base: &str,
    rec: &PositionRecord,
    position: Value,
) -> Option<Value> {
    let mut refs = add_position(ev, PosRef::of(rec), None)?;
    let (nums, dates) = index_paths(ev, base, &json!({ "position": &position }));
    ev.add_numbers_from(&position);
    refs["num"] = Value::Object(nums);
    refs["date"] = Value::Object(dates);
    Some(json!({"position": position, "refs": refs}))
}

/// The record the executor asked for, or `None` for any other one.
fn own_record(rec: Option<PositionRecord>, game_id: &str, ply: u32) -> Option<PositionRecord> {
    rec.filter(|r| r.game_id == game_id && r.ply == ply)
}

fn exemplar_refs(ev: &mut Evidence, e: &ExemplarPosition) -> Value {
    add_position(
        ev,
        PosRef {
            game_id: &e.game_id,
            ply: e.ply,
            fen: &e.fen,
            played: &e.played,
            best: e.best.as_ref(),
            best_pv: &e.best_pv,
        },
        Some(&e.refutation_pv),
    )
    .unwrap_or(Value::Null)
}

// --- The tools: validate, fetch (async, no evidence), then write (sync) ---

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PositionIn {
    game_id: String,
    ply: u32,
}

async fn get_position(deps: &ToolDeps<'_>, s: &ToolSession, input: &Value) -> Out {
    let i: PositionIn = parse(input, &["gameId", "ply"])?;
    check_id("gameId", &i.game_id)?;
    check_ply(i.ply)?;
    let rec = deps
        .ds
        .get_position(&i.game_id, i.ply)
        .await
        .map_err(|e| source_failed("get_position", &e))?;
    let Some((rec, position)) = own_record(rec, &i.game_id, i.ply).and_then(|r| {
        let p = position_json(&r)?;
        Some((r, p))
    }) else {
        return Ok(not_found());
    };
    Ok(s.write(|ev, k| {
        let Some(mut out) = position_result(ev, &format!("tools.{k}"), &rec, position) else {
            return not_found();
        };
        out["found"] = json!(true);
        out
    }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EvalIn {
    game_id: String,
    ply: u32,
    moves: Vec<String>,
}

async fn get_eval(deps: &ToolDeps<'_>, s: &ToolSession, input: &Value) -> Out {
    let i: EvalIn = parse(input, &["gameId", "ply", "moves"])?;
    check_id("gameId", &i.game_id)?;
    check_ply(i.ply)?;
    check_moves(&i.moves)?;
    let rec = deps
        .ds
        .get_position(&i.game_id, i.ply)
        .await
        .map_err(|e| source_failed("get_eval", &e))?;
    let Some(rec) = own_record(rec, &i.game_id, i.ply) else {
        return Ok(not_found());
    };
    let fen = replay(&rec.fen, &i.moves)?;
    let cached = deps
        .ds
        .get_cached_eval(&fen)
        .await
        .map_err(|e| source_failed("get_eval", &e))?;
    let Some(c) = cached.filter(|c| fen_key(&c.fen) == fen_key(&fen)) else {
        return Ok(json!({"available": false, "reason": "not in eval cache"}));
    };
    let tried = pv_to_san(&rec.fen, &i.moves);
    let pv = pv_to_san(&fen, &c.pv);
    let score = serde_json::to_value(c.score).unwrap_or(Value::Null);
    let position = position_json(&rec);
    Ok(s.write(|ev, k| {
        let base = format!("tools.{k}");
        let position = position.and_then(|p| position_result(ev, &base, &rec, p));
        // The tried moves and the cached line become allowed at the base position.
        let mut line_uci = i.moves.clone();
        line_uci.extend(c.pv.iter().take(pv.len()).cloned());
        ev.add_position(PositionArgs {
            game_id: rec.game_id.clone(),
            ply: rec.ply,
            fen: rec.fen.clone(),
            pvs: vec![PvFrom {
                from_fen: rec.fen.clone(),
                uci: line_uci,
            }],
            ..PositionArgs::default()
        });
        let key = format!("tool{k}");
        let mut moves = tried.clone();
        moves.extend(pv.iter().cloned());
        let n = moves.len().min(999);
        ev.add_line(&key, moves);
        let line_token = |i: usize| format!("{{{{line:{key}:{i}}}}}");
        let (nums, _) = index_paths(ev, &base, &json!({ "eval": &score }));
        json!({
            "available": true,
            "gameId": rec.game_id,
            "ply": rec.ply,
            "moves": i.moves,
            "fen": fen,
            "eval": score,
            "bestUci": c.best_uci,
            "triedSan": tried,
            "pvSan": pv,
            "refs": {
                "line": format!("{{{{line:{key}}}}}"),
                "tried": (1..=tried.len().min(n)).map(line_token).collect::<Vec<_>>(),
                "pv": (tried.len() + 1..=n).map(line_token).collect::<Vec<_>>(),
                "num": nums,
                "position": position.map(|p| p["refs"].clone()),
            },
        })
    }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GamesIn {
    #[serde(default)]
    result: Option<String>,
    #[serde(default)]
    opening_contains: Option<String>,
    #[serde(default)]
    since: Option<String>,
    limit: u32,
}

async fn get_games(deps: &ToolDeps<'_>, s: &ToolSession, input: &Value) -> Out {
    let i: GamesIn = parse(input, &["result", "openingContains", "since", "limit"])?;
    check_limit(i.limit, MAX_GAMES)?;
    let result = match i.result.as_deref() {
        None => None,
        Some(r) => Some(
            GameResult::parse(r)
                .ok_or_else(|| "invalid input: result must be win, loss or draw".to_owned())?,
        ),
    };
    let opening_contains = match i.opening_contains {
        Some(o)
            if o.trim().is_empty()
                || o.chars().count() > MAX_ID_CHARS
                || o.chars().any(char::is_control) =>
        {
            return Err(format!(
                "invalid input: openingContains must be 1 to {MAX_ID_CHARS} characters"
            ));
        }
        o => o,
    };
    let since = match i.since.as_deref() {
        None => None,
        Some(d) => Some(
            (d.len() == 10)
                .then(|| Date::parse(d, format_description!("[year]-[month]-[day]")).ok())
                .flatten()
                .ok_or_else(|| "invalid input: since must be a date YYYY-MM-DD".to_owned())?,
        ),
    };
    let games = deps
        .ds
        .get_games(GamesQuery {
            result,
            opening_contains,
            since,
            limit: i.limit,
        })
        .await
        .map_err(|e| source_failed("get_games", &e))?;
    let games: Vec<(GameSummary, Value)> = games
        .into_iter()
        .filter(|g| check_id("gameId", &g.game_id).is_ok())
        .filter_map(|g| {
            let v = serde_json::to_value(&g).ok()?;
            Some((g, v))
        })
        .take(i.limit as usize)
        .collect();
    Ok(s.write(|ev, k| {
        let mut out = Vec::new();
        let mut refs = Vec::new();
        for (g, v) in games {
            ev.add_game(&g.game_id);
            ev.add_numbers_from(&v);
            let (nums, dates) = index_paths(ev, &format!("tools.{k}.games.{}", out.len()), &v);
            refs.push(json!({"gameId": g.game_id, "num": nums, "date": dates}));
            out.push(v);
        }
        json!({"games": out, "refs": {"games": refs}})
    }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WeaknessIn {
    weakness_id: String,
}

async fn get_weakness(deps: &ToolDeps<'_>, s: &ToolSession, input: &Value) -> Out {
    let i: WeaknessIn = parse(input, &["weaknessId"])?;
    check_id("weaknessId", &i.weakness_id)?;
    let w = deps
        .ds
        .get_weakness(&i.weakness_id)
        .await
        .map_err(|e| source_failed("get_weakness", &e))?;
    let Some(w) = w.filter(|w| w.id == i.weakness_id) else {
        return Ok(not_found());
    };
    let v = serde_json::to_value(&w).map_err(|_| "tool unavailable".to_owned())?;
    Ok(s.write(|ev, k| {
        ev.add_numbers_from(&v);
        let (nums, dates) = index_paths(ev, &format!("tools.{k}"), &json!({ "weakness": &v }));
        ev.mark_weakness_rates(&format!("tools.{k}.weakness"));
        let exemplars: Vec<Value> = w.exemplars.iter().map(|e| exemplar_refs(ev, e)).collect();
        json!({
            "found": true,
            "weakness": v,
            "refs": {"num": nums, "date": dates, "exemplars": exemplars},
        })
    }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LineIn {
    line_key: String,
}

async fn get_opening_line(deps: &ToolDeps<'_>, s: &ToolSession, input: &Value) -> Out {
    let i: LineIn = parse(input, &["lineKey"])?;
    check_line_key(&i.line_key)?;
    let o = deps
        .ds
        .get_opening_line(&i.line_key)
        .await
        .map_err(|e| source_failed("get_opening_line", &e))?;
    let Some(o) = o.filter(|o| o.line_key == i.line_key) else {
        return Ok(not_found());
    };
    let v = serde_json::to_value(&o).map_err(|_| "tool unavailable".to_owned())?;
    Ok(s.write(|ev, k| {
        let key = &o.line_key;
        if !ev.lines.contains_key(key) {
            // As the report's index: the line plus its deviation and alternatives are allowed.
            ev.add_line(key, o.moves_san.clone());
            if let Some(set) = ev.lines.get_mut(key) {
                let extra = o
                    .deviation_move_san
                    .iter()
                    .chain(o.alternatives.iter().flat_map(|a| &a.moves_san));
                for m in extra {
                    set.insert(m.clone());
                    set.insert(strip_san(m));
                }
            }
        }
        let n = ev.line_moves.get(key).map_or(0, Vec::len).min(999);
        ev.add_numbers_from(&v);
        let (nums, dates) = index_paths(ev, &format!("tools.{k}"), &json!({ "line": &v }));
        ev.mark_opening_rates(&format!("tools.{k}.line"), o.alternatives.len());
        let examples: Vec<Value> = o.examples.iter().map(|e| exemplar_refs(ev, e)).collect();
        json!({
            "found": true,
            "line": v,
            "refs": {
                "line": format!("{{{{line:{key}}}}}"),
                "moves": (1..=n).map(|i| format!("{{{{line:{key}:{i}}}}}")).collect::<Vec<_>>(),
                "num": nums,
                "date": dates,
                "examples": examples,
            },
        })
    }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ExamplesIn {
    #[serde(default)]
    weakness_id: Option<String>,
    #[serde(default)]
    motif: Option<String>,
    #[serde(default)]
    line_key: Option<String>,
    #[serde(default)]
    phase: Option<String>,
    #[serde(default)]
    judgement: Option<String>,
    #[serde(default)]
    min_judgement: Option<String>,
    #[serde(default)]
    standing: Option<String>,
    #[serde(default)]
    min_win_before: Option<u32>,
    #[serde(default)]
    max_win_before: Option<u32>,
    #[serde(default, rename = "minDeltaW")]
    min_delta_w: Option<u32>,
    #[serde(default)]
    clock_below_seconds: Option<u32>,
    #[serde(default)]
    color: Option<String>,
    #[serde(default)]
    since: Option<String>,
    #[serde(default)]
    until: Option<String>,
    #[serde(default)]
    last_games: Option<u32>,
    #[serde(default)]
    sort: Option<String>,
    limit: u32,
}

const EXAMPLES_FIELDS: &[&str] = &[
    "weaknessId",
    "motif",
    "lineKey",
    "phase",
    "judgement",
    "minJudgement",
    "standing",
    "minWinBefore",
    "maxWinBefore",
    "minDeltaW",
    "clockBelowSeconds",
    "color",
    "since",
    "until",
    "lastGames",
    "sort",
    "limit",
];

/// A validated `YYYY-MM-DD` of `field`.
fn check_date(field: &str, d: Option<&str>) -> Result<Option<Date>, String> {
    d.map(|d| {
        (d.len() == 10)
            .then(|| Date::parse(d, format_description!("[year]-[month]-[day]")).ok())
            .flatten()
            .ok_or_else(|| format!("invalid input: {field} must be a date YYYY-MM-DD"))
    })
    .transpose()
}

/// A validated enum value of `field`, one of `allowed` (never echoes the value).
fn check_enum<T>(
    field: &str,
    v: Option<&str>,
    parse: impl Fn(&str) -> Option<T>,
    allowed: &str,
) -> Result<Option<T>, String> {
    v.map(|s| parse(s).ok_or_else(|| format!("invalid input: {field} must be {allowed}")))
        .transpose()
}

fn check_range(field: &str, v: Option<u32>, lo: u32, hi: u32) -> Result<(), String> {
    match v {
        Some(n) if !(lo..=hi).contains(&n) => {
            Err(format!("invalid input: {field} must be {lo} to {hi}"))
        }
        _ => Ok(()),
    }
}

fn parse_sort(s: &str) -> Option<ExampleSort> {
    match s {
        "recent" => Some(ExampleSort::Recent),
        "worst" => Some(ExampleSort::Worst),
        _ => None,
    }
}

/// The move filter of `i`, or `None` when no filter field is set (`sort` alone filters
/// nothing). Every field is validated here.
fn move_filter(i: &ExamplesIn) -> Result<Option<MoveFilter>, String> {
    const JUDGEMENTS: &str = "inaccuracy, mistake or blunder";
    let any = i.phase.is_some()
        || i.judgement.is_some()
        || i.min_judgement.is_some()
        || i.standing.is_some()
        || i.min_win_before.is_some()
        || i.max_win_before.is_some()
        || i.min_delta_w.is_some()
        || i.clock_below_seconds.is_some()
        || i.color.is_some()
        || i.since.is_some()
        || i.until.is_some()
        || i.last_games.is_some();
    if !any {
        return Ok(None);
    }
    let phase = check_enum(
        "phase",
        i.phase.as_deref(),
        Phase::parse,
        "opening, middlegame or endgame",
    )?;
    let judgement = check_enum(
        "judgement",
        i.judgement.as_deref(),
        Judgement::parse,
        JUDGEMENTS,
    )?;
    let min_judgement = check_enum(
        "minJudgement",
        i.min_judgement.as_deref(),
        Judgement::parse,
        JUDGEMENTS,
    )?;
    if judgement.is_some() && min_judgement.is_some() {
        return Err("invalid input: give judgement or minJudgement, not both".to_owned());
    }
    let standing = check_enum(
        "standing",
        i.standing.as_deref(),
        Standing::parse,
        "winning, equal or losing",
    )?;
    check_range("minWinBefore", i.min_win_before, 0, 100)?;
    check_range("maxWinBefore", i.max_win_before, 0, 100)?;
    if let (Some(lo), Some(hi)) = (i.min_win_before, i.max_win_before)
        && lo > hi
    {
        return Err("invalid input: minWinBefore must not be above maxWinBefore".to_owned());
    }
    check_range("minDeltaW", i.min_delta_w, 1, 100)?;
    check_range(
        "clockBelowSeconds",
        i.clock_below_seconds,
        1,
        MAX_CLOCK_SECONDS,
    )?;
    let color = check_enum(
        "color",
        i.color.as_deref(),
        |c| c.parse::<PlayerColor>().ok(),
        "white or black",
    )?;
    let since = check_date("since", i.since.as_deref())?;
    let until = check_date("until", i.until.as_deref())?;
    if let (Some(a), Some(b)) = (since, until)
        && a > b
    {
        return Err("invalid input: since must not be after until".to_owned());
    }
    check_range("lastGames", i.last_games, 1, MAX_LAST_GAMES)?;
    let sort = check_enum("sort", i.sort.as_deref(), parse_sort, "recent or worst")?;
    Ok(Some(MoveFilter {
        phase,
        judgement,
        min_judgement,
        standing,
        min_win_before: i.min_win_before,
        max_win_before: i.max_win_before,
        min_delta_w: i.min_delta_w,
        clock_below: i.clock_below_seconds,
        color,
        since,
        until,
        last_games: i.last_games,
        sort: sort.unwrap_or_default(),
    }))
}

/// What a move filter searched, so the model can say it when nothing (or little) matched: the
/// validated filter in the tool's own field names, plus the source's counts when it has them.
fn searched_json(f: &MoveFilter, counts: Option<&FilterCounts>) -> Value {
    let mut o = Map::new();
    let mut put = |k: &str, v: Value| {
        o.insert(k.to_owned(), v);
    };
    if let Some(c) = counts {
        put("timeClass", json!(c.time_class));
    }
    put("playerMovesOnly", json!(true));
    if let Some(p) = f.phase {
        put("phase", json!(p.as_str()));
    }
    if let Some(j) = f.judgement {
        put("judgement", json!(j.as_str()));
    }
    if let Some(j) = f.min_judgement {
        put("minJudgement", json!(j.as_str()));
    } else if f.judgement.is_none() && f.min_delta_w.is_none() {
        // The default: only erring moves.
        put("minJudgement", json!(Judgement::Inaccuracy.as_str()));
    }
    if let Some(st) = f.standing {
        put("standing", json!(st.as_str()));
    }
    // The win-chance bounds in force (a preset's own bound, or a tighter one given), so the
    // model can state them with a number token: "winning" is an engine word that needs one.
    let preset = |x: f64| Some(x as u32);
    let (pre_lo, pre_hi) = match f.standing {
        Some(Standing::Winning) => (preset(WINNING_FROM), None),
        Some(Standing::Losing) => (None, preset(LOSING_TO)),
        Some(Standing::Equal) => (preset(LOSING_TO), preset(WINNING_FROM)),
        None => (None, None),
    };
    let lo = f.min_win_before.max(pre_lo);
    let hi = match (f.max_win_before, pre_hi) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    };
    for (k, v) in [
        ("minWinBefore", lo),
        ("maxWinBefore", hi),
        ("minDeltaW", f.min_delta_w),
        ("clockBelowSeconds", f.clock_below),
        ("lastGames", f.last_games),
    ] {
        if let Some(n) = v {
            put(k, json!(n));
        }
    }
    if let Some(c) = f.color {
        put("color", json!(c.as_str()));
    }
    let ymd = format_description!("[year]-[month]-[day]");
    for (k, d) in [("since", f.since), ("until", f.until)] {
        if let Some(s) = d.and_then(|d| d.format(&ymd).ok()) {
            put(k, json!(s));
        }
    }
    put(
        "sort",
        json!(match f.sort {
            ExampleSort::Recent => "recent",
            ExampleSort::Worst => "worst",
        }),
    );
    if let Some(c) = counts {
        put("gamesSearched", json!(c.games_searched));
        put("matchingMoves", json!(c.matching_moves));
        put("matchingGames", json!(c.matching_games));
    }
    Value::Object(o)
}

async fn find_examples(deps: &ToolDeps<'_>, s: &ToolSession, input: &Value) -> Out {
    let i: ExamplesIn = parse(input, EXAMPLES_FIELDS)?;
    let selectors = usize::from(i.weakness_id.is_some())
        + usize::from(i.motif.is_some())
        + usize::from(i.line_key.is_some());
    let filter = move_filter(&i)?;
    if selectors + usize::from(filter.is_some()) != 1 {
        return Err(
            "invalid input: give exactly one of weaknessId, motif, lineKey and a move filter"
                .to_owned(),
        );
    }
    check_limit(i.limit, MAX_EXAMPLES)?;
    if let Some(w) = &i.weakness_id {
        check_id("weaknessId", w)?;
    }
    if let Some(m) = &i.motif {
        check_id("motif", m)?;
    }
    if let Some(l) = &i.line_key {
        check_line_key(l)?;
    }
    let limit = i.limit as usize;
    let counts = match &filter {
        Some(f) => deps
            .ds
            .count_examples(f)
            .await
            .map_err(|e| source_failed("find_examples", &e))?,
        None => None,
    };
    let searched = filter.as_ref().map(|f| searched_json(f, counts.as_ref()));
    let records = deps
        .ds
        .find_examples(ExamplesQuery {
            weakness_id: i.weakness_id,
            motif: i.motif,
            line_key: i.line_key,
            filter,
            limit: i.limit,
        })
        .await
        .map_err(|e| source_failed("find_examples", &e))?;
    let mut seen = std::collections::HashSet::new();
    let picked: Vec<(PositionRecord, Value)> = records
        .into_iter()
        .filter(|r| seen.insert((r.game_id.clone(), r.ply)))
        .filter_map(|r| {
            let p = position_json(&r)?;
            Some((r, p))
        })
        .collect();
    Ok(s.write(|ev, k| {
        let mut out = Vec::new();
        for (rec, position) in picked {
            if out.len() >= limit {
                break;
            }
            let base = format!("tools.{k}.examples.{}", out.len());
            if let Some(r) = position_result(ev, &base, &rec, position) {
                out.push(r);
            }
        }
        let Some(searched) = searched else {
            return json!({ "examples": out });
        };
        let (nums, dates) =
            index_paths(ev, &format!("tools.{k}"), &json!({ "searched": &searched }));
        ev.add_numbers_from(&searched);
        json!({
            "examples": out,
            "searched": searched,
            "refs": {"num": nums, "date": dates},
        })
    }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AnalyseIn {
    fen: String,
    moves: Vec<String>,
}

fn check_analyse(input: &Value) -> Result<(AnalyseIn, String), String> {
    let i: AnalyseIn = parse(input, &["fen", "moves"])?;
    if i.fen.len() > MAX_FEN_CHARS {
        return Err(format!(
            "invalid input: fen allows at most {MAX_FEN_CHARS} characters"
        ));
    }
    check_moves(&i.moves)?;
    if cozy_chess::Board::from_fen(&i.fen, false).is_err() || Position::from_fen(&i.fen).is_err() {
        return Err("invalid FEN".to_owned());
    }
    let fen = replay(&i.fen, &i.moves)?;
    Ok((i, fen))
}

async fn analyse_position(deps: &ToolDeps<'_>, s: &ToolSession, input: &Value) -> ToolExec {
    let (i, fen) = match check_analyse(input) {
        Ok(x) => x,
        Err(e) => return ToolExec::Ready(err(&e)),
    };
    if let Ok(pos) = Position::from_fen(&fen) {
        if pos.is_checkmate() {
            return ToolExec::Ready(ok(json!({"terminal": "checkmate"})));
        }
        if pos.is_stalemate() {
            return ToolExec::Ready(ok(json!({"terminal": "stalemate"})));
        }
    }
    if !s.reserve_engine_check() {
        return ToolExec::Ready(ok(json!({"available": false, "reason": "per_turn"})));
    }
    match deps
        .engine
        .request(&i.fen, &i.moves, ENGINE_WAIT_INLINE)
        .await
    {
        Ok(EngineCheck::Pending { eval_id }) if is_eval_id(&eval_id) => {
            ToolExec::EnginePending { eval_id }
        }
        Ok(EngineCheck::Pending { .. }) => ToolExec::Ready(engine_unavailable()),
        Ok(check) => ToolExec::Ready(engine_result_output(s, check)),
        Err(e) => {
            tracing::warn!(error = %format!("{e:#}"), "interactive engine request failed");
            ToolExec::Ready(engine_unavailable())
        }
    }
}

/// The `{{eng:}}` grammar's eval id: 32 lowercase hex.
fn is_eval_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn engine_done(ev: &mut Evidence, eval_id: &str, mut e: EngineEvidence) -> ToolOutput {
    if !is_eval_id(eval_id) {
        return engine_unavailable();
    }
    e.lines.truncate(MAX_ENGINE_LINES);
    for l in &mut e.lines {
        l.san.truncate(MAX_LINE_MOVES);
    }
    e.tried_san.truncate(MAX_MOVES);
    let eng = |rest: &str| format!("{{{{eng:{eval_id}:{rest}}}}}");
    let lines: Vec<Value> = e
        .lines
        .iter()
        .enumerate()
        .map(|(i, l)| {
            let n = i + 1;
            json!({
                "line": n,
                "score": serde_json::to_value(l.score).unwrap_or(Value::Null),
                "sideToMove": match l.side_to_move { Color::White => "white", Color::Black => "black" },
                "san": l.san,
                "refs": {
                    "eval": eng(&n.to_string()),
                    "moves": (1..=l.san.len()).map(|m| eng(&format!("{n}:{m}"))).collect::<Vec<_>>(),
                },
            })
        })
        .collect();
    let refs = json!({
        "tried": (!e.tried_san.is_empty()).then(|| eng("tried")),
        "lines": (1..=e.lines.len()).map(|n| eng(&n.to_string())).collect::<Vec<_>>(),
        "moves": e.lines.first().map_or(Vec::new(), |l| {
            (1..=l.san.len()).map(|m| eng(&format!("1:{m}"))).collect::<Vec<_>>()
        }),
    });
    let out = json!({
        "evalId": eval_id,
        "triedSan": e.tried_san,
        "lines": lines,
        "refs": refs,
    });
    ev.add_engine_eval(eval_id.to_owned(), e);
    ok(out)
}

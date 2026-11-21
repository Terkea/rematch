//! Runs one chat tool call. Tool inputs are untrusted model output: every field is validated
//! here (sizes, counts, ranges, charsets) before anything is read, and nothing panics.
//!
//! - Every id goes to the account-scoped [`CoachDataSource`]; an id of another account is not
//!   found. The executor also rejects a returned record that is not the one it asked for, so a
//!   faulty source cannot leak another position into the evidence.
//! - Results are JSON with `refs`: ready-made tokens that resolve against the evidence this call
//!   adds (positions, opening lines, `tools.<call_index>.<field>` number and date paths, and
//!   `{{eng:}}` results). Only engine results this account's engine returned become `{{eng:}}`
//!   evidence.
//! - Evidence rules (Task 7): a position already in the evidence keeps its FEN, moves and lines
//!   (new allowed moves are merged in); an opening line already indexed from the report is not
//!   replaced; number and date paths keep their first value.
//! - Errors never echo model input (only a validated UCI move in `illegal move <uci>`) and never
//!   carry an internal error message.

use std::sync::Arc;
use std::time::Duration;

use chess_core::Color;
use chess_core::board::Position;
use futures::future::BoxFuture;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value, json};
use time::Date;
use time::macros::format_description;

use super::data_source::{
    CoachDataSource, EngineCheck, ExamplesQuery, GameResult, GamesQuery, InteractiveEngine,
    PositionRecord,
};
use super::tools::{MAX_EXAMPLES, MAX_FEN_CHARS, MAX_GAMES, MAX_ID_CHARS, MAX_MOVES, MAX_PLY};
use crate::entitlements::QuotaWindow;
use crate::input::types::{ExemplarPosition, MoveRef};
use crate::validator::evidence::{EngineEvidence, Evidence, PositionArgs, PvFrom, PvSides};
use crate::validator::pv::{fen_after, pv_to_san, strip_san};

/// How long `analyse_position` waits for the engine before it reports `EnginePending`.
/// (Task 23's `limits.rs` re-exports this value with `ENGINE_WAIT_MAX`.)
pub const ENGINE_WAIT_INLINE: Duration = Duration::from_secs(5);

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

/// The executor behind a trait, so the turn's tool handler (Task 23 wraps it in T11's
/// `ToolHandler`) can hold borrowed ([`ToolDeps`]) or owned ([`OwnedToolDeps`]) dependencies.
pub trait ToolExecutor: Send + Sync {
    fn execute<'a>(
        &'a self,
        ev: &'a mut Evidence,
        call_index: u32,
        name: &'a str,
        input: Option<&'a Value>,
    ) -> BoxFuture<'a, ToolExec>;
}

impl ToolExecutor for ToolDeps<'_> {
    fn execute<'a>(
        &'a self,
        ev: &'a mut Evidence,
        call_index: u32,
        name: &'a str,
        input: Option<&'a Value>,
    ) -> BoxFuture<'a, ToolExec> {
        execute_tool(self, ev, call_index, name, input)
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
        ev: &'a mut Evidence,
        call_index: u32,
        name: &'a str,
        input: Option<&'a Value>,
    ) -> BoxFuture<'a, ToolExec> {
        Box::pin(async move {
            let deps = ToolDeps {
                ds: self.ds.as_ref(),
                engine: self.engine.as_ref(),
            };
            run(&deps, ev, call_index, name, input).await
        })
    }
}

/// Runs the tool `name` with the model's `input` (`None`: the input could not be parsed).
pub fn execute_tool<'a, 'd: 'a>(
    deps: &'a ToolDeps<'d>,
    ev: &'a mut Evidence,
    call_index: u32,
    name: &'a str,
    input: Option<&'a Value>,
) -> BoxFuture<'a, ToolExec> {
    Box::pin(run(deps, ev, call_index, name, input))
}

/// Renders an engine-check outcome after a pending wait. A still-pending check is `engine
/// busy` (the caller abandons it).
pub fn engine_result_output(ev: &mut Evidence, check: EngineCheck) -> ToolOutput {
    match check {
        EngineCheck::Done { eval_id, evidence } => engine_done(ev, &eval_id, evidence),
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

/// A data-source failure: logged without detail, reported as unavailable.
fn source_failed(tool: &'static str, e: &anyhow::Error) -> String {
    tracing::warn!(
        tool,
        error_chain_len = e.chain().count(),
        "chat tool data source failed"
    );
    "tool unavailable".to_owned()
}

async fn run(
    deps: &ToolDeps<'_>,
    ev: &mut Evidence,
    k: u32,
    name: &str,
    input: Option<&Value>,
) -> ToolExec {
    let Some(input) = input else {
        return ToolExec::Ready(err("input could not be parsed"));
    };
    let result = match name {
        "analyse_position" => return analyse_position(deps, ev, input).await,
        "find_examples" => find_examples(deps, ev, k, input).await,
        "get_eval" => get_eval(deps, ev, k, input).await,
        "get_games" => get_games(deps, ev, k, input).await,
        "get_opening_line" => get_opening_line(deps, ev, k, input).await,
        "get_position" => get_position(deps, ev, k, input).await,
        "get_weakness" => get_weakness(deps, ev, k, input).await,
        _ => Err("unknown tool".to_owned()),
    };
    ToolExec::Ready(match result {
        Ok(v) => ok(v),
        Err(e) => err(&e),
    })
}

// --- Input parsing and validation ---

/// Strict deserialisation. The error names at most a short known field path, never a value.
fn parse<T: DeserializeOwned>(input: &Value) -> Result<T, String> {
    // Derived structs also accept a JSON array of the fields: only an object is an input.
    if !input.is_object() {
        return Err("invalid input".to_owned());
    }
    serde_path_to_error::deserialize::<_, T>(input).map_err(|e| {
        let path = e.path().to_string();
        let printable = !path.is_empty()
            && path != "."
            && path.len() <= 32
            && path.chars().all(|c| c.is_ascii_alphanumeric() || c == '.');
        if printable {
            format!("invalid input at {path}")
        } else {
            "invalid input".to_owned()
        }
    })
}

/// TS `ID`: `[A-Za-z0-9_.-]`, the token grammar's game-id charset.
fn is_id_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-')
}

/// The token grammar's line-key charset (`[A-Za-z0-9_.:-]`).
fn is_line_char(c: char) -> bool {
    is_id_char(c) || c == ':'
}

fn check_chars(field: &str, s: &str, allowed: fn(char) -> bool) -> Result<(), String> {
    if s.is_empty() || s.len() > MAX_ID_CHARS || !s.chars().all(allowed) {
        return Err(format!(
            "invalid input: {field} must be 1 to {MAX_ID_CHARS} characters of letters, digits and _ . -"
        ));
    }
    Ok(())
}

fn check_id(field: &str, s: &str) -> Result<(), String> {
    check_chars(field, s, is_id_char)
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

/// TS `UCI = /^([a-h][1-8])([a-h][1-8])([qrbn])?$/`.
fn is_uci(u: &str) -> bool {
    let b = u.as_bytes();
    let sq = |f: u8, r: u8| (b'a'..=b'h').contains(&f) && (b'1'..=b'8').contains(&r);
    match b {
        [f1, r1, f2, r2] => sq(*f1, *r1) && sq(*f2, *r2),
        [f1, r1, f2, r2, p] => sq(*f1, *r1) && sq(*f2, *r2) && b"qrbn".contains(p),
        _ => false,
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

/// FEN piece placement, side, castling and en passant: the position, without move counters.
fn same_position(a: &str, b: &str) -> bool {
    a.split_whitespace()
        .take(4)
        .eq(b.split_whitespace().take(4))
}

/// `YYYY-MM-DD` at the start of `s` (ISO date or date-time), as `seg`'s date-path rule.
fn iso_date_prefix(s: &str) -> Option<&str> {
    let b = s.as_bytes();
    let shape = b.len() >= 10
        && (b.len() == 10 || b[10] == b'T')
        && b[..10].iter().enumerate().all(|(i, c)| {
            if i == 4 || i == 7 {
                *c == b'-'
            } else {
                c.is_ascii_digit()
            }
        });
    shape.then(|| &s[..10])
}

// --- Evidence writers ---

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

/// Number and date paths under `base` for every number and ISO date in `v` (plus the derived
/// `moveNumber` of a `ply` and `pawns` of a `cp`, as the report's path index has). Returns the
/// ready-made `{{num:}}` and `{{date:}}` tokens keyed by their path relative to `base`.
fn index_paths(
    ev: &mut Evidence,
    base: &str,
    v: &Value,
) -> (Map<String, Value>, Map<String, Value>) {
    let mut nums = Map::new();
    let mut dates = Map::new();
    walk(ev, base, "", v, &mut nums, &mut dates);
    (nums, dates)
}

fn put_num(ev: &mut Evidence, nums: &mut Map<String, Value>, base: &str, rel: String, n: f64) {
    let path = format!("{base}.{rel}");
    ev.put_number_path(&path, n);
    if ev.number_paths.contains_key(&path) {
        nums.insert(rel, Value::String(format!("{{{{num:{path}}}}}")));
    }
}

fn walk(
    ev: &mut Evidence,
    base: &str,
    rel: &str,
    v: &Value,
    nums: &mut Map<String, Value>,
    dates: &mut Map<String, Value>,
) {
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
        Value::String(s) if !rel.is_empty() => {
            if let Some(d) = iso_date_prefix(s) {
                let path = format!("{base}.{rel}");
                ev.put_date_path(&path, d.to_owned());
                dates.insert(
                    rel.to_owned(),
                    Value::String(format!("{{{{date:{path}}}}}")),
                );
            }
        }
        _ => {}
    }
}

/// Adds a position and returns its `move`/`best`/`pv` tokens, or `None` for a record whose id
/// or ply the token grammar cannot carry. A position already in the evidence keeps its line.
/// A position to index: its id, the FEN before `played`, and the best line from that FEN.
struct PosRef<'a> {
    game_id: &'a str,
    ply: u32,
    fen: &'a str,
    played: &'a MoveRef,
    best: Option<&'a MoveRef>,
    best_pv: &'a [String],
}

fn add_position(
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

/// A data-source position as a result (`{position, refs}`), indexed under `base`.
fn position_result(ev: &mut Evidence, base: &str, rec: &PositionRecord) -> Option<Value> {
    let mut refs = add_position(
        ev,
        PosRef {
            game_id: &rec.game_id,
            ply: rec.ply,
            fen: &rec.fen,
            played: &rec.played,
            best: rec.best.as_ref(),
            best_pv: &rec.pv,
        },
        None,
    )?;
    let mut position = serde_json::to_value(rec).ok()?;
    position["bestPvSan"] = json!(pv_to_san(&rec.fen, &rec.pv));
    let (nums, dates) = index_paths(ev, base, &json!({ "position": &position }));
    ev.add_numbers_from(&position);
    refs["num"] = Value::Object(nums);
    refs["date"] = Value::Object(dates);
    Some(json!({"position": position, "refs": refs}))
}

/// Adds the position of a record the executor asked for. Rejects any other record.
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

// --- The tools ---

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PositionIn {
    game_id: String,
    ply: u32,
}

async fn get_position(deps: &ToolDeps<'_>, ev: &mut Evidence, k: u32, input: &Value) -> Out {
    let i: PositionIn = parse(input)?;
    check_id("gameId", &i.game_id)?;
    check_ply(i.ply)?;
    let rec = deps
        .ds
        .get_position(&i.game_id, i.ply)
        .await
        .map_err(|e| source_failed("get_position", &e))?;
    let Some(rec) = own_record(rec, &i.game_id, i.ply) else {
        return Ok(not_found());
    };
    let Some(mut out) = position_result(ev, &format!("tools.{k}"), &rec) else {
        return Ok(not_found());
    };
    out["found"] = json!(true);
    Ok(out)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EvalIn {
    game_id: String,
    ply: u32,
    moves: Vec<String>,
}

async fn get_eval(deps: &ToolDeps<'_>, ev: &mut Evidence, k: u32, input: &Value) -> Out {
    let i: EvalIn = parse(input)?;
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
    let Some(c) = cached.filter(|c| same_position(&c.fen, &fen)) else {
        return Ok(json!({"available": false, "reason": "not in eval cache"}));
    };
    let base = format!("tools.{k}");
    let position = position_result(ev, &base, &rec);
    let tried = pv_to_san(&rec.fen, &i.moves);
    let pv = pv_to_san(&fen, &c.pv);
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
    let score = serde_json::to_value(c.score).unwrap_or(Value::Null);
    let (nums, _) = index_paths(ev, &base, &json!({ "eval": &score }));
    Ok(json!({
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

async fn get_games(deps: &ToolDeps<'_>, ev: &mut Evidence, k: u32, input: &Value) -> Out {
    let i: GamesIn = parse(input)?;
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
        Some(s) => Some(
            (s.len() == 10)
                .then(|| Date::parse(s, format_description!("[year]-[month]-[day]")).ok())
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
    let mut out = Vec::new();
    let mut refs = Vec::new();
    for g in games
        .into_iter()
        .filter(|g| check_id("gameId", &g.game_id).is_ok())
        .take(i.limit as usize)
    {
        let Ok(v) = serde_json::to_value(&g) else {
            continue;
        };
        ev.add_game(&g.game_id);
        ev.add_numbers_from(&v);
        let (nums, dates) = index_paths(ev, &format!("tools.{k}.games.{}", out.len()), &v);
        refs.push(json!({"gameId": g.game_id, "num": nums, "date": dates}));
        out.push(v);
    }
    Ok(json!({"games": out, "refs": {"games": refs}}))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WeaknessIn {
    weakness_id: String,
}

async fn get_weakness(deps: &ToolDeps<'_>, ev: &mut Evidence, k: u32, input: &Value) -> Out {
    let i: WeaknessIn = parse(input)?;
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
    ev.add_numbers_from(&v);
    let (nums, dates) = index_paths(ev, &format!("tools.{k}"), &json!({ "weakness": &v }));
    let exemplars: Vec<Value> = w.exemplars.iter().map(|e| exemplar_refs(ev, e)).collect();
    Ok(json!({
        "found": true,
        "weakness": v,
        "refs": {"num": nums, "date": dates, "exemplars": exemplars},
    }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LineIn {
    line_key: String,
}

async fn get_opening_line(deps: &ToolDeps<'_>, ev: &mut Evidence, k: u32, input: &Value) -> Out {
    let i: LineIn = parse(input)?;
    check_chars("lineKey", &i.line_key, is_line_char)?;
    let o = deps
        .ds
        .get_opening_line(&i.line_key)
        .await
        .map_err(|e| source_failed("get_opening_line", &e))?;
    let Some(o) = o.filter(|o| o.line_key == i.line_key) else {
        return Ok(not_found());
    };
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
    let v = serde_json::to_value(&o).map_err(|_| "tool unavailable".to_owned())?;
    ev.add_numbers_from(&v);
    let (nums, dates) = index_paths(ev, &format!("tools.{k}"), &json!({ "line": &v }));
    let examples: Vec<Value> = o.examples.iter().map(|e| exemplar_refs(ev, e)).collect();
    Ok(json!({
        "found": true,
        "line": v,
        "refs": {
            "line": format!("{{{{line:{key}}}}}"),
            "moves": (1..=n).map(|i| format!("{{{{line:{key}:{i}}}}}")).collect::<Vec<_>>(),
            "num": nums,
            "date": dates,
            "examples": examples,
        },
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
    limit: u32,
}

async fn find_examples(deps: &ToolDeps<'_>, ev: &mut Evidence, k: u32, input: &Value) -> Out {
    let i: ExamplesIn = parse(input)?;
    let selectors = usize::from(i.weakness_id.is_some())
        + usize::from(i.motif.is_some())
        + usize::from(i.line_key.is_some());
    if selectors != 1 {
        return Err("invalid input: give exactly one of weaknessId, motif and lineKey".to_owned());
    }
    check_limit(i.limit, MAX_EXAMPLES)?;
    if let Some(w) = &i.weakness_id {
        check_id("weaknessId", w)?;
    }
    if let Some(m) = &i.motif {
        check_id("motif", m)?;
    }
    if let Some(l) = &i.line_key {
        check_chars("lineKey", l, is_line_char)?;
    }
    let limit = i.limit;
    let records = deps
        .ds
        .find_examples(ExamplesQuery {
            weakness_id: i.weakness_id,
            motif: i.motif,
            line_key: i.line_key,
            limit,
        })
        .await
        .map_err(|e| source_failed("find_examples", &e))?;
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for rec in records {
        if out.len() >= limit as usize {
            break;
        }
        if !seen.insert((rec.game_id.clone(), rec.ply)) {
            continue;
        }
        if let Some(r) = position_result(ev, &format!("tools.{k}.examples.{}", out.len()), &rec) {
            out.push(r);
        }
    }
    Ok(json!({ "examples": out }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AnalyseIn {
    fen: String,
    moves: Vec<String>,
}

async fn analyse_position(deps: &ToolDeps<'_>, ev: &mut Evidence, input: &Value) -> ToolExec {
    let checked = (|| -> Result<AnalyseIn, String> {
        let i: AnalyseIn = parse(input)?;
        if i.fen.len() > MAX_FEN_CHARS {
            return Err(format!(
                "invalid input: fen allows at most {MAX_FEN_CHARS} characters"
            ));
        }
        check_moves(&i.moves)?;
        if cozy_chess::Board::from_fen(&i.fen, false).is_err()
            || Position::from_fen(&i.fen).is_err()
        {
            return Err("invalid FEN".to_owned());
        }
        Ok(i)
    })();
    let i = match checked {
        Ok(i) => i,
        Err(e) => return ToolExec::Ready(err(&e)),
    };
    let fen = match replay(&i.fen, &i.moves) {
        Ok(f) => f,
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
    match deps
        .engine
        .request(&i.fen, &i.moves, ENGINE_WAIT_INLINE)
        .await
    {
        Ok(EngineCheck::Pending { eval_id }) if is_eval_id(&eval_id) => {
            ToolExec::EnginePending { eval_id }
        }
        Ok(EngineCheck::Pending { .. }) => ToolExec::Ready(engine_unavailable()),
        Ok(check) => ToolExec::Ready(engine_result_output(ev, check)),
        Err(e) => {
            tracing::warn!(
                error_chain_len = e.chain().count(),
                "interactive engine request failed"
            );
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

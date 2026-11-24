//! Per-game enrichment (R2 Task 25, TS M2 Task 22 `enrichGame`, ADR-R2-14).
//!
//! Over an analysed game (main evals at MultiPV 1, never rewritten here):
//!
//! - **MultiPV**: 3 lines at `MULTIPV_NODES` on `fen_before` of the critical plies
//!   ([`select_critical_plies`]), cached in `position_multipv` under `MULTIPV_PROTOCOL`.
//! - **PV completion**: `refutation_pv = evals[i+1].pv`; when that is empty (Lichess server
//!   evals) and the ply is critical, a MultiPV probe of `fen_after` supplies it. `best_pv` is
//!   `evals[i].pv`; when that is empty, the `fen_before` probe's line for the main eval's
//!   `best_uci`, or empty when no line starts with it.
//! - **Threat probe**: `THREAT_NODES`, MultiPV 1 on `null_move_fen(fen_before)` for eligible
//!   (not decided) plies of the requested colours; skipped in check. Cached in
//!   `position_threats` under `THREAT_PROTOCOL`.
//! - **Tablebase**: positions with at most `max_pieces` pieces; `tb_wdl_after = -probe(fen_after)`
//!   (the probe answers for the side to move, the opponent). A missing tables directory gives
//!   `None`, never a failure.
//! - **Difficulty**: from Maia only (owner override), batched over the requested eligible plies.
//!   Plies `<= 10` are `not_applicable/opening_ply` and never sent. When Maia is missing or down,
//!   the difficulty is `None` with `pending_maia` (re-enriched later by [`crate::reenrich`]);
//!   nothing is ever estimated in its place, and enrichment completes.
//!
//! Every FEN is checked with `Position::from_fen` (and for a legal move) before it reaches the
//! analyser.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::future::Future;
use std::sync::Arc;
use std::time::Instant;

use chess_core::board::Position;
use chess_core::fen::fen_key;
use chess_core::score::win_percent;
use chess_core::{Color, MoveRecord, ParsedGame, PositionEval};
use engine::{AnalyseOptions, Analyser, AnalysisLine, DEFAULT_ANALYSIS_TIMEOUT, EngineError};
use profile_core::board::{null_move_fen, piece_count};
use profile_core::features::DIFFICULTY_MIN_PLY;
use profile_core::types::{MultipvLine, RawMoveFeatures, ThreatProbe, Wdl};
use tokio_util::sync::CancellationToken;

use crate::critical::select_critical_plies;
use crate::difficulty::select::select_difficulty_provider;
use crate::difficulty::{DifficultyOutcome, DifficultySource, NotApplicableReason};
use crate::protocol::{MAX_CRITICAL_PLIES, MULTIPV, MULTIPV_NODES, THREAT_NODES};
use crate::store::{EnrichmentStore, ThreatEntry};
use crate::types::{DifficultyInput, TablebaseProbe};

pub struct EnrichDeps {
    pub analyser: Arc<dyn Analyser>,
    pub store: Arc<dyn EnrichmentStore>,
    /// `None`: no tablebase (every `tb_wdl_*` is `None`).
    pub tb: Option<Arc<dyn TablebaseProbe>>,
    /// `None`: Maia is not configured, so eligible plies past the opening are
    /// `pending_maia/maia_not_configured`.
    pub difficulty: Option<Arc<dyn DifficultySource>>,
    pub cancel: CancellationToken,
}

pub struct EnrichInput<'a> {
    pub parsed: &'a ParsedGame,
    /// `plies + 1` evals (white's view); `evals[i]` is the position before ply `i + 1`.
    pub evals: &'a [PositionEval],
    pub records: &'a [MoveRecord],
    /// The colours to enrich (normally the player's).
    pub colors: &'a [Color],
    pub elo_white: Option<i32>,
    pub elo_black: Option<i32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Enriched {
    /// One entry per record; `None` for colours not requested.
    pub raw: Vec<Option<RawMoveFeatures>>,
    /// One entry per record: the difficulty outcome of requested eligible plies, `None` for
    /// decided plies and colours not requested. Persisted as `difficulty_status/_reason`.
    pub difficulty: Vec<Option<DifficultyOutcome>>,
    /// The Maia model id when any ply got a difficulty.
    pub difficulty_model: Option<String>,
    /// `multipv_ms`, `threat_ms`, `tb_ms`, `difficulty_ms`.
    pub stages: BTreeMap<String, f64>,
}

#[derive(thiserror::Error, Debug)]
pub enum EnrichError {
    #[error("enrichment engine error: {0}")]
    Engine(#[from] EngineError),
    #[error("enrichment cancelled")]
    Cancelled,
    #[error("enrichment store error: {0}")]
    Store(#[from] sqlx::Error),
}

/// A FEN the analyser may see: valid, with a legal move.
fn analysable(fen: &str) -> bool {
    Position::from_fen(fen).is_ok_and(|p| !p.is_checkmate() && !p.is_stalemate())
}

fn valid(fen: &str) -> bool {
    Position::from_fen(fen).is_ok()
}

async fn cancellable<T>(
    cancel: &CancellationToken,
    fut: impl Future<Output = T>,
) -> Result<T, EnrichError> {
    tokio::select! {
        biased;
        () = cancel.cancelled() => Err(EnrichError::Cancelled),
        v = fut => Ok(v),
    }
}

async fn analyse(
    deps: &EnrichDeps,
    fen: &str,
    nodes: u64,
    multipv: u32,
) -> Result<Vec<AnalysisLine>, EnrichError> {
    let opts = AnalyseOptions {
        nodes,
        multipv,
        timeout: DEFAULT_ANALYSIS_TIMEOUT,
    };
    let result = cancellable(&deps.cancel, deps.analyser.analyse(fen, opts)).await??;
    Ok(result.lines)
}

fn to_multipv(lines: Vec<AnalysisLine>) -> Vec<MultipvLine> {
    let mut lines = lines;
    lines.sort_by_key(|l| l.multipv);
    lines
        .into_iter()
        .filter_map(|l| {
            Some(MultipvLine {
                uci: l.pv.first()?.clone(),
                score: l.score,
                pv: l.pv,
            })
        })
        .collect()
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

/// The Maia input for one ply: the MultiPV lines with their mover win%, the main eval's best
/// move with `win_before` when it is not among them, and the played move with `win_after`.
/// `best_uci` is the main eval's (MultiPV never replaces it), else the first MultiPV line's.
#[allow(clippy::too_many_arguments)]
pub fn difficulty_input(
    fen_before: &str,
    played_uci: &str,
    best_uci: Option<&str>,
    win_before: f64,
    win_after: f64,
    multipv: Option<&[MultipvLine]>,
    elo_self: Option<i32>,
    elo_oppo: Option<i32>,
) -> DifficultyInput {
    let mut lines: Vec<(String, f64)> = multipv
        .unwrap_or_default()
        .iter()
        .map(|l| (l.uci.clone(), win_percent(l.score)))
        .collect();
    let best = best_uci
        .map(str::to_owned)
        .or_else(|| multipv.and_then(|m| m.first()).map(|l| l.uci.clone()));
    if let Some(b) = &best
        && !lines.iter().any(|(u, _)| u == b)
    {
        lines.push((b.clone(), win_before));
    }
    lines.push((played_uci.to_owned(), win_after));
    DifficultyInput {
        fen: fen_before.to_owned(),
        elo_self,
        elo_oppo,
        lines: Some(lines),
        best_uci: best,
    }
}

/// Enriches one game. Fails only on an engine or store error, or cancellation; Maia and the
/// tablebase never fail it.
pub async fn enrich_game(
    deps: &EnrichDeps,
    input: &EnrichInput<'_>,
) -> Result<Enriched, EnrichError> {
    let records = input.records;
    let n = records.len();
    let requested: Vec<bool> = records
        .iter()
        .map(|r| input.colors.contains(&r.color))
        .collect();
    let critical: HashSet<u32> = select_critical_plies(records, input.colors, MAX_CRITICAL_PLIES)
        .into_iter()
        .collect();
    let fen_after = |i: usize| input.parsed.plies.get(i).map(|p| p.fen_after.as_str());
    let eval_pv = |i: usize| {
        input
            .evals
            .get(i)
            .map(|e| e.pv.as_slice())
            .unwrap_or_default()
    };
    let mut stages = BTreeMap::new();

    // MultiPV on critical plies, plus fen_after where the cached reply PV is empty.
    let t = Instant::now();
    let mut wanted: Vec<(String, String)> = Vec::new();
    let mut seen = HashSet::new();
    let mut want = |fen: &str, wanted: &mut Vec<(String, String)>| {
        if analysable(fen) {
            let key = fen_key(fen);
            if seen.insert(key.clone()) {
                wanted.push((key, fen.to_owned()));
            }
        }
    };
    for (i, r) in records.iter().enumerate() {
        if !critical.contains(&r.ply) {
            continue;
        }
        want(&r.fen_before, &mut wanted);
        if eval_pv(i + 1).is_empty()
            && let Some(after) = fen_after(i)
        {
            want(after, &mut wanted);
        }
    }
    let keys: Vec<String> = wanted.iter().map(|(k, _)| k.clone()).collect();
    let mut multipv = deps.store.get_multipv(&keys).await?;
    let mut fresh = Vec::new();
    for (key, fen) in &wanted {
        if multipv.contains_key(key) {
            continue;
        }
        let lines = to_multipv(analyse(deps, fen, MULTIPV_NODES, MULTIPV).await?);
        if !lines.is_empty() {
            multipv.insert(key.clone(), lines.clone());
            fresh.push((key.clone(), lines));
        }
    }
    deps.store.put_multipv(&fresh).await?;
    stages.insert("multipv_ms".to_owned(), ms(t));
    stages.insert("multipv_positions".to_owned(), wanted.len() as f64);
    stages.insert("multipv_engine_calls".to_owned(), fresh.len() as f64);
    let lines_of = |fen: &str| -> Option<&Vec<MultipvLine>> {
        valid(fen).then(|| multipv.get(&fen_key(fen))).flatten()
    };

    // Threat probe on the null-move position of eligible requested plies.
    let t = Instant::now();
    let mut null_fens: Vec<Option<String>> = vec![None; n];
    let mut threat_wanted: Vec<(String, String)> = Vec::new();
    let mut seen = HashSet::new();
    for (i, r) in records.iter().enumerate() {
        if !requested[i] || r.decided || !valid(&r.fen_before) {
            continue;
        }
        let Some(null) = null_move_fen(&r.fen_before).filter(|f| analysable(f)) else {
            continue;
        };
        let key = fen_key(&null);
        if seen.insert(key.clone()) {
            threat_wanted.push((key, null.clone()));
        }
        null_fens[i] = Some(null);
    }
    let keys: Vec<String> = threat_wanted.iter().map(|(k, _)| k.clone()).collect();
    let mut threats: HashMap<String, ThreatEntry> = deps.store.get_threat(&keys).await?;
    let mut fresh = Vec::new();
    for (key, fen) in &threat_wanted {
        if threats.contains_key(key) {
            continue;
        }
        let lines = analyse(deps, fen, THREAT_NODES, 1).await?;
        if let Some(l) = lines.into_iter().min_by_key(|l| l.multipv) {
            let entry = (l.score, l.pv);
            threats.insert(key.clone(), entry.clone());
            fresh.push((key.clone(), entry));
        }
    }
    deps.store.put_threat(&fresh).await?;
    stages.insert("threat_ms".to_owned(), ms(t));
    stages.insert("threat_positions".to_owned(), threat_wanted.len() as f64);
    stages.insert("threat_engine_calls".to_owned(), fresh.len() as f64);

    // Tablebase.
    let t = Instant::now();
    let mut tb_before: Vec<Option<Wdl>> = vec![None; n];
    let mut tb_after: Vec<Option<Wdl>> = vec![None; n];
    let mut tb_probes = 0usize;
    if let Some(tb) = &deps.tb {
        let max = tb.max_pieces();
        let mut cache: HashMap<String, Option<Wdl>> = HashMap::new();
        for (i, r) in records.iter().enumerate() {
            if !requested[i] {
                continue;
            }
            for (fen, slot, sign) in [
                (Some(r.fen_before.as_str()), &mut tb_before[i], 1),
                (fen_after(i), &mut tb_after[i], -1),
            ] {
                let Some(fen) = fen.filter(|f| valid(f) && piece_count(f) <= max) else {
                    continue;
                };
                let key = fen_key(fen);
                let wdl = match cache.get(&key) {
                    Some(w) => *w,
                    None => {
                        tb_probes += 1;
                        let w = cancellable(&deps.cancel, tb.probe(fen)).await?;
                        cache.insert(key, w);
                        w
                    }
                };
                *slot = wdl.map(|w| sign * w);
            }
        }
    }
    stages.insert("tb_ms".to_owned(), ms(t));
    stages.insert("tb_probes".to_owned(), tb_probes as f64);

    // Difficulty: Maia only.
    let t = Instant::now();
    let mut difficulty: Vec<Option<DifficultyOutcome>> = vec![None; n];
    let mut inputs = Vec::new();
    let mut input_idx = Vec::new();
    for (i, r) in records.iter().enumerate() {
        if !requested[i] || r.decided {
            continue;
        }
        if r.ply < DIFFICULTY_MIN_PLY {
            difficulty[i] = Some(DifficultyOutcome::NotApplicable {
                reason: NotApplicableReason::OpeningPly,
            });
            continue;
        }
        if !valid(&r.fen_before) {
            difficulty[i] = Some(DifficultyOutcome::NotApplicable {
                reason: NotApplicableReason::InvalidFen,
            });
            continue;
        }
        let (elo_self, elo_oppo) = match r.color {
            Color::White => (input.elo_white, input.elo_black),
            Color::Black => (input.elo_black, input.elo_white),
        };
        let lines = critical
            .contains(&r.ply)
            .then(|| lines_of(&r.fen_before))
            .flatten();
        inputs.push(difficulty_input(
            &r.fen_before,
            &r.uci,
            r.best_uci.as_deref(),
            r.win_before,
            r.win_after,
            lines.map(Vec::as_slice),
            elo_self,
            elo_oppo,
        ));
        input_idx.push(i);
    }
    if !inputs.is_empty() {
        let outcomes = match &deps.difficulty {
            Some(src) => cancellable(&deps.cancel, src.estimate(&inputs)).await?,
            None => select_difficulty_provider(None).estimate(&inputs).await,
        };
        let pending = outcomes.iter().filter(|o| o.is_pending()).count();
        if pending > 0 {
            tracing::warn!(
                pending,
                "maia could not answer; difficulty pending_maia (re-enriched once Maia is healthy)"
            );
        }
        for (i, o) in input_idx.into_iter().zip(outcomes) {
            difficulty[i] = Some(o);
        }
    }
    stages.insert("difficulty_ms".to_owned(), ms(t));
    let difficulty_model = difficulty
        .iter()
        .flatten()
        .find_map(|o| o.difficulty().map(|d| d.model.clone()));

    let raw = records
        .iter()
        .enumerate()
        .map(|(i, r)| {
            if !requested[i] {
                return None;
            }
            let is_critical = critical.contains(&r.ply);
            let before_lines = if is_critical {
                lines_of(&r.fen_before)
            } else {
                None
            };
            let first_pv = |lines: Option<&Vec<MultipvLine>>| {
                lines
                    .and_then(|l| l.first())
                    .map(|l| l.pv.clone())
                    .unwrap_or_default()
            };
            // The fallback must start with the main eval's best move (MultiPV never replaces
            // it): the probe's line for that move, or nothing.
            let best_pv = match eval_pv(i) {
                [] => r
                    .best_uci
                    .as_deref()
                    .and_then(|b| before_lines?.iter().find(|l| l.uci == b))
                    .map(|l| l.pv.clone())
                    .unwrap_or_default(),
                pv => pv.to_vec(),
            };
            let refutation_pv = match eval_pv(i + 1) {
                [] if is_critical => first_pv(fen_after(i).and_then(lines_of)),
                pv => pv.to_vec(),
            };
            let threat = null_fens[i].as_ref().and_then(|null| {
                threats.get(&fen_key(null)).map(|(score, pv)| ThreatProbe {
                    null_fen: null.clone(),
                    pv: pv.clone(),
                    score: *score,
                })
            });
            Some(RawMoveFeatures {
                ply: r.ply,
                multipv: before_lines.cloned(),
                best_pv,
                refutation_pv,
                threat,
                tb_wdl_before: tb_before[i],
                tb_wdl_after: tb_after[i],
                difficulty: difficulty[i].as_ref().and_then(|o| o.difficulty().cloned()),
            })
        })
        .collect();

    Ok(Enriched {
        raw,
        difficulty,
        difficulty_model,
        stages,
    })
}

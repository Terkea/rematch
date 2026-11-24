//! Engine depth against the judgements (EXP-0008): `cli lab positions|evaluate|judge`.
//!
//! The three steps split the production analysis of a game at the engine, so the lab can search
//! the same positions at several node counts, keep every search, and judge each game on any of
//! them, without running a database or a worker:
//!
//! 1. [`positions`] (pure): the positions `analysis::evaluate::evaluate_game` asks the engine for,
//!    per game, in its order: the start and the position after every ply, each distinct position
//!    once, the final position left out when the game ends in mate, stalemate or a draw (production
//!    scores it without a search).
//! 2. [`evaluate`]: each position searched by **Stockfish** the way `evaluate_game` searches it
//!    (one thread, `ucinewgame`, `go nodes N`, MultiPV 1, the last complete line), at the request's
//!    node count, with the production hash (16 MB) unless the request says otherwise and, only when
//!    asked, the Syzygy tables (`SYZYGY_PATH`), as the experiment's deep reference is. The score is
//!    the engine's own, from the side to move's point of view.
//! 3. [`judge`] (pure): each game through `evaluate_game` with those searches as its engine and an
//!    empty cache, then `build_move_records` and `game_accuracy`, exactly as the `analyse-game`
//!    job computes them. Lichess's own evaluations, when the request gives them, go in where
//!    production puts a Lichess game's (`games.lichess_evals`): before the engine.
//!
//! None of this changes the product: the manifest's components are only called.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use analysis::eval_cache::MemoryEvalCache;
use analysis::evaluate::{EvaluateDeps, evaluate_game};
use chess_core::game_accuracy::game_accuracy;
use chess_core::move_record::build_move_records;
use chess_core::pgn::parse_pgn;
use chess_core::score::win_percent;
use chess_core::{ClockInfo, Color, EngineScore};
use engine::uci::{StartOptions, UciEngine};
use engine::{AnalyseOptions, Analyser, AnalysisLine, AnalysisResult, EngineError};
use futures::FutureExt;
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};

use crate::lab::SCHEMA_VERSION;

fn check_version(v: &str) -> anyhow::Result<()> {
    anyhow::ensure!(
        v == SCHEMA_VERSION,
        "schema_version {v:?} is not {SCHEMA_VERSION:?}"
    );
    Ok(())
}

/// One game of a `positions` or `judge` request.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DepthGame {
    pub id: String,
    pub pgn: String,
    #[serde(default)]
    pub base_seconds: Option<i32>,
    #[serde(default)]
    pub increment_seconds: Option<i32>,
    /// Lichess's evaluation after each ply, White's point of view (the shape production stores in
    /// `games.lichess_evals`); `judge` uses one wherever it is present, as production does.
    #[serde(default)]
    pub lichess_evals: Option<Vec<Option<EngineScore>>>,
}

// ---- positions ------------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PositionsRequest {
    pub schema_version: String,
    pub games: Vec<DepthGame>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GamePositions {
    pub id: String,
    /// Plies in the game.
    pub plies: u32,
    /// Whether the final position is mate, stalemate or a draw by rule (scored without a search).
    pub terminal: bool,
    /// The FENs the engine is asked for, in `evaluate_game`'s order, each once.
    pub fens: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PositionsResponse {
    pub schema_version: String,
    pub algorithm_id: String,
    pub games: Vec<GamePositions>,
}

/// An engine that answers every search with a fixed score and records what it was asked.
#[derive(Default)]
struct Recorder {
    asked: Mutex<Vec<String>>,
}

impl Analyser for Recorder {
    fn analyse<'a>(
        &'a self,
        fen: &'a str,
        _opts: AnalyseOptions,
    ) -> BoxFuture<'a, Result<AnalysisResult, EngineError>> {
        self.asked
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(fen.to_owned());
        futures::future::ready(Ok(line_result(EngineScore::Cp(0), None))).boxed()
    }
}

fn line_result(score: EngineScore, best: Option<&str>) -> AnalysisResult {
    AnalysisResult {
        lines: vec![AnalysisLine {
            multipv: 1,
            score,
            pv: best.map(|b| vec![b.to_owned()]).unwrap_or_default(),
            depth: 0,
            nodes: 0,
        }],
        wall: Duration::ZERO,
        cpu: None,
    }
}

fn deps<'a>(analyser: &'a dyn Analyser, cache: &'a MemoryEvalCache) -> EvaluateDeps<'a> {
    EvaluateDeps {
        analyser,
        cache,
        // One at a time, so the order of the engine's requests is the order of the game.
        concurrency: 1,
        on_progress: None,
        cancel: None,
        deadline: None,
    }
}

/// `cli lab positions`.
pub async fn positions(
    req: &PositionsRequest,
    algorithm_id: &str,
) -> anyhow::Result<PositionsResponse> {
    check_version(&req.schema_version)?;
    let mut games = Vec::with_capacity(req.games.len());
    for g in &req.games {
        let parsed = parse_pgn(&g.pgn).map_err(|e| anyhow::anyhow!("game {}: {e}", g.id))?;
        let recorder = Recorder::default();
        let cache = MemoryEvalCache::default();
        evaluate_game(&deps(&recorder, &cache), &parsed, None)
            .await
            .map_err(|e| anyhow::anyhow!("game {}: {e}", g.id))?;
        games.push(GamePositions {
            id: g.id.clone(),
            plies: u32::try_from(parsed.plies.len()).unwrap_or(u32::MAX),
            terminal: parsed.terminal.is_some(),
            fens: recorder
                .asked
                .into_inner()
                .unwrap_or_else(|e| e.into_inner()),
        });
    }
    Ok(PositionsResponse {
        schema_version: SCHEMA_VERSION.to_owned(),
        algorithm_id: algorithm_id.to_owned(),
        games,
    })
}

// ---- evaluate -------------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvaluateRequest {
    pub schema_version: String,
    /// Nodes per search (`go nodes`).
    pub nodes: u64,
    /// `setoption name Hash` in MB; default the production `ENGINE_HASH_MB` (16).
    #[serde(default)]
    pub hash_mb: Option<u32>,
    /// Probe the Syzygy tables at `SYZYGY_PATH` (production does not).
    #[serde(default)]
    pub syzygy: bool,
    /// Stockfish processes, each searching one position at a time; default 1. It changes the
    /// speed, never a result.
    #[serde(default)]
    pub engines: Option<usize>,
    pub fens: Vec<String>,
}

/// One search, as `evaluate_game` reads it: the last complete line of MultiPV 1.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Search {
    pub fen: String,
    /// The side to move's point of view, as the engine reports it.
    pub score: EngineScore,
    pub best_uci: Option<String>,
    pub depth: u32,
    pub nodes: u64,
    /// The engine process's CPU time for the search, in milliseconds (not deterministic; for
    /// costing only).
    pub cpu_ms: Option<f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EvaluateResponse {
    pub schema_version: String,
    pub algorithm_id: String,
    pub nodes: u64,
    pub hash_mb: u32,
    pub syzygy: bool,
    pub threads: u32,
    pub searches: Vec<Search>,
}

pub struct EvaluateOptions {
    pub stockfish: PathBuf,
    /// `SYZYGY_PATH`, required when the request asks for the tables.
    pub syzygy_path: Option<String>,
}

/// Time allowed for one search: the production 30 s, or more for a deep one at an assumed
/// 200,000 nodes a second (a failure, never a different result).
fn timeout_for(nodes: u64) -> Duration {
    engine::DEFAULT_ANALYSIS_TIMEOUT.max(Duration::from_secs(30 + nodes / 200_000))
}

/// `cli lab evaluate`.
pub async fn evaluate(
    req: &EvaluateRequest,
    o: &EvaluateOptions,
    algorithm_id: &str,
) -> anyhow::Result<EvaluateResponse> {
    check_version(&req.schema_version)?;
    anyhow::ensure!(req.nodes > 0, "nodes must be positive");
    let hash_mb = req.hash_mb.unwrap_or(engine::ENGINE_HASH_MB);
    anyhow::ensure!((1..=4096).contains(&hash_mb), "hash_mb must be 1 to 4096");
    let syzygy_path = if req.syzygy {
        let p = o.syzygy_path.clone().filter(|p| !p.is_empty());
        anyhow::ensure!(p.is_some(), "syzygy needs SYZYGY_PATH");
        p
    } else {
        None
    };
    for f in &req.fens {
        engine::uci::canonical_fen(f).map_err(|e| anyhow::anyhow!("{e}"))?;
    }
    let engines = req
        .engines
        .unwrap_or(1)
        .clamp(1, 64)
        .min(req.fens.len().max(1));
    let queue = Arc::new(Mutex::new(
        req.fens.iter().cloned().enumerate().collect::<Vec<_>>(),
    ));
    let opts = AnalyseOptions {
        nodes: req.nodes,
        multipv: 1,
        timeout: timeout_for(req.nodes),
    };
    let mut tasks = Vec::new();
    for _ in 0..engines {
        let queue = queue.clone();
        let start = StartOptions {
            path: o.stockfish.clone(),
            threads: Some(engine::ENGINE_THREADS),
            hash_mb: Some(hash_mb),
            syzygy_path: syzygy_path.clone(),
            handshake_timeout: None,
        };
        tasks.push(tokio::spawn(async move {
            let mut uci = UciEngine::start(start).await?;
            let mut out = Vec::new();
            loop {
                let next = queue.lock().unwrap_or_else(|e| e.into_inner()).pop();
                let Some((i, fen)) = next else { break };
                let res = uci.analyse(&fen, opts).await?;
                let line = res
                    .lines
                    .into_iter()
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("no analysis for {fen}"))?;
                out.push((
                    i,
                    Search {
                        fen,
                        score: line.score,
                        best_uci: line.pv.first().cloned(),
                        depth: line.depth,
                        nodes: line.nodes,
                        cpu_ms: res.cpu.map(|c| c.as_secs_f64() * 1000.0),
                    },
                ));
            }
            uci.quit().await;
            anyhow::Ok(out)
        }));
    }
    let mut found: BTreeMap<usize, Search> = BTreeMap::new();
    for t in tasks {
        for (i, s) in t.await?? {
            found.insert(i, s);
        }
    }
    Ok(EvaluateResponse {
        schema_version: SCHEMA_VERSION.to_owned(),
        algorithm_id: algorithm_id.to_owned(),
        nodes: req.nodes,
        hash_mb,
        syzygy: req.syzygy,
        threads: engine::ENGINE_THREADS,
        searches: found.into_values().collect(),
    })
}

// ---- judge ----------------------------------------------------------------------------------

/// A search to judge with: the side to move's score and best move, as `evaluate` returns them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GivenSearch {
    pub fen: String,
    pub score: EngineScore,
    #[serde(default)]
    pub best_uci: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JudgeRequest {
    pub schema_version: String,
    pub games: Vec<DepthGame>,
    /// Every position any game asks for (`positions`), by FEN.
    pub searches: Vec<GivenSearch>,
}

/// One move as the `analyse-game` job stores it (`move_records`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct JudgedMove {
    pub ply: u32,
    pub color: Color,
    pub uci: String,
    pub best_uci: Option<String>,
    pub win_before: f64,
    pub win_after: f64,
    pub accuracy: f64,
    /// `inaccuracy`, `mistake`, `blunder` or null.
    pub judgement: Option<String>,
    pub decided: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct JudgedGame {
    pub id: String,
    pub moves: Vec<JudgedMove>,
    pub accuracy_white: f64,
    pub accuracy_black: f64,
    /// Positions scored from `lichess_evals` rather than a search.
    pub lichess_used: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JudgeResponse {
    pub schema_version: String,
    pub algorithm_id: String,
    pub games: Vec<JudgedGame>,
}

/// The given searches as an engine; a position it was not given is an error.
struct Given<'a> {
    by_fen: &'a HashMap<&'a str, &'a GivenSearch>,
}

impl Analyser for Given<'_> {
    fn analyse<'a>(
        &'a self,
        fen: &'a str,
        _opts: AnalyseOptions,
    ) -> BoxFuture<'a, Result<AnalysisResult, EngineError>> {
        let out = match self.by_fen.get(fen) {
            Some(s) => Ok(line_result(s.score, s.best_uci.as_deref())),
            None => Err(EngineError::InvalidFen {
                fen: fen.to_owned(),
                reason: "no search given for this position".to_owned(),
            }),
        };
        futures::future::ready(out).boxed()
    }
}

/// `cli lab judge`.
pub async fn judge(req: &JudgeRequest, algorithm_id: &str) -> anyhow::Result<JudgeResponse> {
    check_version(&req.schema_version)?;
    let by_fen: HashMap<&str, &GivenSearch> =
        req.searches.iter().map(|s| (s.fen.as_str(), s)).collect();
    let given = Given { by_fen: &by_fen };
    let mut games = Vec::with_capacity(req.games.len());
    for g in &req.games {
        let parsed = parse_pgn(&g.pgn).map_err(|e| anyhow::anyhow!("game {}: {e}", g.id))?;
        let cache = MemoryEvalCache::default();
        let evaluated = evaluate_game(&deps(&given, &cache), &parsed, g.lichess_evals.as_deref())
            .await
            .map_err(|e| anyhow::anyhow!("game {}: {e}", g.id))?;
        let clock = match (g.base_seconds, g.increment_seconds) {
            (Some(base), Some(inc)) => Some(ClockInfo {
                base_seconds: f64::from(base),
                increment_seconds: f64::from(inc),
            }),
            _ => None,
        };
        let evals = evaluated.evals;
        let records = build_move_records(&parsed, &evals, clock)
            .map_err(|e| anyhow::anyhow!("game {}: {e}", g.id))?;
        let wins: Vec<f64> = evals.iter().map(|e| win_percent(e.score)).collect();
        games.push(JudgedGame {
            id: g.id.clone(),
            moves: records
                .iter()
                .map(|r| JudgedMove {
                    ply: r.ply,
                    color: r.color,
                    uci: r.uci.clone(),
                    best_uci: r.best_uci.clone(),
                    win_before: r.win_before,
                    win_after: r.win_after,
                    accuracy: r.accuracy,
                    judgement: r.judgement.map(|j| j.as_str().to_owned()),
                    decided: r.decided,
                })
                .collect(),
            accuracy_white: game_accuracy(&wins, Color::White),
            accuracy_black: game_accuracy(&wins, Color::Black),
            lichess_used: u32::try_from(evaluated.stats.lichess_used).unwrap_or(u32::MAX),
        });
    }
    Ok(JudgeResponse {
        schema_version: SCHEMA_VERSION.to_owned(),
        algorithm_id: algorithm_id.to_owned(),
        games,
    })
}

//! White-POV evals for every position of a game: cache first, then supplied Lichess evals, then
//! the engine. Port of `apps/worker/src/analysis/evaluate-game.ts`, plus per-stage statistics.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Instant;

use chess_core::fen::fen_key;
use chess_core::score::flip_score;
use chess_core::{Color, EngineScore, ParsedGame, PositionEval, Terminal};
use engine::{
    ANALYSIS_PROTOCOL, AnalyseOptions, Analyser, DEFAULT_ANALYSIS_TIMEOUT, ENGINE_NODES,
    EngineError,
};
use futures::future::BoxFuture;
use tokio_util::sync::CancellationToken;

use crate::deadline::{ANALYSE_DEADLINE, JobDeadlineError};
use crate::eval_cache::{EvalCache, EvalCacheEntry, EvalSource};

/// Heartbeat callback: `(done, total)` engine positions. Errors are ignored.
pub type ProgressFn<'a> =
    &'a (dyn Fn(usize, usize) -> BoxFuture<'static, Result<(), String>> + Sync);

/// Which deadline a cancelled token stands for, reported in `EvaluateError::Deadline`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeadlineInfo {
    /// `"analyse-game"` or `"import-account"`.
    pub kind: &'static str,
    pub ms: u64,
}

impl Default for DeadlineInfo {
    fn default() -> Self {
        Self {
            kind: "analyse-game",
            ms: u64::try_from(ANALYSE_DEADLINE.as_millis()).unwrap_or(u64::MAX),
        }
    }
}

pub struct EvaluateDeps<'a> {
    pub analyser: &'a dyn Analyser,
    pub cache: &'a dyn EvalCache,
    /// Parallel engine requests (default 4).
    pub concurrency: usize,
    /// Called after each engine result is persisted. Because it runs only when work completes, a
    /// hung loop stops calling it, which is how the reconciler spots a stuck analysis.
    pub on_progress: Option<ProgressFn<'a>>,
    /// Cancelling it (a job deadline or shutdown) aborts the evaluation with `Deadline`, even
    /// mid-search.
    pub cancel: Option<CancellationToken>,
    /// Kind and duration reported when `cancel` fires. `None` means analyse-game at 10 minutes.
    pub deadline: Option<DeadlineInfo>,
}

/// Statistics of one `evaluate_game` call. Also returned (partial) when the call fails.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EvalStats {
    /// Positions in the game (`plies + 1`), counted per occurrence (repeated positions count each
    /// time).
    pub positions: usize,
    /// Positions (per occurrence, terminal excluded) served from the cache.
    pub cache_hits: usize,
    /// Positions (per occurrence) served from the supplied Lichess evals.
    pub lichess_used: usize,
    /// Distinct positions (per distinct key) whose engine analysis completed successfully. On a
    /// failed call this is the number finished before the failure.
    pub engine_positions: usize,
    /// Sum of the time spent inside cache calls across all workers (it can exceed the elapsed
    /// time when workers overlap).
    pub cache_ms: f64,
    /// SUM over every completed analysis of the engine-reported wall time. Workers run in
    /// parallel, so this exceeds `engine_stage_elapsed_ms` by about the concurrency factor.
    pub engine_wall_ms: f64,
    /// SUM over every completed analysis of the engine-reported CPU time; `None` when the engine
    /// reported none.
    pub engine_cpu_ms: Option<f64>,
    /// Real elapsed wall-clock span of the engine stage (first worker start to last worker end).
    pub engine_stage_elapsed_ms: f64,
    /// Real elapsed wall-clock span of the whole `evaluate_game` call.
    pub total_elapsed_ms: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Evaluated {
    pub evals: Vec<PositionEval>,
    pub stats: EvalStats,
}

#[derive(thiserror::Error, Debug)]
pub enum EvaluateError {
    #[error(transparent)]
    Engine(#[from] EngineError),
    #[error("eval cache: {0}")]
    Cache(#[from] sqlx::Error),
    #[error(transparent)]
    Deadline(#[from] JobDeadlineError),
    #[error("{0}")]
    NoAnalysis(String),
}

/// A failed evaluation together with the statistics gathered up to the failure.
#[derive(thiserror::Error, Debug)]
#[error("{error}")]
pub struct EvaluateFailure {
    #[source]
    pub error: EvaluateError,
    /// Boxed to keep the `Err` variant small.
    pub stats: Box<EvalStats>,
}

fn cancelled(deps: &EvaluateDeps<'_>) -> EvaluateError {
    let info = deps.deadline.unwrap_or_default();
    EvaluateError::Deadline(JobDeadlineError {
        kind: info.kind,
        ms: info.ms,
    })
}

fn ms(d: std::time::Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

/// Fixed white-POV score for a final position that is checkmate, stalemate or another draw.
fn terminal_eval(game: &ParsedGame) -> Option<PositionEval> {
    let terminal = game.terminal?;
    let cp = match terminal {
        // The side to move is mated, so the side that just moved won.
        Terminal::Checkmate => match game.plies.last().map(|p| p.color) {
            Some(Color::White) => 1000,
            _ => -1000,
        },
        Terminal::Stalemate | Terminal::Draw => 0,
    };
    Some(PositionEval {
        score: EngineScore::Cp(cp),
        best_uci: None,
        pv: vec![],
    })
}

#[derive(Default)]
struct Shared {
    queue: VecDeque<(String, String)>,
    analysed: HashMap<String, PositionEval>,
    failure: Option<EvaluateError>,
    wall_ms: f64,
    cpu_ms: Option<f64>,
    cache_ms: f64,
}

/// White-POV evals for every position of the game (`plies.len() + 1`).
/// `lichess_evals[i]` is the eval after ply `i + 1`.
pub async fn evaluate_game(
    deps: &EvaluateDeps<'_>,
    game: &ParsedGame,
    lichess_evals: Option<&[Option<EngineScore>]>,
) -> Result<Evaluated, EvaluateFailure> {
    let started = Instant::now();
    let mut stats = EvalStats::default();
    let outcome = run(deps, game, lichess_evals, &mut stats).await;
    stats.total_elapsed_ms = ms(started.elapsed());
    match outcome {
        Ok(evals) => Ok(Evaluated { evals, stats }),
        Err(error) => Err(EvaluateFailure {
            error,
            stats: Box::new(stats),
        }),
    }
}

async fn run(
    deps: &EvaluateDeps<'_>,
    game: &ParsedGame,
    lichess_evals: Option<&[Option<EngineScore>]>,
    stats: &mut EvalStats,
) -> Result<Vec<PositionEval>, EvaluateError> {
    let cancel = deps.cancel.clone().unwrap_or_default();
    let fens: Vec<&str> = std::iter::once(game.start_fen.as_str())
        .chain(game.plies.iter().map(|p| p.fen_after.as_str()))
        .collect();
    let keys: Vec<String> = fens.iter().map(|f| fen_key(f)).collect();
    let last = fens.len() - 1;
    let mut result: Vec<Option<PositionEval>> = vec![None; fens.len()];
    stats.positions = fens.len();

    if let Some(t) = terminal_eval(game) {
        result[last] = Some(t);
    }

    let wanted: Vec<String> = keys
        .iter()
        .enumerate()
        .filter(|(i, _)| result[*i].is_none())
        .map(|(_, k)| k.clone())
        .collect();
    if cancel.is_cancelled() {
        return Err(cancelled(deps));
    }
    let t = Instant::now();
    let cached = deps.cache.get_many(&wanted, ANALYSIS_PROTOCOL).await?;
    stats.cache_ms += ms(t.elapsed());

    let mut puts: Vec<EvalCacheEntry> = Vec::new();
    let mut put_keys: HashSet<&str> = HashSet::new();
    let mut miss_keys: HashSet<&str> = HashSet::new();
    let mut misses: VecDeque<(String, String)> = VecDeque::new();

    for i in 0..fens.len() {
        if result[i].is_some() {
            continue;
        }
        let key = keys[i].as_str();
        if let Some(hit) = cached.get(key) {
            result[i] = Some(hit.clone());
            stats.cache_hits += 1;
            continue;
        }
        let lichess = if i > 0 {
            lichess_evals.and_then(|l| l.get(i - 1)).copied().flatten()
        } else {
            None
        };
        if let Some(score) = lichess {
            let ev = PositionEval {
                score,
                best_uci: None,
                pv: vec![],
            };
            result[i] = Some(ev.clone());
            stats.lichess_used += 1;
            if put_keys.insert(key) {
                puts.push(EvalCacheEntry {
                    fen_key: key.to_string(),
                    protocol: ANALYSIS_PROTOCOL.to_string(),
                    eval: ev,
                    source: EvalSource::Lichess,
                });
            }
            continue;
        }
        if miss_keys.insert(key) {
            misses.push_back((key.to_string(), fens[i].to_string()));
        }
    }

    if cancel.is_cancelled() {
        return Err(cancelled(deps));
    }
    // Persist Lichess evals first, then each engine result as soon as it resolves, so a failure
    // or crash midway keeps all completed work.
    let t = Instant::now();
    deps.cache.put_many(&puts).await?;
    stats.cache_ms += ms(t.elapsed());

    let total = misses.len();
    let n = deps.concurrency.max(1).min(total);
    let shared = Mutex::new(Shared {
        queue: misses,
        ..Shared::default()
    });
    let failed = AtomicBool::new(false);
    let done = AtomicUsize::new(0);

    let worker = || async {
        while !failed.load(Ordering::SeqCst) {
            let next = shared
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .queue
                .pop_front();
            let Some((key, fen)) = next else { return };
            if let Err(err) = process(deps, &cancel, &shared, &done, total, &key, &fen).await {
                let mut s = shared.lock().unwrap_or_else(|e| e.into_inner());
                if s.failure.is_none() {
                    s.failure = Some(err);
                }
                failed.store(true, Ordering::SeqCst);
                return;
            }
        }
    };
    // All workers are awaited, so in-flight calls finish (or are cancelled) before we return.
    let stage = Instant::now();
    futures::future::join_all((0..n).map(|_| worker())).await;
    stats.engine_stage_elapsed_ms = ms(stage.elapsed());

    let s = shared.into_inner().unwrap_or_else(|e| e.into_inner());
    stats.cache_ms += s.cache_ms;
    stats.engine_wall_ms = s.wall_ms;
    stats.engine_cpu_ms = s.cpu_ms;
    stats.engine_positions = s.analysed.len();
    if let Some(err) = s.failure {
        return Err(err);
    }

    let evals = result
        .into_iter()
        .enumerate()
        .map(|(i, r)| {
            r.or_else(|| s.analysed.get(&keys[i]).cloned())
                .ok_or_else(|| EvaluateError::NoAnalysis(format!("No eval for {}", fens[i])))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(evals)
}

async fn process(
    deps: &EvaluateDeps<'_>,
    cancel: &CancellationToken,
    shared: &Mutex<Shared>,
    done: &AtomicUsize,
    total: usize,
    key: &str,
    fen: &str,
) -> Result<(), EvaluateError> {
    if cancel.is_cancelled() {
        return Err(cancelled(deps));
    }
    let opts = AnalyseOptions {
        nodes: ENGINE_NODES,
        multipv: 1,
        timeout: DEFAULT_ANALYSIS_TIMEOUT,
    };
    let res = tokio::select! {
        biased;
        _ = cancel.cancelled() => return Err(cancelled(deps)),
        r = deps.analyser.analyse(fen, opts) => r?,
    };
    let line = res.lines.into_iter().next().ok_or_else(|| {
        EvaluateError::NoAnalysis(format!("Engine returned no analysis for {fen}"))
    })?;
    let black_to_move = fen.split(' ').nth(1) == Some("b");
    let ev = PositionEval {
        score: if black_to_move {
            flip_score(line.score)
        } else {
            line.score
        },
        best_uci: line.pv.first().cloned(),
        pv: line.pv,
    };
    let entry = [EvalCacheEntry {
        fen_key: key.to_string(),
        protocol: ANALYSIS_PROTOCOL.to_string(),
        eval: ev.clone(),
        source: EvalSource::Stockfish,
    }];
    let t = Instant::now();
    deps.cache.put_many(&entry).await?;
    let cache_ms = ms(t.elapsed());
    {
        let mut s = shared.lock().unwrap_or_else(|e| e.into_inner());
        s.analysed.insert(key.to_string(), ev);
        s.cache_ms += cache_ms;
        s.wall_ms += ms(res.wall);
        if let Some(cpu) = res.cpu {
            s.cpu_ms = Some(s.cpu_ms.unwrap_or(0.0) + ms(cpu));
        }
    }
    let d = done.fetch_add(1, Ordering::SeqCst) + 1;
    if let Some(cb) = deps.on_progress {
        // A failed heartbeat must not fail the analysis.
        let _ = cb(d, total).await;
    }
    Ok(())
}

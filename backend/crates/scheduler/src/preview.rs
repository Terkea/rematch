//! Preview pass at 30k nodes (R2 Task 28, R-13, ADR-R2-15 "Preview pass").
//!
//! A quick first look so users see early results while the full pass is queued. It is the only
//! exception to the one-protocol rule: evals are never cached (no read, no write), results live
//! only in `game_previews`, and the event carries `preview: true`. It never touches
//! `games.analysis_status`, `moves`, `position_evals` or any metric. The full pass replaces it.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use chess_core::fen::fen_key;
use chess_core::game_accuracy::game_accuracy;
use chess_core::move_record::build_move_records;
use chess_core::pgn::parse_pgn;
use chess_core::score::{flip_score, win_percent};
use chess_core::{ClockInfo, Color, EngineScore, ParsedGame, PositionEval, Terminal};
use db::games::{AnalysisStatus, get_game};
use db::stage_metrics::{StageMetrics, record_stage_metrics};
use engine::{AnalyseOptions, Analyser, DEFAULT_ANALYSIS_TIMEOUT};
use events::store::{UserEventType, publish};
use futures::future::BoxFuture;
use futures::{FutureExt, StreamExt, stream};
use queue::queue::{Queue, QueueError};
use queue::runner::{Handler, JobContext};
use queue::{
    EnqueueOptions, Job, JobError, KIND_PREVIEW, PREVIEW_PRIORITY, PreviewPayload, preview_key,
};
use serde_json::{Value, json};
use sqlx::PgPool;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

/// Nodes per position of a preview.
pub const PREVIEW_NODES: u64 = 30_000;
/// Stored in `game_previews.protocol`; never equal to the full analysis protocol.
pub const PREVIEW_PROTOCOL: &str = "sf19-n30k-preview-v1";
/// Attempts of a preview job.
pub const PREVIEW_MAX_ATTEMPTS: i32 = 2;

pub struct PreviewDeps {
    pub pool: PgPool,
    pub analyser: Arc<dyn Analyser>,
    /// Parallel engine requests per game (2).
    pub concurrency: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PreviewOutcome {
    Written,
    /// The full analysis already exists.
    SkippedDone,
    /// Unsupported variant or fewer than 2 plies.
    SkippedUnsupported,
    /// The full analysis finished while the preview ran: no row, no event, no metrics.
    Superseded,
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

fn retryable(message: String) -> JobError {
    JobError::Retryable {
        message,
        code: None,
    }
}

fn terminal_eval(game: &ParsedGame) -> Option<PositionEval> {
    let cp = match game.terminal? {
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

/// White-POV evals of every position at [`PREVIEW_NODES`], MultiPV 1, one engine call per
/// distinct `fen_key`; no cache. Returns the evals and the summed engine wall time (ms).
async fn evaluate(
    deps: &PreviewDeps,
    game: &ParsedGame,
    cancel: &CancellationToken,
) -> Result<(Vec<PositionEval>, f64), JobError> {
    let fens: Vec<&str> = std::iter::once(game.start_fen.as_str())
        .chain(game.plies.iter().map(|p| p.fen_after.as_str()))
        .collect();
    let keys: Vec<String> = fens.iter().map(|f| fen_key(f)).collect();
    let terminal = terminal_eval(game);
    let last = fens.len() - 1;
    let mut seen = HashSet::new();
    let todo: Vec<(String, String)> = fens
        .iter()
        .enumerate()
        .filter(|(i, _)| !(terminal.is_some() && *i == last))
        .filter(|(i, _)| seen.insert(keys[*i].clone()))
        .map(|(i, f)| (keys[i].clone(), (*f).to_owned()))
        .collect();

    let failed_flag = AtomicBool::new(false);
    let failed = &failed_flag;
    let release = || JobError::Release {
        delay: Duration::ZERO,
        reason: "shutdown".to_owned(),
    };
    let results: Vec<Result<(String, PositionEval, f64), JobError>> = stream::iter(todo)
        .map(|(key, fen): (String, String)| async move {
            let fen = fen.as_str();
            if cancel.is_cancelled() {
                return Err(release());
            }
            // Stop at the first failure, as `evaluate_game` does.
            if failed.load(Ordering::SeqCst) {
                return Err(retryable(
                    "skipped after an earlier engine failure".to_owned(),
                ));
            }
            let opts = AnalyseOptions {
                nodes: PREVIEW_NODES,
                multipv: 1,
                timeout: DEFAULT_ANALYSIS_TIMEOUT,
            };
            let res = tokio::select! {
                biased;
                _ = cancel.cancelled() => return Err(release()),
                r = deps.analyser.analyse(fen, opts) => match r {
                    Ok(r) => r,
                    Err(e) => {
                        failed.store(true, Ordering::SeqCst);
                        return Err(retryable(e.to_string()));
                    }
                },
            };
            let wall = ms(res.wall);
            let line = res
                .lines
                .into_iter()
                .next()
                .ok_or_else(|| retryable(format!("Engine returned no analysis for {fen}")))?;
            let black = fen.split(' ').nth(1) == Some("b");
            let ev = PositionEval {
                score: if black {
                    flip_score(line.score)
                } else {
                    line.score
                },
                best_uci: line.pv.first().cloned(),
                pv: line.pv,
            };
            Ok((key, ev, wall))
        })
        .buffer_unordered(deps.concurrency.max(1))
        .collect()
        .await;

    let mut by_key: HashMap<String, PositionEval> = HashMap::new();
    let mut wall = 0.0;
    // Report the first real failure, not a follow-on skip.
    let mut results = results;
    if let Some(i) = results
        .iter()
        .position(|r| matches!(r, Err(JobError::Release { .. })))
    {
        return Err(results.swap_remove(i).err().unwrap_or_else(release));
    }
    if let Some(i) = results.iter().position(|r| {
        matches!(r, Err(JobError::Retryable { message, .. }) if !message.starts_with("skipped after"))
    }) {
        return Err(results.swap_remove(i).err().unwrap_or_else(release));
    }
    for r in results {
        let (k, ev, w) = r?;
        by_key.insert(k, ev);
        wall += w;
    }
    let evals = (0..fens.len())
        .map(|i| {
            if i == last
                && let Some(t) = &terminal
            {
                return Ok(t.clone());
            }
            by_key
                .get(&keys[i])
                .cloned()
                .ok_or_else(|| retryable(format!("No eval for {}", fens[i])))
        })
        .collect::<Result<Vec<_>, JobError>>()?;
    Ok((evals, wall))
}

/// Writes (or replaces) the game's preview. See the module docs.
pub async fn handle_preview(
    deps: &PreviewDeps,
    job: &Job,
    payload: PreviewPayload,
    cancel: CancellationToken,
) -> Result<PreviewOutcome, JobError> {
    let started = Instant::now();
    let game_id = payload.game_id;
    let db_err = |e: sqlx::Error| retryable(format!("database error: {e}"));
    let game = get_game(&deps.pool, game_id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| JobError::Unrecoverable {
            message: format!("Game not found: {game_id}"),
            code: Some("not_found".to_owned()),
        })?;
    if game.analysis_status == AnalysisStatus::Done {
        return Ok(PreviewOutcome::SkippedDone);
    }

    let t = Instant::now();
    let parsed = match parse_pgn(&game.pgn) {
        Ok(p) => p,
        Err(e) if e.is_unsupported() => return Ok(PreviewOutcome::SkippedUnsupported),
        Err(e) => return Err(retryable(e.to_string())),
    };
    let parse_ms = ms(t.elapsed());
    if parsed.plies.len() < 2 {
        return Ok(PreviewOutcome::SkippedUnsupported);
    }

    let (evals, engine_wall_ms) = evaluate(deps, &parsed, &cancel).await?;
    let clock = match (game.base_seconds, game.increment_seconds) {
        (Some(base), Some(inc)) => Some(ClockInfo {
            base_seconds: f64::from(base),
            increment_seconds: f64::from(inc),
        }),
        _ => None,
    };
    let records =
        build_move_records(&parsed, &evals, clock).map_err(|e| retryable(e.to_string()))?;
    let wins: Vec<f64> = evals.iter().map(|e| win_percent(e.score)).collect();
    let white = game_accuracy(&wins, Color::White);
    let black = game_accuracy(&wins, Color::Black);
    let moves: Vec<Value> = records
        .iter()
        .map(|r| {
            json!({
                "ply": r.ply,
                "judgement": r.judgement.map(|j| j.as_str()),
                "deltaW": r.delta_w,
                "accuracy": r.accuracy,
            })
        })
        .collect();

    // The full pass may have finished while this one ran; it deletes the row on completion
    // (Task 31), so never write a preview for a game that is already done.
    let t = Instant::now();
    let inserted = sqlx::query(
        "INSERT INTO game_previews (game_id, protocol, white_accuracy, black_accuracy, moves) \
         SELECT $1, $2, $3, $4, $5 WHERE NOT EXISTS \
           (SELECT 1 FROM games WHERE id = $1 AND analysis_status = 'done') \
         ON CONFLICT (game_id) DO UPDATE SET protocol = EXCLUDED.protocol, \
           white_accuracy = EXCLUDED.white_accuracy, black_accuracy = EXCLUDED.black_accuracy, \
           moves = EXCLUDED.moves, created_at = now()",
    )
    .bind(game_id)
    .bind(PREVIEW_PROTOCOL)
    .bind(white as f32)
    .bind(black as f32)
    .bind(Value::Array(moves))
    .execute(&deps.pool)
    .await
    .map_err(db_err)?;
    if inserted.rows_affected() == 0 {
        return Ok(PreviewOutcome::Superseded);
    }
    let persist_ms = ms(t.elapsed());

    // After the commit: tell the user, marked as a preview.
    let accuracy = if game.player_color == "white" {
        white
    } else {
        black
    };
    let payload = json!({ "gameId": game_id, "accuracy": accuracy, "preview": true });
    if let Err(e) = publish(
        &deps.pool,
        game.account_id,
        UserEventType::GameAnalysed,
        payload,
    )
    .await
    {
        tracing::warn!(game_id = %game_id, error = %e, "preview publish failed");
    }

    let metrics = StageMetrics {
        job_id: Some(job.id),
        kind: "preview-game".to_owned(),
        game_id: Some(game_id),
        account_id: Some(game.account_id),
        total_ms: ms(started.elapsed()),
        stages: [
            ("parse_ms".to_owned(), parse_ms),
            ("engine_wall_ms".to_owned(), engine_wall_ms),
            ("persist_ms".to_owned(), persist_ms),
        ]
        .into(),
    };
    if let Err(e) = record_stage_metrics(&deps.pool, &metrics).await {
        tracing::warn!(game_id = %game_id, error = %e, "recording preview metrics failed");
    }
    Ok(PreviewOutcome::Written)
}

/// The queue handler for [`KIND_PREVIEW`].
pub struct PreviewHandler(pub Arc<PreviewDeps>);

impl Handler for PreviewHandler {
    fn kind(&self) -> &'static str {
        KIND_PREVIEW
    }

    fn handle(&self, job: Job, ctx: JobContext) -> BoxFuture<'_, Result<Value, JobError>> {
        async move {
            let payload: PreviewPayload =
                serde_json::from_value(job.payload.clone()).map_err(|e| {
                    JobError::Unrecoverable {
                        message: format!("invalid preview-game payload: {e}"),
                        code: Some("invalid_payload".to_owned()),
                    }
                })?;
            let outcome = handle_preview(&self.0, &job, payload, ctx.cancel).await?;
            let status = match outcome {
                PreviewOutcome::Written => "written",
                PreviewOutcome::SkippedDone => "skipped_done",
                PreviewOutcome::SkippedUnsupported => "skipped_unsupported",
                PreviewOutcome::Superseded => "superseded",
            };
            Ok(json!({ "status": status }))
        }
        .boxed()
    }
}

/// Enqueues one preview job per game (priority 0, owned by the account, deduped on
/// `preview:<game>`). Returns how many were newly queued.
pub async fn enqueue_previews(
    queue: &Queue,
    account_id: Uuid,
    game_ids: &[Uuid],
) -> Result<usize, QueueError> {
    let jobs: Vec<(Value, EnqueueOptions)> = game_ids
        .iter()
        .map(|g| {
            (
                json!({ "gameId": g }),
                EnqueueOptions {
                    dedupe_key: Some(preview_key(*g)),
                    user_key: Some(account_id),
                    priority: PREVIEW_PRIORITY,
                    max_attempts: PREVIEW_MAX_ATTEMPTS,
                    ..EnqueueOptions::default()
                },
            )
        })
        .collect();
    let n = queue.enqueue_many(KIND_PREVIEW, &jobs).await?;
    Ok(usize::try_from(n).unwrap_or(usize::MAX))
}

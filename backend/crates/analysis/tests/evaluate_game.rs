//! Port of apps/worker/test/analysis/evaluate-game.test.ts. The PGN parser (Task 5) is not on this
//! branch, so games are built from the same positions with their chess.js FENs.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use analysis::deadline::JobDeadlineError;
use analysis::eval_cache::{EvalCache, EvalCacheEntry, EvalSource, MemoryEvalCache};
use analysis::evaluate::{DeadlineInfo, EvaluateDeps, EvaluateError, evaluate_game};
use chess_core::fen::fen_key;
use chess_core::{Color, EngineScore, ParsedGame, Ply, PositionEval, START_FEN, Terminal};
use engine::{ANALYSIS_PROTOCOL, ENGINE_NODES};
use futures::future::BoxFuture;
use testkit::fake_engine::{FAKE_CPU, FAKE_WALL, FakeAnalyser};
use tokio_util::sync::CancellationToken;

const E4: &str = "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1";
const E5: &str = "rnbqkbnr/pppp1ppp/8/4p3/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 2";
const NF3: &str = "rnbqkbnr/pppp1ppp/8/4p3/4P3/5N2/PPPP1PPP/RNBQKB1R b KQkq - 1 2";
const NC6: &str = "r1bqkbnr/pppp1ppp/2n5/4p3/4P3/5N2/PPPP1PPP/RNBQKB1R w KQkq - 2 3";
const BB5: &str = "r1bqkbnr/pppp1ppp/2n5/1B2p3/4P3/5N2/PPPP1PPP/RNBQK2R b KQkq - 3 3";

fn game(fens: &[&str], terminal: Option<Terminal>) -> ParsedGame {
    let mut before = START_FEN.to_string();
    let plies = fens
        .iter()
        .enumerate()
        .map(|(i, after)| {
            let ply = Ply {
                ply: i as u32 + 1,
                color: if i % 2 == 0 {
                    Color::White
                } else {
                    Color::Black
                },
                san: String::new(),
                uci: String::new(),
                fen_before: before.clone(),
                fen_after: after.to_string(),
                clock_seconds: None,
            };
            before = after.to_string();
            ply
        })
        .collect();
    ParsedGame {
        headers: vec![],
        start_fen: START_FEN.to_string(),
        plies,
        terminal,
    }
}

/// 1. e4 e5 2. Nf3 *
fn short_game() -> ParsedGame {
    game(&[E4, E5, NF3], None)
}

fn keys(g: &ParsedGame) -> Vec<String> {
    std::iter::once(g.start_fen.as_str())
        .chain(g.plies.iter().map(|p| p.fen_after.as_str()))
        .map(fen_key)
        .collect()
}

fn deps<'a>(a: &'a FakeAnalyser, c: &'a dyn EvalCache) -> EvaluateDeps<'a> {
    EvaluateDeps {
        analyser: a,
        cache: c,
        concurrency: 4,
        on_progress: None,
        cancel: None,
        deadline: None,
    }
}

fn stockfish_entry(key: &str, cp: i32) -> EvalCacheEntry {
    EvalCacheEntry {
        fen_key: key.to_string(),
        protocol: ANALYSIS_PROTOCOL.to_string(),
        eval: PositionEval {
            score: EngineScore::Cp(cp),
            best_uci: None,
            pv: vec![],
        },
        source: EvalSource::Stockfish,
    }
}

#[derive(Default)]
struct RecordingCache {
    inner: MemoryEvalCache,
    puts: Mutex<Vec<EvalCacheEntry>>,
}

impl EvalCache for RecordingCache {
    fn get_many<'a>(
        &'a self,
        k: &'a [String],
        p: &'a str,
    ) -> BoxFuture<'a, Result<HashMap<String, PositionEval>, sqlx::Error>> {
        self.inner.get_many(k, p)
    }

    fn put_many<'a>(&'a self, e: &'a [EvalCacheEntry]) -> BoxFuture<'a, Result<(), sqlx::Error>> {
        self.puts.lock().unwrap().extend(e.iter().cloned());
        self.inner.put_many(e)
    }
}

#[tokio::test]
async fn analyses_every_position_once_at_engine_nodes_and_caches_results() {
    let g = short_game();
    let cache = MemoryEvalCache::default();
    let an = FakeAnalyser::new();
    let out = evaluate_game(&deps(&an, &cache), &g, None).await.unwrap();
    assert_eq!(out.evals.len(), g.plies.len() + 1);
    {
        let calls = an.calls.lock().unwrap();
        assert_eq!(calls.len(), 4);
        assert!(calls.iter().all(|c| c.1 == ENGINE_NODES && c.2 == 1));
    }
    let cached = cache.get_many(&keys(&g), ANALYSIS_PROTOCOL).await.unwrap();
    assert_eq!(cached.len(), 4);
}

#[tokio::test]
async fn makes_0_engine_calls_on_a_second_run() {
    let g = short_game();
    let cache = MemoryEvalCache::default();
    let first = FakeAnalyser::new();
    let a = evaluate_game(&deps(&first, &cache), &g, None)
        .await
        .unwrap();
    let second = FakeAnalyser::new();
    let b = evaluate_game(&deps(&second, &cache), &g, None)
        .await
        .unwrap();
    assert_eq!(second.call_count(), 0);
    assert_eq!(b.evals, a.evals);
    assert_eq!(b.stats.engine_positions, 0);
    assert_eq!(b.stats.cache_hits, 4);
}

#[tokio::test]
async fn terminal_positions_not_sent_to_engine_checkmate_gives_cp_1000_for_white() {
    // Five plies, so white made the last move.
    let g = game(
        &[E4, E5, NF3, NC6, "mate fen b - -"],
        Some(Terminal::Checkmate),
    );
    let an = FakeAnalyser::new();
    let cache = MemoryEvalCache::default();
    let out = evaluate_game(&deps(&an, &cache), &g, None).await.unwrap();
    assert_eq!(out.evals.len(), g.plies.len() + 1);
    assert_eq!(out.evals.last().unwrap().score, EngineScore::Cp(1000));
    assert!(!an.called_fens().contains(&"mate fen b - -".to_string()));
    assert_eq!(an.call_count(), g.plies.len());
    // The terminal position is not cached.
    let cached = cache
        .get_many(&[fen_key("mate fen b - -")], ANALYSIS_PROTOCOL)
        .await
        .unwrap();
    assert!(cached.is_empty());
}

#[tokio::test]
async fn gives_minus_1000_when_black_delivers_mate() {
    // 1. f3 e5 2. g4 Qh4#
    let g = game(
        &[
            "rnbqkbnr/pppppppp/8/8/8/5P2/PPPPP1PP/RNBQKBNR b KQkq - 0 1",
            "rnbqkbnr/pppp1ppp/8/4p3/8/5P2/PPPPP1PP/RNBQKBNR w KQkq - 0 2",
            "rnbqkbnr/pppp1ppp/8/4p3/6P1/5P2/PPPPP2P/RNBQKBNR b KQkq - 0 2",
            "rnb1kbnr/pppp1ppp/8/4p3/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 1 3",
        ],
        Some(Terminal::Checkmate),
    );
    let an = FakeAnalyser::new();
    let out = evaluate_game(&deps(&an, &MemoryEvalCache::default()), &g, None)
        .await
        .unwrap();
    assert_eq!(out.evals.last().unwrap().score, EngineScore::Cp(-1000));
}

#[tokio::test]
async fn terminal_positions_not_sent_to_engine_stalemate_gives_cp_0() {
    let g = game(
        &[E4, E5, NF3, NC6, "stalemate fen b - -"],
        Some(Terminal::Stalemate),
    );
    let an = FakeAnalyser::new();
    let out = evaluate_game(&deps(&an, &MemoryEvalCache::default()), &g, None)
        .await
        .unwrap();
    let last = out.evals.last().unwrap();
    assert_eq!(last.score, EngineScore::Cp(0));
    assert_eq!(last.best_uci, None);
    assert!(
        !an.called_fens()
            .contains(&"stalemate fen b - -".to_string())
    );
    assert_eq!(an.call_count(), g.plies.len());
}

#[tokio::test]
async fn converts_side_to_move_scores_to_white_point_of_view() {
    let g = game(&[E4], None);
    let an = FakeAnalyser::with_score(|_| EngineScore::Cp(30));
    let out = evaluate_game(&deps(&an, &MemoryEvalCache::default()), &g, None)
        .await
        .unwrap();
    assert_eq!(out.evals[0].score, EngineScore::Cp(30));
    assert_eq!(out.evals[1].score, EngineScore::Cp(-30));
    assert_eq!(out.evals[0].best_uci.as_deref(), Some("e2e4"));
    assert_eq!(out.evals[0].pv, vec!["e2e4", "e7e5"]);
}

#[tokio::test]
async fn uses_complete_lichess_evals_and_calls_the_engine_only_for_the_start_position() {
    let g = short_game();
    let cache = MemoryEvalCache::default();
    let an = FakeAnalyser::new();
    let lichess = [
        Some(EngineScore::Cp(20)),
        Some(EngineScore::Cp(10)),
        Some(EngineScore::Mate(-3)),
    ];
    let out = evaluate_game(&deps(&an, &cache), &g, Some(&lichess))
        .await
        .unwrap();
    assert_eq!(an.called_fens(), vec![START_FEN.to_string()]);
    let got: Vec<EngineScore> = out.evals[1..].iter().map(|e| e.score).collect();
    assert_eq!(
        got,
        vec![
            EngineScore::Cp(20),
            EngineScore::Cp(10),
            EngineScore::Mate(-3)
        ]
    );
    let stored = cache
        .get_many(&[fen_key(E4)], ANALYSIS_PROTOCOL)
        .await
        .unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!(out.stats.lichess_used, 3);
    assert_eq!(out.stats.engine_positions, 1);
}

#[tokio::test]
async fn lichess_evals_are_stored_with_source_lichess() {
    let rows = RecordingCache::default();
    let g = game(&[E4], None);
    let an = FakeAnalyser::new();
    evaluate_game(&deps(&an, &rows), &g, Some(&[Some(EngineScore::Cp(7))]))
        .await
        .unwrap();
    let puts = rows.puts.lock().unwrap();
    let lichess: Vec<_> = puts
        .iter()
        .filter(|e| e.source == EvalSource::Lichess)
        .collect();
    assert_eq!(lichess.len(), 1);
    assert_eq!(lichess[0].fen_key, fen_key(E4));
    let stockfish: Vec<_> = puts
        .iter()
        .filter(|e| e.source == EvalSource::Stockfish)
        .collect();
    assert_eq!(stockfish.len(), 1);
    assert_eq!(stockfish[0].fen_key, fen_key(START_FEN));
}

#[tokio::test]
async fn makes_0_engine_calls_with_complete_lichess_evals_and_a_cached_start_position() {
    let g = short_game();
    let cache = MemoryEvalCache::default();
    cache
        .put_many(&[stockfish_entry(&fen_key(START_FEN), 30)])
        .await
        .unwrap();
    let an = FakeAnalyser::new();
    let l = [
        Some(EngineScore::Cp(1)),
        Some(EngineScore::Cp(2)),
        Some(EngineScore::Cp(3)),
    ];
    evaluate_game(&deps(&an, &cache), &g, Some(&l))
        .await
        .unwrap();
    assert_eq!(an.call_count(), 0);
}

#[tokio::test]
async fn analyses_null_lichess_entries_with_the_engine() {
    let g = short_game();
    let an = FakeAnalyser::new();
    let l = [Some(EngineScore::Cp(20)), None, Some(EngineScore::Cp(5))];
    let out = evaluate_game(&deps(&an, &MemoryEvalCache::default()), &g, Some(&l))
        .await
        .unwrap();
    assert_eq!(an.call_count(), 2); // start + ply 2
    assert_eq!(out.evals[1].score, EngineScore::Cp(20));
    assert_eq!(out.evals[2].score, EngineScore::Cp(30)); // white to move, engine score as is
}

#[tokio::test]
async fn analyses_duplicate_positions_once() {
    // 1. Nf3 Nf6 2. Ng1 Ng8
    let g = game(
        &[
            "rnbqkbnr/pppppppp/8/8/8/5N2/PPPPPPPP/RNBQKB1R b KQkq - 1 1",
            "rnbqkb1r/pppppppp/5n2/8/8/5N2/PPPPPPPP/RNBQKB1R w KQkq - 2 2",
            "rnbqkb1r/pppppppp/5n2/8/8/8/PPPPPPPP/RNBQKBNR b KQkq - 3 2",
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 4 3",
        ],
        None,
    );
    let an = FakeAnalyser::new();
    let out = evaluate_game(&deps(&an, &MemoryEvalCache::default()), &g, None)
        .await
        .unwrap();
    assert_eq!(out.evals.len(), 5);
    assert_eq!(an.call_count(), 4); // the final position repeats the start
    assert_eq!(out.evals[4], out.evals[0]);
}

#[tokio::test]
async fn flips_mate_scores_at_black_to_move_positions() {
    let g = game(&[E4], None);
    let an = FakeAnalyser::with_score(|_| EngineScore::Mate(2));
    let out = evaluate_game(&deps(&an, &MemoryEvalCache::default()), &g, None)
        .await
        .unwrap();
    assert_eq!(out.evals[0].score, EngineScore::Mate(2));
    assert_eq!(out.evals[1].score, EngineScore::Mate(-2));
}

#[tokio::test]
async fn keeps_completed_positions_when_the_engine_fails_midway_and_resumes_from_them() {
    let g = game(&[E4, E5, NF3, NC6, BB5], None); // 6 positions
    let cache = MemoryEvalCache::default();
    let mut failing = FakeAnalyser::new();
    failing.fail_after = Some(3);
    let mut d = deps(&failing, &cache);
    d.concurrency = 1;
    let err = evaluate_game(&d, &g, None).await.unwrap_err().error;
    assert!(matches!(err, EvaluateError::Engine(_)), "{err:?}");
    assert_eq!(
        cache
            .get_many(&keys(&g), ANALYSIS_PROTOCOL)
            .await
            .unwrap()
            .len(),
        3
    );
    let an = FakeAnalyser::new();
    let out = evaluate_game(&deps(&an, &cache), &g, None).await.unwrap();
    assert_eq!(out.evals.len(), 6);
    assert_eq!(an.call_count(), 3);
}

#[tokio::test]
async fn failure_stops_launching_new_positions_and_awaits_in_flight_calls() {
    let g = game(&[E4, E5, NF3, NC6, BB5], None);
    let mut failing = FakeAnalyser::new();
    failing.fail_after = Some(0);
    let cache = MemoryEvalCache::default();
    let mut d = deps(&failing, &cache);
    d.concurrency = 2;
    evaluate_game(&d, &g, None).await.unwrap_err();
    // At most the two in-flight calls were launched before the failure stopped the queue.
    assert!(failing.call_count() <= 2, "{}", failing.call_count());
}

#[tokio::test]
async fn reports_progress_from_inside_the_loop_after_each_persisted_engine_result() {
    let g = short_game(); // 4 positions
    let cache = Arc::new(MemoryEvalCache::default());
    let all = keys(&g);
    let progress: Arc<Mutex<Vec<(usize, usize, usize)>>> = Arc::new(Mutex::new(vec![]));
    let cb = {
        let cache = cache.clone();
        let progress = progress.clone();
        move |done: usize, total: usize| -> BoxFuture<'static, Result<(), String>> {
            let cache = cache.clone();
            let progress = progress.clone();
            let all = all.clone();
            Box::pin(async move {
                let cached = cache
                    .get_many(&all, ANALYSIS_PROTOCOL)
                    .await
                    .map_err(|e| e.to_string())?
                    .len();
                progress.lock().unwrap().push((done, total, cached));
                Ok(())
            })
        }
    };
    let an = FakeAnalyser::new();
    let mut d = deps(&an, &*cache);
    d.concurrency = 1;
    d.on_progress = Some(&cb);
    evaluate_game(&d, &g, None).await.unwrap();
    assert_eq!(
        *progress.lock().unwrap(),
        vec![(1, 4, 1), (2, 4, 2), (3, 4, 3), (4, 4, 4)]
    );
}

#[tokio::test]
async fn does_not_fail_the_evaluation_when_on_progress_throws() {
    let g = game(&[E4, E5], None);
    let cb = |_: usize, _: usize| -> BoxFuture<'static, Result<(), String>> {
        Box::pin(async { Err("heartbeat write failed".to_string()) })
    };
    let an = FakeAnalyser::new();
    let cache = MemoryEvalCache::default();
    let mut d = deps(&an, &cache);
    d.on_progress = Some(&cb);
    let out = evaluate_game(&d, &g, None).await.unwrap();
    assert_eq!(out.evals.len(), 3);
}

#[tokio::test]
async fn stats_count_engine_cache_and_lichess_positions() {
    let g = game(&[E4, E5, NF3, NC6], None); // 5 positions
    let cache = MemoryEvalCache::default();
    // One cache hit (ply 1), one lichess (ply 2), three engine positions.
    cache
        .put_many(&[stockfish_entry(&fen_key(E4), 5)])
        .await
        .unwrap();
    let l = [None, Some(EngineScore::Cp(9)), None, None];
    let an = FakeAnalyser::new();
    let out = evaluate_game(&deps(&an, &cache), &g, Some(&l))
        .await
        .unwrap();
    let s = out.stats;
    assert_eq!(s.positions, 5);
    assert_eq!(s.cache_hits, 1);
    assert_eq!(s.lichess_used, 1);
    assert_eq!(s.engine_positions, 3);
    assert_eq!(an.call_count(), 3);
    // Per-stage timings are populated: the fake reports 5 ms wall and 3 ms CPU per call.
    let wall = FAKE_WALL.as_secs_f64() * 1000.0 * 3.0;
    let cpu = FAKE_CPU.as_secs_f64() * 1000.0 * 3.0;
    assert!(
        (s.engine_wall_ms - wall).abs() < 1e-6,
        "{}",
        s.engine_wall_ms
    );
    assert!((s.engine_cpu_ms.unwrap() - cpu).abs() < 1e-6);
    assert!(s.cache_ms > 0.0, "cache_ms {}", s.cache_ms);
}

#[tokio::test]
async fn cancel_token_aborts_mid_search() {
    let g = short_game();
    let mut an = FakeAnalyser::new();
    an.hang = true;
    let cache = MemoryEvalCache::default();
    let token = CancellationToken::new();
    let mut d = deps(&an, &cache);
    d.cancel = Some(token.clone());
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        token.cancel();
    });
    let started = Instant::now();
    let err = evaluate_game(&d, &g, None).await.unwrap_err().error;
    assert!(
        matches!(err, EvaluateError::Deadline(JobDeadlineError { .. })),
        "{err:?}"
    );
    assert!(started.elapsed() < Duration::from_secs(2));
}

fn synthetic_game(n_plies: usize) -> ParsedGame {
    let fens: Vec<String> = (1..=n_plies)
        .map(|i| format!("pos{i} {} - -", if i % 2 == 1 { "b" } else { "w" }))
        .collect();
    let refs: Vec<&str> = fens.iter().map(String::as_str).collect();
    game(&refs, None)
}

#[tokio::test]
async fn elapsed_span_is_shorter_than_the_summed_engine_wall_time() {
    let g = synthetic_game(7); // 8 positions
    let mut an = FakeAnalyser::new();
    an.delay = Duration::from_millis(50);
    let cache = MemoryEvalCache::default();
    let out = evaluate_game(&deps(&an, &cache), &g, None).await.unwrap();
    let s = out.stats;
    assert_eq!(s.engine_positions, 8);
    // The sum is 8 x 50 ms; four workers overlap, so the span is about 100 ms.
    assert!(
        (s.engine_wall_ms - 400.0).abs() < 1e-6,
        "{}",
        s.engine_wall_ms
    );
    assert!(
        s.engine_stage_elapsed_ms >= 90.0 && s.engine_stage_elapsed_ms < 350.0,
        "{}",
        s.engine_stage_elapsed_ms
    );
    assert!(
        s.engine_wall_ms >= 4.0 * s.engine_stage_elapsed_ms - 150.0,
        "sum {} span {}",
        s.engine_wall_ms,
        s.engine_stage_elapsed_ms
    );
    assert!(s.total_elapsed_ms >= s.engine_stage_elapsed_ms);
}

#[tokio::test]
async fn failed_evaluation_still_reports_partial_stats() {
    let g = game(&[E4, E5, NF3, NC6, BB5], None); // 6 positions
    let cache = MemoryEvalCache::default();
    let mut failing = FakeAnalyser::new();
    failing.fail_after = Some(3);
    failing.delay = Duration::from_millis(5);
    let mut d = deps(&failing, &cache);
    d.concurrency = 1;
    let fail = evaluate_game(&d, &g, None).await.unwrap_err();
    assert!(matches!(fail.error, EvaluateError::Engine(_)));
    let s = fail.stats;
    assert_eq!(s.positions, 6);
    assert_eq!(s.engine_positions, 3);
    assert!((s.engine_wall_ms - 15.0).abs() < 1e-6);
    assert!(s.engine_stage_elapsed_ms > 0.0);
    assert!(s.total_elapsed_ms >= s.engine_stage_elapsed_ms);
}

#[tokio::test]
async fn pre_cancelled_token_makes_no_engine_calls_and_reports_its_deadline() {
    let g = short_game();
    let an = FakeAnalyser::new();
    let cache = MemoryEvalCache::default();
    let token = CancellationToken::new();
    token.cancel();
    let mut d = deps(&an, &cache);
    d.cancel = Some(token);
    d.deadline = Some(DeadlineInfo {
        kind: "import-account",
        ms: 1234,
    });
    let fail = evaluate_game(&d, &g, None).await.unwrap_err();
    match fail.error {
        EvaluateError::Deadline(e) => {
            assert_eq!(e.kind, "import-account");
            assert_eq!(e.ms, 1234);
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(an.call_count(), 0);
}

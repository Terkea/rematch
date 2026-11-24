//! R2 Task 25: the enrichment pass (TS M2 Task 22 `enrichGame`), with fakes for the engine,
//! Maia and the tablebase. Maia is the only difficulty source (owner override): when it is down
//! the difficulty is `None`/`pending_maia`, nothing is estimated and enrichment completes.

mod common;

use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chess_core::Color;
use chess_core::board::Position;
use common::*;
use engine::ENGINE_NODES;
use enrich::difficulty::{DifficultyOutcome, NotApplicableReason, PendingReason};
use enrich::enrich_game::{EnrichError, enrich_game};
use enrich::protocol::{MULTIPV, MULTIPV_NODES, THREAT_NODES};
use enrich::tablebase::pool::SyzygyPool;
use enrich::types::TablebaseProbe;
use profile_core::board::null_move_fen;
use testkit::fake_engine::FakeAnalyser;

const PV: &[&str] = &["a2a3", "a7a6"];
const WHITE: &[Color] = &[Color::White];
const BOTH: &[Color] = &[Color::White, Color::Black];

/// White's critical plies 13 (ΔW 6) and 15 (ΔW 12); 16 (Black) follows the big swing.
fn ruy(colors: &[Color]) -> Game {
    Game::new(game(RUY), PV, &[(13, 6.0), (15, 12.0)], colors)
}

fn fen_before(g: &Game, ply: u32) -> String {
    g.parsed.plies[ply as usize - 1].fen_before.clone()
}

fn fen_after(g: &Game, ply: u32) -> String {
    g.parsed.plies[ply as usize - 1].fen_after.clone()
}

fn calls_with(a: &FakeAnalyser, nodes: u64, multipv: u32) -> Vec<String> {
    a.calls
        .lock()
        .unwrap()
        .iter()
        .filter(|c| c.1 == nodes && c.2 == multipv)
        .map(|c| c.0.clone())
        .collect()
}

#[tokio::test]
async fn multipv_calls_are_exactly_the_critical_player_plies() {
    let g = ruy(WHITE);
    let store = memory_store();
    let a = Arc::new(FakeAnalyser::new());
    let d = deps(a.clone(), store.clone(), None, None);
    let out = enrich_game(&d, &g.input()).await.unwrap();

    assert_eq!(
        calls_with(&a, MULTIPV_NODES, MULTIPV),
        vec![fen_before(&g, 13), fen_before(&g, 15)]
    );
    // The MultiPV probe keeps its own 300,000 nodes: EXP-0008 measured the main search only.
    assert_eq!(MULTIPV_NODES, 300_000);
    assert_ne!(MULTIPV_NODES, ENGINE_NODES);
    // Only the threat probe besides.
    for c in a.calls.lock().unwrap().iter() {
        assert!(
            (c.1, c.2) == (MULTIPV_NODES, MULTIPV) || (c.1, c.2) == (THREAT_NODES, 1),
            "unexpected call {c:?}"
        );
    }
    let r13 = out.raw[12].as_ref().unwrap();
    let lines = r13.multipv.as_ref().expect("critical ply has multipv");
    assert_eq!(lines[0].uci, "e2e4");
    assert_eq!(lines[0].pv, vec!["e2e4", "e7e5"]);
    assert!(
        out.raw[10].as_ref().unwrap().multipv.is_none(),
        "ply 11 is not critical"
    );

    // A second run reads the cache: no MultiPV call at all.
    let a2 = Arc::new(FakeAnalyser::new());
    let d2 = deps(a2.clone(), store, None, None);
    let again = enrich_game(&d2, &g.input()).await.unwrap();
    assert!(calls_with(&a2, MULTIPV_NODES, MULTIPV).is_empty());
    assert_eq!(again.raw, out.raw);
}

#[tokio::test]
async fn raw_is_none_for_colours_not_requested() {
    let g = ruy(WHITE);
    let a = Arc::new(FakeAnalyser::new());
    let out = enrich_game(&deps(a, memory_store(), None, None), &g.input())
        .await
        .unwrap();
    assert_eq!(out.raw.len(), g.records.len());
    for (i, r) in out.raw.iter().enumerate() {
        let ply = i as u32 + 1;
        assert_eq!(r.is_some(), ply % 2 == 1, "ply {ply}");
        if let Some(r) = r {
            assert_eq!(r.ply, ply);
        }
    }
}

#[tokio::test]
async fn threat_probe_uses_the_flipped_side_at_threat_nodes() {
    let g = ruy(WHITE);
    let a = Arc::new(FakeAnalyser::new());
    let out = enrich_game(&deps(a.clone(), memory_store(), None, None), &g.input())
        .await
        .unwrap();
    let expected: Vec<String> = (1..=24)
        .filter(|p| p % 2 == 1)
        .map(|p| null_move_fen(&fen_before(&g, p)).unwrap())
        .collect();
    let threat_calls = calls_with(&a, THREAT_NODES, 1);
    assert_eq!(threat_calls, expected);
    assert_eq!(THREAT_NODES, 50_000);
    for fen in &threat_calls {
        assert_eq!(
            fen.split(' ').nth(1),
            Some("b"),
            "white ply probed with black to move"
        );
    }
    let t = out.raw[12].as_ref().unwrap().threat.as_ref().unwrap();
    assert_eq!(t.null_fen, null_move_fen(&fen_before(&g, 13)).unwrap());
    assert_eq!(t.pv, vec!["e2e4", "e7e5"]);
    assert_eq!(t.score, chess_core::EngineScore::Cp(30));
}

#[tokio::test]
async fn threat_probe_skipped_when_in_check() {
    let g = Game::new(game(CHECK), PV, &[], &[Color::Black]);
    assert!(Position::from_fen(&fen_before(&g, 4)).unwrap().is_check());
    let a = Arc::new(FakeAnalyser::new());
    let out = enrich_game(&deps(a.clone(), memory_store(), None, None), &g.input())
        .await
        .unwrap();
    assert_eq!(
        calls_with(&a, THREAT_NODES, 1),
        vec![null_move_fen(&fen_before(&g, 2)).unwrap()]
    );
    assert!(out.raw[3].as_ref().unwrap().threat.is_none());
    assert!(out.raw[1].as_ref().unwrap().threat.is_some());
}

#[tokio::test]
async fn threat_probe_skipped_for_decided_and_opponent_plies() {
    let mut g = ruy(WHITE);
    g.records[12].decided = true; // ply 13
    let a = Arc::new(FakeAnalyser::new());
    let out = enrich_game(&deps(a.clone(), memory_store(), None, None), &g.input())
        .await
        .unwrap();
    let probed: HashSet<String> = calls_with(&a, THREAT_NODES, 1).into_iter().collect();
    assert!(!probed.contains(&null_move_fen(&fen_before(&g, 13)).unwrap()));
    for p in (2..=24).step_by(2) {
        assert!(
            !probed.contains(&null_move_fen(&fen_before(&g, p)).unwrap()),
            "black ply {p}"
        );
    }
    assert_eq!(probed.len(), 11);
    assert!(out.raw[12].as_ref().unwrap().threat.is_none());
}

#[tokio::test]
async fn refutation_pv_comes_from_the_next_eval() {
    let g = ruy(WHITE);
    let a = Arc::new(FakeAnalyser::new());
    let out = enrich_game(&deps(a.clone(), memory_store(), None, None), &g.input())
        .await
        .unwrap();
    let r = out.raw[12].as_ref().unwrap();
    assert_eq!(r.refutation_pv, vec!["a2a3", "a7a6"]);
    assert_eq!(r.best_pv, vec!["a2a3", "a7a6"]);
    assert!(!a.called_fens().contains(&fen_after(&g, 13)));
}

#[tokio::test]
async fn refutation_pv_falls_back_to_engine_when_cached_pv_is_empty() {
    // Lichess-style evals: `pv: []`.
    let g = Game::new(game(RUY), &[], &[(13, 6.0), (15, 12.0)], WHITE);
    let a = Arc::new(FakeAnalyser::new());
    let out = enrich_game(&deps(a.clone(), memory_store(), None, None), &g.input())
        .await
        .unwrap();
    let mpv = calls_with(&a, MULTIPV_NODES, MULTIPV);
    assert!(
        mpv.contains(&fen_after(&g, 13)),
        "fen_after of critical ply 13 probed"
    );
    assert!(mpv.contains(&fen_after(&g, 15)));
    assert!(
        !mpv.contains(&fen_after(&g, 11)),
        "non-critical plies get no probe"
    );
    let r = out.raw[12].as_ref().unwrap();
    assert_eq!(r.refutation_pv, vec!["e2e4", "e7e5"]);
    // No main-eval best move: no best line is borrowed from MultiPV (reviewer M3).
    assert!(r.best_pv.is_empty());
    assert!(out.raw[10].as_ref().unwrap().refutation_pv.is_empty());
}

#[tokio::test]
async fn tablebase_probed_only_for_five_pieces_and_after_is_negated() {
    let g = Game::new(game_from(ENDGAME_FEN, ENDGAME), PV, &[], BOTH);
    let tb = FakeTb::new(1);
    let a = Arc::new(FakeAnalyser::new());
    let out = enrich_game(
        &deps(
            a,
            memory_store(),
            Some(tb.clone() as Arc<dyn TablebaseProbe>),
            None,
        ),
        &g.input(),
    )
    .await
    .unwrap();
    let probed: HashSet<String> = tb.probed().into_iter().collect();
    // Ply 1 starts with six pieces: only its fen_after is probed.
    assert!(!probed.contains(&fen_before(&g, 1)));
    assert!(probed.contains(&fen_after(&g, 1)));
    assert!(probed.contains(&fen_after(&g, 3)));
    let r1 = out.raw[0].as_ref().unwrap();
    assert_eq!(r1.tb_wdl_before, None);
    assert_eq!(r1.tb_wdl_after, Some(-1), "after is from the mover's view");
    let r2 = out.raw[1].as_ref().unwrap();
    assert_eq!((r2.tb_wdl_before, r2.tb_wdl_after), (Some(1), Some(-1)));
    assert!(out.stages.contains_key("tb_ms"));
}

#[tokio::test]
async fn missing_tables_dir_is_unavailable_never_a_failure() {
    let dir = std::env::temp_dir().join(format!("no-syzygy-{}", uuid::Uuid::new_v4()));
    let pool = SyzygyPool::start(
        std::path::Path::new("/nonexistent/stockfish"),
        &dir,
        1,
        1000,
    )
    .await
    .expect("no tables starts no engine");
    assert_eq!(pool.status().as_str(), "unavailable");
    let g = Game::new(game_from(ENDGAME_FEN, ENDGAME), PV, &[], BOTH);
    let a = Arc::new(FakeAnalyser::new());
    let out = enrich_game(
        &deps(
            a,
            memory_store(),
            Some(Arc::new(pool) as Arc<dyn TablebaseProbe>),
            None,
        ),
        &g.input(),
    )
    .await
    .expect("enrichment completes");
    for r in out.raw.iter().flatten() {
        assert_eq!((r.tb_wdl_before, r.tb_wdl_after), (None, None));
    }
}

#[tokio::test]
async fn maia_is_the_only_difficulty_and_skips_opening_plies() {
    let g = ruy(WHITE);
    let maia = FakeMaia::new(true, MaiaMode::Ok);
    let a = Arc::new(FakeAnalyser::new());
    let out = enrich_game(
        &deps(a, memory_store(), None, Some(maia.clone())),
        &g.input(),
    )
    .await
    .unwrap();
    assert_eq!(out.difficulty_model.as_deref(), Some(MODEL));
    // Plies <= 10 are never sent, and are not_applicable/opening_ply.
    let sent = maia.sent_fens();
    let expected: Vec<String> = [11, 13, 15, 17, 19, 21, 23]
        .iter()
        .map(|&p| fen_before(&g, p))
        .collect();
    assert_eq!(sent, expected);
    for ply in [1u32, 3, 5, 7, 9] {
        let i = ply as usize - 1;
        assert_eq!(out.raw[i].as_ref().unwrap().difficulty, None);
        assert_eq!(
            out.difficulty[i],
            Some(DifficultyOutcome::NotApplicable {
                reason: NotApplicableReason::OpeningPly
            }),
            "ply {ply}"
        );
    }
    let r13 = out.raw[12].as_ref().unwrap();
    assert_eq!(r13.difficulty, Some(FakeMaia::difficulty()));
    assert!(out.difficulty[1].is_none(), "black plies are not requested");

    // Inputs: the mover's Elo first; MultiPV lines plus the played move with its win%.
    let input13 = maia
        .sent()
        .into_iter()
        .find(|i| i.fen == fen_before(&g, 13))
        .unwrap();
    assert_eq!(
        (input13.elo_self, input13.elo_oppo),
        (Some(1500), Some(1600))
    );
    let lines = input13.lines.unwrap();
    assert!(lines.iter().any(|(u, _)| u == "e2e4"), "multipv line");
    let played = &g.records[12];
    assert!(
        lines
            .iter()
            .any(|(u, w)| *u == played.uci && (*w - played.win_after).abs() < 1e-9),
        "played move with win_after: {lines:?}"
    );
    // The best move is the main (MultiPV 1) eval's; MultiPV never replaces it.
    assert_eq!(input13.best_uci.as_deref(), Some("a2a3"));
    assert!(
        lines.iter().any(|(u, _)| u == "a2a3"),
        "best move is a line"
    );
    // A non-critical ply: the eval's best move with win_before, plus the played move.
    let input11 = maia
        .sent()
        .into_iter()
        .find(|i| i.fen == fen_before(&g, 11))
        .unwrap();
    assert_eq!(input11.best_uci.as_deref(), Some("a2a3"));
    let lines11 = input11.lines.unwrap();
    assert!(
        lines11
            .iter()
            .any(|(u, w)| u == "a2a3" && (*w - g.records[10].win_before).abs() < 1e-9)
    );
}

#[tokio::test]
async fn difficulty_provider_failure_yields_null_difficulty() {
    // Maia down: no estimate of any kind, pending_maia, and the rest of enrichment completes.
    let g = ruy(BOTH);
    let maia = FakeMaia::new(false, MaiaMode::Ok);
    let a = Arc::new(FakeAnalyser::new());
    let out = enrich_game(&deps(a, memory_store(), None, Some(maia)), &g.input())
        .await
        .expect("analysis is never blocked on Maia");
    assert_eq!(out.difficulty_model, None);
    for (i, r) in out.raw.iter().enumerate() {
        let r = r.as_ref().unwrap();
        assert_eq!(r.difficulty, None, "ply {}", i + 1);
        let expected = if r.ply <= 10 {
            DifficultyOutcome::NotApplicable {
                reason: NotApplicableReason::OpeningPly,
            }
        } else {
            DifficultyOutcome::PendingMaia {
                reason: PendingReason::MaiaUnavailable,
            }
        };
        assert_eq!(out.difficulty[i].as_ref(), Some(&expected), "ply {}", r.ply);
    }
    assert!(out.raw[12].as_ref().unwrap().multipv.is_some());
    assert!(out.raw[12].as_ref().unwrap().threat.is_some());
}

#[tokio::test]
async fn no_difficulty_source_is_pending_not_configured() {
    let g = ruy(WHITE);
    let a = Arc::new(FakeAnalyser::new());
    let out = enrich_game(&deps(a, memory_store(), None, None), &g.input())
        .await
        .unwrap();
    assert_eq!(out.difficulty_model, None);
    assert_eq!(
        out.difficulty[12],
        Some(DifficultyOutcome::PendingMaia {
            reason: PendingReason::MaiaNotConfigured
        })
    );
}

#[tokio::test]
async fn decided_plies_get_no_difficulty() {
    let mut g = ruy(WHITE);
    g.records[12].decided = true;
    let maia = FakeMaia::new(true, MaiaMode::Ok);
    let a = Arc::new(FakeAnalyser::new());
    let out = enrich_game(
        &deps(a, memory_store(), None, Some(maia.clone())),
        &g.input(),
    )
    .await
    .unwrap();
    assert!(!maia.sent_fens().contains(&fen_before(&g, 13)));
    assert_eq!(out.difficulty[12], None);
}

#[tokio::test]
async fn invalid_fens_never_reach_the_analyser() {
    let mut g = ruy(WHITE);
    g.parsed.plies[12].fen_before = "not a fen".into();
    g.records[12].fen_before = "not a fen".into();
    g.parsed.plies[14].fen_after = "8/8/8/8/8/8/8/8 w - - 0 1".into();
    let a = Arc::new(FakeAnalyser::new());
    enrich_game(&deps(a.clone(), memory_store(), None, None), &g.input())
        .await
        .expect("invalid positions are skipped");
    for fen in a.called_fens() {
        assert!(Position::from_fen(&fen).is_ok(), "invalid fen sent: {fen}");
    }
}

#[tokio::test]
async fn engine_error_fails_the_pass() {
    let g = ruy(WHITE);
    let mut fake = FakeAnalyser::new();
    fake.fail_after = Some(0);
    let err = enrich_game(
        &deps(Arc::new(fake), memory_store(), None, None),
        &g.input(),
    )
    .await
    .unwrap_err();
    assert!(matches!(err, EnrichError::Engine(_)), "{err:?}");
}

#[tokio::test]
async fn cancel_returns_cancelled_within_2s() {
    let g = ruy(WHITE);
    let mut fake = FakeAnalyser::new();
    fake.hang = true;
    let d = deps(Arc::new(fake), memory_store(), None, None);
    let cancel = d.cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        cancel.cancel();
    });
    let t0 = Instant::now();
    let err = enrich_game(&d, &g.input()).await.unwrap_err();
    assert!(matches!(err, EnrichError::Cancelled), "{err:?}");
    assert!(t0.elapsed() < Duration::from_secs(2), "{:?}", t0.elapsed());
}

#[tokio::test]
async fn stages_are_reported() {
    let g = ruy(WHITE);
    let a = Arc::new(FakeAnalyser::new());
    let out = enrich_game(&deps(a, memory_store(), None, None), &g.input())
        .await
        .unwrap();
    let keys: Vec<&str> = out.stages.keys().map(String::as_str).collect();
    assert_eq!(
        keys,
        vec![
            "difficulty_ms",
            "multipv_engine_calls",
            "multipv_ms",
            "multipv_positions",
            "tb_ms",
            "tb_probes",
            "threat_engine_calls",
            "threat_ms",
            "threat_positions",
        ]
    );
    assert!(out.stages.values().all(|v| v.is_finite() && *v >= 0.0));
    // Every probed position was an engine call (an empty store), and no table was probed.
    assert_eq!(
        out.stages["multipv_positions"],
        out.stages["multipv_engine_calls"]
    );
    assert_eq!(
        out.stages["threat_positions"],
        out.stages["threat_engine_calls"]
    );
    assert!(out.stages["threat_positions"] > 0.0);
    assert_eq!(out.stages["tb_probes"], 0.0);
}

/// Real Stockfish and the real Syzygy tables (Docker): a critical endgame ply gets three MultiPV
/// lines, and the 5-piece position after 1. bxc3 (KRPvKP) is a win for the mover.
#[tokio::test]
async fn real_stockfish_and_tables_enrich_an_endgame() {
    let Some(sf) = testkit::stockfish() else {
        eprintln!("skipped: STOCKFISH_PATH unset");
        return;
    };
    let Some(tables) = std::env::var_os("SYZYGY_PATH").map(std::path::PathBuf::from) else {
        assert!(
            !testkit::in_docker(),
            "SYZYGY_PATH must be set when IN_DOCKER=1"
        );
        eprintln!("skipped: SYZYGY_PATH unset");
        return;
    };
    let pool = engine::pool::EnginePool::create(engine::pool::PoolOptions {
        size: 1,
        path: sf.clone(),
        hash_mb: None,
    })
    .await
    .unwrap();
    let tb = SyzygyPool::start(&sf, &tables, 1, 1000).await.unwrap();
    assert!(
        tb.tables_found() > 0 || !testkit::in_docker(),
        "no tables at {}",
        tables.display()
    );
    let g = Game::new(
        game_from(ENDGAME_FEN, ENDGAME),
        PV,
        &[(1, 8.0)],
        &[Color::White],
    );
    let out = enrich_game(
        &deps(
            pool.clone(),
            memory_store(),
            Some(Arc::new(tb) as Arc<dyn TablebaseProbe>),
            None,
        ),
        &g.input(),
    )
    .await
    .unwrap();
    pool.close().await;
    let r1 = out.raw[0].as_ref().unwrap();
    let lines = r1.multipv.as_ref().expect("critical ply");
    assert_eq!(lines.len(), 3, "{lines:?}");
    for l in lines {
        assert_eq!(l.pv.first(), Some(&l.uci));
        Position::from_fen(&fen_before(&g, 1))
            .unwrap()
            .play_uci(&l.uci)
            .expect("legal multipv move");
    }
    assert_eq!(r1.tb_wdl_before, None, "six pieces");
    assert_eq!(r1.tb_wdl_after, Some(1), "KRPvKP after bxc3 wins for White");
    assert!(r1.threat.is_some());
}

/// Reviewer M3: the `best_pv` fallback must start with the main eval's `best_uci`, else empty.
#[tokio::test]
async fn best_pv_fallback_starts_with_best_uci() {
    for (best, expected) in [("e2e4", vec!["e2e4", "e7e5"]), ("d2d4", vec![])] {
        let parsed = game(RUY);
        let mut evals = common::evals(&parsed, &[]);
        for e in &mut evals {
            e.best_uci = Some(best.to_owned());
        }
        let records = common::records(&parsed, &evals, &[(13, 6.0)]);
        let g = Game {
            parsed,
            evals,
            records,
            colors: WHITE.to_vec(),
        };
        let a = Arc::new(FakeAnalyser::new());
        let out = enrich_game(&deps(a, memory_store(), None, None), &g.input())
            .await
            .unwrap();
        assert_eq!(
            out.raw[12].as_ref().unwrap().best_pv,
            expected,
            "best {best}"
        );
    }
}

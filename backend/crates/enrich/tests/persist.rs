//! R2 Task 25: the database store and `persist_move_features` (TS M2 Task 22 DB cases).

mod common;

use std::sync::Arc;

use chess_core::{Color, EngineScore};
use common::*;
use engine::ENGINE_NODES;
use enrich::difficulty::{DifficultyOutcome, PendingReason};
use enrich::enrich_game::enrich_game;
use enrich::persist::persist_move_features;
use enrich::protocol::MULTIPV;
use enrich::store::{DbEnrichmentStore, EnrichmentStore};
use profile_core::types::{MoveFeatures, MultipvLine, RawMoveFeatures};
use sqlx::PgPool;
use testkit::fake_engine::FakeAnalyser;

const PV: &[&str] = &["a2a3", "a7a6"];

fn ruy(colors: &[Color]) -> Game {
    Game::new(game(RUY), PV, &[(13, 6.0), (15, 12.0)], colors)
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn db_store_round_trips(pool: PgPool) {
    let store = DbEnrichmentStore::new(pool.clone());
    let lines = vec![MultipvLine {
        uci: "e2e4".into(),
        score: EngineScore::Mate(3),
        pv: vec!["e2e4".into(), "e7e5".into()],
    }];
    store
        .put_multipv(&[("k1".into(), lines.clone())])
        .await
        .unwrap();
    // Writing again is a no-op, not an error.
    store
        .put_multipv(&[("k1".into(), lines.clone())])
        .await
        .unwrap();
    let got = store
        .get_multipv(&["k1".into(), "missing".into()])
        .await
        .unwrap();
    assert_eq!(got.len(), 1);
    assert_eq!(got["k1"], lines);

    let threat = (EngineScore::Cp(-120), vec!["d8h4".to_owned()]);
    store
        .put_threat(&[
            ("t1".into(), threat.clone()),
            ("t2".into(), (EngineScore::Mate(-2), vec![])),
        ])
        .await
        .unwrap();
    let got = store.get_threat(&["t1".into(), "t2".into()]).await.unwrap();
    assert_eq!(got["t1"], threat);
    assert_eq!(got["t2"], (EngineScore::Mate(-2), vec![]));
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM position_multipv WHERE protocol = 'sf19-n300k-mpv3-v1'"
        )
        .await,
        1
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM position_threats WHERE protocol = 'sf19-n50k-null-v1'"
        )
        .await,
        2
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn enrichment_never_touches_position_evals(pool: PgPool) {
    sqlx::query(
        "INSERT INTO position_evals (fen_key, protocol, score_cp, best_uci, pv, source) \
         VALUES ('k', 'sf19-n300k-v1', 10, 'e2e4', '{e2e4}', 'stockfish')",
    )
    .execute(&pool)
    .await
    .unwrap();
    let before = count(&pool, "SELECT count(*) FROM position_evals").await;
    let g = ruy(&[Color::White]);
    let store: Arc<dyn EnrichmentStore> = Arc::new(DbEnrichmentStore::new(pool.clone()));
    let a = Arc::new(FakeAnalyser::new());
    enrich_game(&deps(a, store.clone(), None, None), &g.input())
        .await
        .unwrap();
    assert_eq!(
        count(&pool, "SELECT count(*) FROM position_evals").await,
        before
    );
    assert_eq!(
        count(&pool, "SELECT count(*) FROM position_multipv").await,
        2
    );
    assert_eq!(
        count(&pool, "SELECT count(*) FROM position_threats").await,
        12
    );

    // The DB cache serves a second run: no MultiPV and no threat call.
    let a2 = Arc::new(FakeAnalyser::new());
    enrich_game(&deps(a2.clone(), store, None, None), &g.input())
        .await
        .unwrap();
    assert_eq!(a2.call_count(), 0, "{:?}", a2.called_fens());
    assert_eq!(
        count(&pool, "SELECT count(*) FROM position_evals").await,
        before
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn persist_replaces_rows(pool: PgPool) {
    let game_id = seed_game(&pool).await;
    let g = ruy(&[Color::White]);
    let maia = FakeMaia::new(false, MaiaMode::Ok);
    let a = Arc::new(FakeAnalyser::new());
    let out = enrich_game(&deps(a, memory_store(), None, Some(maia)), &g.input())
        .await
        .unwrap();
    for _ in 0..2 {
        let mut conn = pool.acquire().await.unwrap();
        persist_move_features(
            &mut conn,
            game_id,
            &g.records,
            &out.raw,
            None,
            &out.difficulty,
        )
        .await
        .unwrap();
    }
    // One row per requested (White) ply, the same after a re-run.
    assert_eq!(count(&pool, "SELECT count(*) FROM move_features").await, 12);
    // ply, status, reason, attempts, p_best, model, enrichment_protocol.
    type StatusRow = (
        i32,
        Option<String>,
        Option<String>,
        i16,
        Option<f32>,
        Option<String>,
        String,
    );
    let rows: Vec<StatusRow> = sqlx::query_as(
        "SELECT ply, difficulty_status, difficulty_reason, difficulty_attempts, p_best, \
               difficulty_model, enrichment_protocol FROM move_features ORDER BY ply",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    let r1 = &rows[0];
    assert_eq!(r1.0, 1);
    assert_eq!(r1.1.as_deref(), Some("not_applicable"));
    assert_eq!(r1.2.as_deref(), Some("opening_ply"));
    let r13 = rows.iter().find(|r| r.0 == 13).unwrap();
    assert_eq!(r13.1.as_deref(), Some("pending_maia"));
    assert_eq!(r13.2.as_deref(), Some("maia_unavailable"));
    assert_eq!((r13.3, r13.4, r13.5.as_deref()), (0, None, None));
    assert!(rows.iter().all(|r| r.6 == "enr-v1"));
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn persist_writes_raw_and_derived_values(pool: PgPool) {
    let game_id = seed_game(&pool).await;
    let g = ruy(&[Color::White]);
    let maia = FakeMaia::new(true, MaiaMode::Ok);
    let a = Arc::new(FakeAnalyser::new());
    let out = enrich_game(&deps(a, memory_store(), None, Some(maia)), &g.input())
        .await
        .unwrap();
    let mut derived = vec![MoveFeatures::default(); g.records.len()];
    derived[12].is_critical = Some(true);
    derived[12].multipv_gap = Some(17.5);
    derived[12].threat_existed = Some(false);
    let mut conn = pool.acquire().await.unwrap();
    persist_move_features(
        &mut conn,
        game_id,
        &g.records,
        &out.raw,
        Some(&derived),
        &out.difficulty,
    )
    .await
    .unwrap();
    type Row = (
        Option<serde_json::Value>,
        Vec<String>,
        Vec<String>,
        Option<serde_json::Value>,
        Option<f32>,
        Option<f32>,
        Option<String>,
        Option<bool>,
        Option<f32>,
        Option<bool>,
        Option<String>,
    );
    let r: Row = sqlx::query_as(
        "SELECT multipv, best_pv, refutation_pv, threat, p_best, p_ok, difficulty_model, \
           is_critical, multipv_gap, threat_existed, difficulty_status \
         FROM move_features WHERE ply = 13",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let raw13 = out.raw[12].as_ref().unwrap();
    let multipv: Vec<MultipvLine> = serde_json::from_value(r.0.unwrap()).unwrap();
    assert_eq!(Some(multipv), raw13.multipv);
    assert_eq!(r.1, raw13.best_pv);
    assert_eq!(r.2, raw13.refutation_pv);
    assert_eq!(
        r.3.unwrap()["nullFen"],
        raw13.threat.as_ref().unwrap().null_fen
    );
    assert_eq!((r.4, r.5), (Some(0.25), Some(0.75)));
    assert_eq!(r.6.as_deref(), Some(MODEL));
    assert_eq!((r.7, r.8, r.9), (Some(true), Some(17.5), Some(false)));
    assert_eq!(r.10.as_deref(), Some("ok"));
    // A non-critical ply has no multipv and no derived values.
    let (mpv, crit): (Option<serde_json::Value>, Option<bool>) =
        sqlx::query_as("SELECT multipv, is_critical FROM move_features WHERE ply = 11")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!((mpv, crit), (None, None));
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn persist_writes_in_chunks(pool: PgPool) {
    let game_id = seed_game(&pool).await;
    let g = ruy(&[Color::White]);
    // 1100 plies: three UNNEST chunks of at most 500.
    let template = g.records[12].clone();
    let records: Vec<_> = (1..=1100u32)
        .map(|ply| {
            let mut r = template.clone();
            r.ply = ply;
            r
        })
        .collect();
    let raw: Vec<Option<RawMoveFeatures>> = records
        .iter()
        .map(|r| {
            Some(RawMoveFeatures {
                ply: r.ply,
                multipv: None,
                best_pv: vec![],
                refutation_pv: vec!["e2e4".into()],
                threat: None,
                tb_wdl_before: Some(1),
                tb_wdl_after: None,
                difficulty: None,
            })
        })
        .collect();
    let difficulty = vec![
        Some(DifficultyOutcome::PendingMaia {
            reason: PendingReason::InvalidResponse
        });
        records.len()
    ];
    let mut conn = pool.acquire().await.unwrap();
    persist_move_features(&mut conn, game_id, &records, &raw, None, &difficulty)
        .await
        .unwrap();
    assert_eq!(
        count(&pool, "SELECT count(*) FROM move_features").await,
        1100
    );
    // An invalid response is the first attempt (the retry cap counts it).
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM move_features WHERE difficulty_attempts = 1 AND tb_wdl_before = 1"
        )
        .await,
        1100
    );
    assert_eq!(ENGINE_NODES, 300_000);
    assert_eq!(MULTIPV, 3);
}

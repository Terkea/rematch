use analysis::eval_cache::{
    CACHE_CHUNK, DbEvalCache, EvalCache, EvalCacheEntry, EvalSource, MemoryEvalCache,
};
use chess_core::{EngineScore, PositionEval};
use uuid::Uuid;

fn ev(score: EngineScore, best: Option<&str>, pv: &[&str]) -> PositionEval {
    PositionEval {
        score,
        best_uci: best.map(str::to_string),
        pv: pv.iter().map(|s| s.to_string()).collect(),
    }
}

async fn round_trips_cp_and_mate_scores_for_the_right_protocol_only(cache: &dyn EvalCache) {
    let p = format!("test-{}", Uuid::new_v4());
    let a = format!("fen-a-{} w - -", Uuid::new_v4());
    let b = format!("fen-b-{} b - -", Uuid::new_v4());
    let ea = ev(EngineScore::Cp(-42), Some("e2e4"), &["e2e4"]);
    let eb = ev(EngineScore::Mate(3), None, &[]);
    cache
        .put_many(&[
            EvalCacheEntry {
                fen_key: a.clone(),
                protocol: p.clone(),
                eval: ea.clone(),
                source: EvalSource::Stockfish,
            },
            EvalCacheEntry {
                fen_key: b.clone(),
                protocol: p.clone(),
                eval: eb.clone(),
                source: EvalSource::Lichess,
            },
        ])
        .await
        .unwrap();
    let keys = vec![a.clone(), b.clone(), "missing".to_string()];
    let got = cache.get_many(&keys, &p).await.unwrap();
    assert_eq!(got.get(&a), Some(&ea));
    assert_eq!(got.get(&b), Some(&eb));
    assert_eq!(got.len(), 2);
    let other = format!("other-{p}");
    assert!(cache.get_many(&[a], &other).await.unwrap().is_empty());
}

async fn keeps_the_first_row_on_conflict_and_handles_empty_input(cache: &dyn EvalCache) {
    let p = format!("test-{}", Uuid::new_v4());
    let k = format!("fen-{} w - -", Uuid::new_v4());
    cache.put_many(&[]).await.unwrap();
    assert!(cache.get_many(&[], &p).await.unwrap().is_empty());
    let entry = |cp| EvalCacheEntry {
        fen_key: k.clone(),
        protocol: p.clone(),
        eval: ev(EngineScore::Cp(cp), None, &[]),
        source: EvalSource::Stockfish,
    };
    cache.put_many(&[entry(1)]).await.unwrap();
    cache.put_many(&[entry(2)]).await.unwrap();
    let got = cache.get_many(std::slice::from_ref(&k), &p).await.unwrap();
    assert_eq!(got.get(&k).unwrap().score, EngineScore::Cp(1));
}

async fn chunks_large_batches_and_dedupes_keys(cache: &dyn EvalCache) {
    let p = format!("test-{}", Uuid::new_v4());
    let n = CACHE_CHUNK * 2 + 5;
    let entries: Vec<EvalCacheEntry> = (0..n)
        .map(|i| EvalCacheEntry {
            fen_key: format!("k{i} w - -"),
            protocol: p.clone(),
            eval: ev(EngineScore::Cp(i as i32), None, &[]),
            source: EvalSource::Stockfish,
        })
        .collect();
    cache.put_many(&entries).await.unwrap();
    let mut keys: Vec<String> = entries.iter().map(|e| e.fen_key.clone()).collect();
    keys.extend(keys.clone());
    let got = cache.get_many(&keys, &p).await.unwrap();
    assert_eq!(got.len(), n);
    assert_eq!(got["k1234 w - -"].score, EngineScore::Cp(1234));
}

#[tokio::test]
async fn memory_round_trips_cp_and_mate_scores_for_the_right_protocol_only() {
    round_trips_cp_and_mate_scores_for_the_right_protocol_only(&MemoryEvalCache::default()).await;
}
#[tokio::test]
async fn memory_keeps_the_first_row_on_conflict_and_handles_empty_input() {
    keeps_the_first_row_on_conflict_and_handles_empty_input(&MemoryEvalCache::default()).await;
}
#[tokio::test]
async fn memory_chunks_large_batches_and_dedupes_keys() {
    chunks_large_batches_and_dedupes_keys(&MemoryEvalCache::default()).await;
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn db_round_trips_cp_and_mate_scores_for_the_right_protocol_only(pool: sqlx::PgPool) {
    round_trips_cp_and_mate_scores_for_the_right_protocol_only(&DbEvalCache::new(pool)).await;
}
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn db_keeps_the_first_row_on_conflict_and_handles_empty_input(pool: sqlx::PgPool) {
    keeps_the_first_row_on_conflict_and_handles_empty_input(&DbEvalCache::new(pool)).await;
}
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn db_chunks_large_batches_and_dedupes_keys(pool: sqlx::PgPool) {
    chunks_large_batches_and_dedupes_keys(&DbEvalCache::new(pool)).await;
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn db_ignores_rows_with_no_score_and_mate_wins_over_cp(pool: sqlx::PgPool) {
    sqlx::query(
        "INSERT INTO position_evals (fen_key, protocol, score_cp, score_mate, best_uci, pv, source) VALUES \
         ('none', 'p', NULL, NULL, NULL, '{}', 'stockfish'), \
         ('both', 'p', 50, 2, 'a1a2', '{a1a2,b1b2}', 'stockfish')",
    )
    .execute(&pool)
    .await
    .unwrap();
    let cache = DbEvalCache::new(pool);
    let keys = vec!["none".to_string(), "both".to_string()];
    let got = cache.get_many(&keys, "p").await.unwrap();
    assert_eq!(got.len(), 1);
    assert_eq!(
        got["both"],
        ev(EngineScore::Mate(2), Some("a1a2"), &["a1a2", "b1b2"])
    );
}

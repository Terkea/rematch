//! R2 Task 17: opening explorer storage (`refdata::explorer`). TS M2 Task 11's explorer cases
//! applied at the storage level (three B2 blitz games `1.e4 e5` 1-0, `1.e4 c5` 0-1, `1.d4 d5`
//! ½ with `maxPly 2`), plus the R2 cases and a plan guard on both hot reads.

use std::collections::{BTreeMap, HashMap};

use chess_core::TimeClass;
use chess_core::board::Position;
use chess_core::fen::fen_key;
use profile_core::types::Band;
use refdata::builds::{
    BuildKind, NewBuild, activate_build, active_build, create_build, set_source_sha,
};
use refdata::explorer::{
    EXPLORER_INDEX_SQL, ExplorerCounts, ExplorerKey, ExplorerMove, QUERY_EXPLORER_SQL,
    flush_explorer, in_theory, load_explorer_index, prune_explorer, query_explorer,
};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

fn start_key() -> String {
    fen_key(&Position::start().fen())
}

fn key_after(sans: &[&str]) -> String {
    let mut p = Position::start();
    for s in sans {
        p.play_san(s).unwrap();
    }
    fen_key(&p.fen())
}

/// Moves of one game and its (white, draws, black) result.
type Game = (&'static [&'static str], (i64, i64, i64));

/// TS M2 Task 11's aggregation of the three games with `maxPly 2`, as the build (Task 20) hands
/// it to `flush_explorer`.
fn three_games() -> Vec<(ExplorerKey, ExplorerCounts)> {
    let games: [Game; 3] = [
        (&["e4", "e5"], (1, 0, 0)),
        (&["e4", "c5"], (0, 0, 1)),
        (&["d4", "d5"], (0, 1, 0)),
    ];
    let mut acc: BTreeMap<(String, String), (i64, i64, i64)> = BTreeMap::new();
    for (sans, (w, d, b)) in games {
        let mut p = Position::start();
        for s in sans {
            let before = fen_key(&p.fen());
            let played = p.play_san(s).unwrap();
            let e = acc.entry((before, played.uci)).or_default();
            e.0 += w;
            e.1 += d;
            e.2 += b;
        }
    }
    acc.into_iter()
        .map(|((fen_key, uci), (white, draws, black))| {
            (
                ExplorerKey {
                    fen_key,
                    band: Band::B2,
                    time_class: TimeClass::Blitz,
                    uci,
                },
                ExplorerCounts {
                    white,
                    draws,
                    black,
                },
            )
        })
        .collect()
}

async fn building(pool: &PgPool, month: &str) -> Uuid {
    create_build(
        pool,
        &NewBuild {
            kind: BuildKind::OpeningExplorer,
            source_month: month.into(),
            source_url: None,
            params: json!({ "max_ply": 2, "min_row_count": 1 }),
            protocol: None,
            enrichment_protocol: None,
            difficulty_model: None,
        },
    )
    .await
    .unwrap()
}

async fn activate(pool: &PgPool, id: Uuid) {
    set_source_sha(pool, id, &"ab".repeat(32), 3).await.unwrap();
    activate_build(pool, id).await.unwrap();
}

fn mv(uci: &str, white: i64, draws: i64, black: i64) -> ExplorerMove {
    ExplorerMove {
        uci: uci.into(),
        white,
        draws,
        black,
        total: white + draws + black,
    }
}

async fn row_count(pool: &PgPool, build: Uuid) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM opening_explorer WHERE build_id = $1")
        .bind(build)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn one_flush_orders_start_moves_by_total(pool: PgPool) {
    let id = building(&pool, "2026-10").await;
    flush_explorer(&pool, id, &three_games()).await.unwrap();
    activate(&pool, id).await;
    let moves = query_explorer(&pool, &start_key(), Band::B2, TimeClass::Blitz)
        .await
        .unwrap();
    assert_eq!(moves, vec![mv("e2e4", 1, 0, 1), mv("d2d4", 0, 1, 0)]);
    let after_e4 = query_explorer(&pool, &key_after(&["e4"]), Band::B2, TimeClass::Blitz)
        .await
        .unwrap();
    // Equal totals tie-break by uci.
    assert_eq!(after_e4, vec![mv("c7c5", 0, 0, 1), mv("e7e5", 1, 0, 0)]);
    // Another band or time class sees nothing.
    assert!(
        query_explorer(&pool, &start_key(), Band::B3, TimeClass::Blitz)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        query_explorer(&pool, &start_key(), Band::B2, TimeClass::Rapid)
            .await
            .unwrap()
            .is_empty()
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn flushing_twice_adds_counts(pool: PgPool) {
    let id = building(&pool, "2026-10").await;
    let rows = three_games();
    flush_explorer(&pool, id, &rows).await.unwrap();
    flush_explorer(&pool, id, &rows).await.unwrap();
    assert_eq!(row_count(&pool, id).await, rows.len() as i64);
    activate(&pool, id).await;
    let moves = query_explorer(&pool, &start_key(), Band::B2, TimeClass::Blitz)
        .await
        .unwrap();
    assert_eq!(moves, vec![mv("e2e4", 2, 0, 2), mv("d2d4", 0, 2, 0)]);
}

/// Repeated keys inside one flush are summed (one `INSERT … ON CONFLICT` cannot touch a row
/// twice), and an empty flush is a no-op.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn duplicate_keys_in_one_flush_are_summed(pool: PgPool) {
    let id = building(&pool, "2026-10").await;
    let mut rows = three_games();
    rows.extend(three_games());
    flush_explorer(&pool, id, &rows).await.unwrap();
    flush_explorer(&pool, id, &[]).await.unwrap();
    activate(&pool, id).await;
    let moves = query_explorer(&pool, &start_key(), Band::B2, TimeClass::Blitz)
        .await
        .unwrap();
    assert_eq!(moves, vec![mv("e2e4", 2, 0, 2), mv("d2d4", 0, 2, 0)]);
}

/// More rows than one 5 000-row chunk.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn large_flush_is_chunked_and_complete(pool: PgPool) {
    let id = building(&pool, "2026-10").await;
    let rows: Vec<_> = (0..12_001)
        .map(|i| {
            (
                ExplorerKey {
                    fen_key: format!("f{i}"),
                    band: Band::ALL[i % 6],
                    time_class: TimeClass::Blitz,
                    uci: "e2e4".into(),
                },
                ExplorerCounts {
                    white: 1,
                    draws: 2,
                    black: 3,
                },
            )
        })
        .collect();
    flush_explorer(&pool, id, &rows).await.unwrap();
    let (n, sum): (i64, i64) = sqlx::query_as(
        "SELECT count(*), sum(white + draws + black)::int8 FROM opening_explorer \
         WHERE build_id = $1",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!((n, sum), (12_001, 6 * 12_001));
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn prune_removes_rows_below_min_count(pool: PgPool) {
    let id = building(&pool, "2026-10").await;
    flush_explorer(&pool, id, &three_games()).await.unwrap();
    // Totals after one flush: e2e4 2; d2d4, e7e5, c7c5, d7d5 1 each.
    assert_eq!(prune_explorer(&pool, id, 2).await.unwrap(), 4);
    activate(&pool, id).await;
    let moves = query_explorer(&pool, &start_key(), Band::B2, TimeClass::Blitz)
        .await
        .unwrap();
    assert_eq!(moves, vec![mv("e2e4", 1, 0, 1)]);
    assert!(!moves.iter().any(|m| m.uci == "d2d4"));
    assert_eq!(row_count(&pool, id).await, 1);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn second_build_supersedes_the_first(pool: PgPool) {
    let first = building(&pool, "2026-09").await;
    flush_explorer(&pool, first, &three_games()).await.unwrap();
    activate(&pool, first).await;

    let second = building(&pool, "2026-10").await;
    let rows = three_games();
    flush_explorer(&pool, second, &rows).await.unwrap();
    flush_explorer(&pool, second, &rows).await.unwrap();
    // Until activation, reads still serve the first build.
    assert_eq!(
        query_explorer(&pool, &start_key(), Band::B2, TimeClass::Blitz)
            .await
            .unwrap()[0],
        mv("e2e4", 1, 0, 1)
    );
    activate(&pool, second).await;
    assert_eq!(row_count(&pool, first).await, 0);
    assert_eq!(
        active_build(&pool, BuildKind::OpeningExplorer)
            .await
            .unwrap(),
        Some(second)
    );
    assert_eq!(
        query_explorer(&pool, &start_key(), Band::B2, TimeClass::Blitz)
            .await
            .unwrap(),
        vec![mv("e2e4", 2, 0, 2), mv("d2d4", 0, 2, 0)]
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn reads_without_an_active_build_are_empty(pool: PgPool) {
    let id = building(&pool, "2026-10").await;
    flush_explorer(&pool, id, &three_games()).await.unwrap();
    assert!(
        query_explorer(&pool, &start_key(), Band::B2, TimeClass::Blitz)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        load_explorer_index(&pool, &[start_key()], Band::B2, TimeClass::Blitz)
            .await
            .unwrap()
            .is_empty()
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn load_index_batches_and_keeps_only_known_positions(pool: PgPool) {
    let id = building(&pool, "2026-10").await;
    flush_explorer(&pool, id, &three_games()).await.unwrap();
    // 2 500 positions over three batches of 1 000; only the even ones exist.
    let rows: Vec<_> = (0..2_500)
        .filter(|i| i % 2 == 0)
        .map(|i| {
            (
                ExplorerKey {
                    fen_key: format!("k{i}"),
                    band: Band::B2,
                    time_class: TimeClass::Blitz,
                    uci: "g1f3".into(),
                },
                ExplorerCounts {
                    white: i,
                    draws: 0,
                    black: 0,
                },
            )
        })
        .collect();
    flush_explorer(&pool, id, &rows).await.unwrap();
    // A B3 row for the start position must not leak into a B2 index.
    flush_explorer(
        &pool,
        id,
        &[(
            ExplorerKey {
                fen_key: start_key(),
                band: Band::B3,
                time_class: TimeClass::Blitz,
                uci: "c2c4".into(),
            },
            ExplorerCounts {
                white: 100,
                draws: 0,
                black: 0,
            },
        )],
    )
    .await
    .unwrap();
    activate(&pool, id).await;

    let mut keys: Vec<String> = (0..2_500).map(|i| format!("k{i}")).collect();
    keys.push(start_key());
    keys.push(start_key()); // duplicates are harmless
    keys.push("unknown".into());
    let index = load_explorer_index(&pool, &keys, Band::B2, TimeClass::Blitz)
        .await
        .unwrap();
    // The 1 250 existing k-keys plus the start position; after-1.e4/1.d4 were not requested.
    assert_eq!(index.len(), 1_250 + 1);
    assert!(index.contains_key("k2498"));
    assert!(!index.contains_key("k1"));
    assert!(!index.contains_key("unknown"));
    assert_eq!(index["k2498"], vec![mv("g1f3", 2_498, 0, 0)]);
    assert_eq!(
        index[&start_key()],
        vec![mv("e2e4", 1, 0, 1), mv("d2d4", 0, 1, 0)]
    );
    assert!(
        load_explorer_index(&pool, &[], Band::B2, TimeClass::Blitz)
            .await
            .unwrap()
            .is_empty()
    );
}

fn index_of(moves: Vec<ExplorerMove>) -> HashMap<String, Vec<ExplorerMove>> {
    HashMap::from([("pos".to_string(), moves)])
}

#[test]
fn in_theory_needs_50_games_and_a_5_percent_share() {
    // Position total 49: never theory, however popular the move.
    let idx = index_of(vec![mv("e2e4", 30, 0, 0), mv("d2d4", 19, 0, 0)]);
    assert!(!in_theory(&idx, "pos", "e2e4"));
    // Total 50, share 30/50: theory.
    let idx = index_of(vec![mv("e2e4", 30, 0, 0), mv("d2d4", 20, 0, 0)]);
    assert!(in_theory(&idx, "pos", "e2e4"));
    // Exactly 5% (3 of 60) is theory; 2 of 50 (4%) is not.
    let idx = index_of(vec![mv("e2e4", 57, 0, 0), mv("a2a3", 1, 1, 1)]);
    assert!(in_theory(&idx, "pos", "a2a3"));
    let idx = index_of(vec![mv("e2e4", 48, 0, 0), mv("a2a3", 1, 0, 1)]);
    assert!(!in_theory(&idx, "pos", "a2a3"));
    // Unknown move or position.
    assert!(!in_theory(&idx, "pos", "h2h4"));
    assert!(!in_theory(&idx, "other", "e2e4"));
}

/// Plan guard for both hot reads, under custom and generic plans: each uses
/// `opening_explorer_pkey` and never scans `opening_explorer` sequentially. (`reference_builds`
/// holds a handful of rows; its active-build lookup may be a seq scan.)
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn explorer_reads_use_the_primary_key(pool: PgPool) {
    sqlx::raw_sql(
        "INSERT INTO reference_builds (kind, status, source_month, source_sha256) \
           SELECT 'opening_explorer', 'superseded', '2020-' || g, 'x' FROM generate_series(1, 20) g; \
         INSERT INTO reference_builds (kind, status, source_month, source_sha256) \
           VALUES ('opening_explorer', 'active', '2026-10', 'a'), \
                  ('opening_explorer', 'building', '2026-11', 'b'); \
         INSERT INTO opening_explorer (build_id, fen_key, band, time_class, uci, white, draws, black) \
           SELECT b.id, 'f' || f, band::band, tc::time_class, u, f % 7, 1, f % 5 \
           FROM reference_builds b, generate_series(1, 5000) f, \
                unnest(ARRAY['B1', 'B2', 'B3']) band, unnest(ARRAY['blitz', 'rapid']) tc, \
                unnest(ARRAY['e2e4', 'd2d4', 'g1f3', 'c2c4']) u \
           WHERE b.status IN ('active', 'building')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::raw_sql("VACUUM ANALYZE")
        .execute(&pool)
        .await
        .unwrap();
    let mut conn = pool.acquire().await.unwrap();
    let reads = [
        ("q_explorer", QUERY_EXPLORER_SQL, "'f17', 'B2', 'blitz'"),
        (
            "q_index",
            EXPLORER_INDEX_SQL,
            "ARRAY['f1', 'f17', 'f1999', 'f4000'], 'B2', 'blitz'",
        ),
    ];
    let mut failures = Vec::new();
    for (name, sql, args) in reads {
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!("PREPARE {name} AS {sql}")))
            .execute(&mut *conn)
            .await
            .unwrap();
        for mode in ["force_custom_plan", "force_generic_plan"] {
            sqlx::query("SELECT set_config('plan_cache_mode', $1, false)")
                .bind(mode)
                .execute(&mut *conn)
                .await
                .unwrap();
            let plan: Vec<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
                "EXPLAIN EXECUTE {name}({args})"
            )))
            .fetch_all(&mut *conn)
            .await
            .unwrap();
            let plan = plan.join("\n");
            if !plan.contains(" opening_explorer_pkey ")
                || plan.contains("Seq Scan on opening_explorer")
            {
                failures.push(format!("{name} {mode}:\n{plan}"));
            }
        }
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!("DEALLOCATE {name}")))
            .execute(&mut *conn)
            .await
            .unwrap();
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

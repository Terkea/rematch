//! R2 Task 17: reference games, observations and band reference storage
//! (`refdata::{reference_games, band_reference}`). TS M2 Task 14's storage cases: the
//! low-sample fallback of `loadBandReference`, supersede, plus a plan guard on the hot reads.

use chess_core::{Color, TimeClass};
use db::games::AnalysisStatus;
use profile_core::types::{
    Band, BandReferenceStat, BaseMetric, MetricId, PLAYER_GAME_METRICS_VERSION, PlayerGameMetrics,
    WeaknessKey,
};
use refdata::band_reference::{
    BAND_CELL_SQL, BandReferenceRow, load_band_reference, write_band_reference,
};
use refdata::builds::{
    BuildKind, NewBuild, RefdataError, activate_build, create_build, fail_build, set_source_sha,
};
use refdata::reference_games::{
    NewReferenceGame, ObservationRow, PENDING_REFERENCE_SQL, insert_reference_games,
    pending_reference_count, set_reference_game_status, write_observations,
};
use serde_json::json;
use sqlx::PgPool;
use time::macros::datetime;
use uuid::Uuid;

const W1: MetricId = MetricId::Weakness(WeaknessKey::W1);
const ACC: MetricId = MetricId::Base(BaseMetric::AccuracyOpening);

async fn band_build(pool: &PgPool, month: &str) -> Uuid {
    create_build(
        pool,
        &NewBuild {
            kind: BuildKind::BandReference,
            source_month: month.into(),
            source_url: None,
            params: json!({ "players_per_cell": 20, "games_per_player": 10 }),
            protocol: Some("sf19-n300k-v1".into()),
            enrichment_protocol: Some("enr-v1".into()),
            difficulty_model: None,
        },
    )
    .await
    .unwrap()
}

async fn activate(pool: &PgPool, id: Uuid) {
    set_source_sha(pool, id, &"cd".repeat(32), 10)
        .await
        .unwrap();
    activate_build(pool, id).await.unwrap();
}

fn stat(metric: MetricId, median: f64, n_players: u32) -> BandReferenceStat {
    BandReferenceStat {
        metric,
        median,
        p25: median * 0.5,
        p40: median * 0.8,
        p75: median * 1.5,
        pooled_rate: Some(median),
        epl_per_event: Some(12.25),
        opps_per_game: None,
        opp_slope_per_100: -0.125,
        n_players,
        n_games: n_players * 10,
        low_sample: n_players < 10,
    }
}

fn cell(band: Band, median: f64, n_players: u32) -> Vec<BandReferenceRow> {
    [W1, ACC]
        .into_iter()
        .map(|m| BandReferenceRow {
            band,
            time_class: TimeClass::Blitz,
            stat: stat(m, median, n_players),
        })
        .collect()
}

/// What a `real` column gives back.
fn as_real(x: f64) -> f64 {
    f64::from(x as f32)
}

async fn write(pool: &PgPool, build: Uuid, rows: &[BandReferenceRow]) {
    let mut conn = pool.acquire().await.unwrap();
    write_band_reference(&mut conn, build, rows).await.unwrap();
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn band_reference_round_trips(pool: PgPool) {
    let id = band_build(&pool, "2026-10").await;
    write(&pool, id, &cell(Band::B2, 0.03, 20)).await;
    activate(&pool, id).await;
    let (build, r) = load_band_reference(&pool, Band::B2, TimeClass::Blitz)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(build, id);
    assert_eq!(r.len(), 2);
    let s = &r[&W1];
    let mut want = stat(W1, as_real(0.03), 20);
    want.p25 = as_real(0.03 * 0.5);
    want.p40 = as_real(0.03 * 0.8);
    want.p75 = as_real(0.03 * 1.5);
    want.pooled_rate = Some(as_real(0.03));
    assert_eq!(*s, want);
    assert_eq!(r[&ACC].opps_per_game, None);
    // Another time class has no rows.
    assert!(
        load_band_reference(&pool, Band::B2, TimeClass::Rapid)
            .await
            .unwrap()
            .is_none()
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn rewriting_a_cell_replaces_its_rows(pool: PgPool) {
    let id = band_build(&pool, "2026-10").await;
    write(&pool, id, &cell(Band::B2, 0.03, 20)).await;
    write(&pool, id, &cell(Band::B2, 0.5, 30)).await;
    activate(&pool, id).await;
    let (_, r) = load_band_reference(&pool, Band::B2, TimeClass::Blitz)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(r[&W1].median, 0.5);
    assert_eq!(r[&W1].n_players, 30);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn low_sample_cell_falls_back_to_the_nearest_band(pool: PgPool) {
    let id = band_build(&pool, "2026-10").await;
    let mut rows = cell(Band::B3, 0.9, 5); // low sample
    rows.extend(cell(Band::B2, 0.2, 20));
    rows.extend(cell(Band::B0, 0.7, 20));
    write(&pool, id, &rows).await;
    activate(&pool, id).await;
    let (build, r) = load_band_reference(&pool, Band::B3, TimeClass::Blitz)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(build, id);
    assert_eq!(r[&W1].median, as_real(0.2), "B3 falls back to B2");
    // B5 has no rows: nearest usable is B2 (B4, B3 are missing or low).
    let (_, r) = load_band_reference(&pool, Band::B5, TimeClass::Blitz)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(r[&W1].median, as_real(0.2));
    // B1 is equally far from B0 and B2: the lower band wins.
    let (_, r) = load_band_reference(&pool, Band::B1, TimeClass::Blitz)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(r[&W1].median, as_real(0.7));
    // A usable cell is served as itself.
    let (_, r) = load_band_reference(&pool, Band::B2, TimeClass::Blitz)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(r[&W1].median, as_real(0.2));
}

/// A cell is low-sample only when every row in it is; a cell with some low-sample metrics is
/// served as itself and the rows keep their own `low_sample` flag.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn partly_low_cell_is_served_with_its_flags(pool: PgPool) {
    let id = band_build(&pool, "2026-10").await;
    let rows = vec![
        BandReferenceRow {
            band: Band::B3,
            time_class: TimeClass::Blitz,
            stat: stat(W1, 0.9, 5),
        },
        BandReferenceRow {
            band: Band::B3,
            time_class: TimeClass::Blitz,
            stat: stat(ACC, 0.9, 20),
        },
    ];
    write(&pool, id, &rows).await;
    write(&pool, id, &cell(Band::B2, 0.2, 20)).await;
    activate(&pool, id).await;
    let (_, r) = load_band_reference(&pool, Band::B3, TimeClass::Blitz)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(r[&W1].median, as_real(0.9));
    assert!(r[&W1].low_sample);
    assert!(!r[&ACC].low_sample);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn every_cell_low_sample_gives_none(pool: PgPool) {
    let id = band_build(&pool, "2026-10").await;
    let mut rows = Vec::new();
    for b in Band::ALL {
        rows.extend(cell(b, 0.1, 9));
    }
    // A usable cell in another time class does not count.
    rows.push(BandReferenceRow {
        band: Band::B2,
        time_class: TimeClass::Rapid,
        stat: stat(W1, 0.1, 50),
    });
    write(&pool, id, &rows).await;
    activate(&pool, id).await;
    for b in Band::ALL {
        assert!(
            load_band_reference(&pool, b, TimeClass::Blitz)
                .await
                .unwrap()
                .is_none(),
            "{b:?}"
        );
    }
    assert!(
        load_band_reference(&pool, Band::B4, TimeClass::Rapid)
            .await
            .unwrap()
            .is_some()
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn only_the_active_build_is_read(pool: PgPool) {
    let building = band_build(&pool, "2026-10").await;
    write(&pool, building, &cell(Band::B2, 0.2, 20)).await;
    assert!(
        load_band_reference(&pool, Band::B2, TimeClass::Blitz)
            .await
            .unwrap()
            .is_none()
    );
    activate(&pool, building).await;
    let next = band_build(&pool, "2026-11").await;
    write(&pool, next, &cell(Band::B2, 0.4, 20)).await;
    let (b, r) = load_band_reference(&pool, Band::B2, TimeClass::Blitz)
        .await
        .unwrap()
        .unwrap();
    assert_eq!((b, r[&W1].median), (building, as_real(0.2)));
    activate(&pool, next).await;
    let (b, r) = load_band_reference(&pool, Band::B2, TimeClass::Blitz)
        .await
        .unwrap()
        .unwrap();
    assert_eq!((b, r[&W1].median), (next, as_real(0.4)));
}

fn game(id: &str) -> NewReferenceGame {
    NewReferenceGame {
        lichess_game_id: id.into(),
        pgn: format!("[Site \"https://lichess.org/{id}\"]\n\n1. e4 e5 1-0\n"),
        time_class: TimeClass::Blitz,
        base_seconds: Some(180),
        increment_seconds: Some(2),
        white_rating: Some(1500),
        black_rating: Some(1550),
        white_hash: "a".repeat(64),
        black_hash: "b".repeat(64),
        result: "1-0".into(),
        termination: Some("Normal".into()),
        played_at: datetime!(2026-10-03 12:00:00 UTC),
    }
}

fn metrics() -> PlayerGameMetrics {
    PlayerGameMetrics {
        version: PLAYER_GAME_METRICS_VERSION,
        moves: 31,
        blunders: 2,
        accuracy: Some(81.5),
        ..Default::default()
    }
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn reference_games_insert_in_order_and_count_pending(pool: PgPool) {
    let id = band_build(&pool, "2026-10").await;
    let games: Vec<_> = (0..1_205).map(|i| game(&format!("g{i:05}"))).collect();
    let ids = insert_reference_games(&pool, id, &games).await.unwrap();
    assert_eq!(ids.len(), 1_205);
    // Ids follow the input order.
    let lichess_id: String =
        sqlx::query_scalar("SELECT lichess_game_id FROM reference_games WHERE id = $1")
            .bind(ids[777])
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(lichess_id, "g00777");
    // Re-inserting (a retried enqueue) returns the same ids and adds no rows.
    let again = insert_reference_games(&pool, id, &games[..3])
        .await
        .unwrap();
    assert_eq!(again, ids[..3]);
    assert_eq!(pending_reference_count(&pool, id).await.unwrap(), 1_205);
    let row: (String, String, Option<i32>, String) = sqlx::query_as(
        "SELECT status::text, time_class::text, base_seconds, result::text \
         FROM reference_games WHERE id = $1",
    )
    .bind(ids[0])
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        row,
        ("pending".into(), "blitz".into(), Some(180), "1-0".into())
    );

    // `analysing` still counts as outstanding; `done` and `failed` do not.
    let mut conn = pool.acquire().await.unwrap();
    set_reference_game_status(&mut conn, ids[0], AnalysisStatus::Analysing, None)
        .await
        .unwrap();
    set_reference_game_status(&mut conn, ids[1], AnalysisStatus::Done, None)
        .await
        .unwrap();
    set_reference_game_status(
        &mut conn,
        ids[2],
        AnalysisStatus::Failed,
        Some("engine died"),
    )
    .await
    .unwrap();
    assert_eq!(pending_reference_count(&pool, id).await.unwrap(), 1_203);
    let err: Option<String> = sqlx::query_scalar("SELECT error FROM reference_games WHERE id = $1")
        .bind(ids[2])
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(err.as_deref(), Some("engine died"));
    assert!(matches!(
        set_reference_game_status(&mut conn, Uuid::new_v4(), AnalysisStatus::Done, None).await,
        Err(RefdataError::NotFound(_))
    ));
    assert!(
        insert_reference_games(&pool, id, &[])
            .await
            .unwrap()
            .is_empty()
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn reference_games_need_a_building_build(pool: PgPool) {
    let id = band_build(&pool, "2026-10").await;
    fail_build(&pool, id, "sha256 mismatch").await.unwrap();
    assert!(matches!(
        insert_reference_games(&pool, id, &[game("g1")]).await,
        Err(RefdataError::NotBuilding { .. })
    ));
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn observations_upsert_per_game_and_colour(pool: PgPool) {
    let id = band_build(&pool, "2026-10").await;
    let ids = insert_reference_games(&pool, id, &[game("g1")])
        .await
        .unwrap();
    let obs = |color, m: PlayerGameMetrics| ObservationRow {
        reference_game_id: ids[0],
        color,
        player_hash: "a".repeat(64),
        band: Band::B2,
        time_class: TimeClass::Blitz,
        player_rating: 1500,
        opponent_rating: 1550,
        metrics: m,
    };
    let mut conn = pool.acquire().await.unwrap();
    write_observations(
        &mut conn,
        id,
        &[obs(Color::White, metrics()), obs(Color::Black, metrics())],
    )
    .await
    .unwrap();
    let mut changed = metrics();
    changed.blunders = 5;
    write_observations(&mut conn, id, &[obs(Color::White, changed.clone())])
        .await
        .unwrap();
    let rows: Vec<(String, serde_json::Value)> = sqlx::query_as(
        "SELECT color::text, metrics FROM reference_observations WHERE build_id = $1 \
         ORDER BY color",
    )
    .bind(id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(rows.len(), 2);
    let white = rows.iter().find(|r| r.0 == "white").unwrap();
    let back: PlayerGameMetrics = serde_json::from_value(white.1.clone()).unwrap();
    assert_eq!(back, changed);
    assert_eq!(white.1["version"], json!(PLAYER_GAME_METRICS_VERSION));
}

/// Superseding a band-reference build deletes its rows in every reference table.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn superseded_band_build_loses_all_rows(pool: PgPool) {
    let first = band_build(&pool, "2026-09").await;
    let ids = insert_reference_games(&pool, first, &[game("g1")])
        .await
        .unwrap();
    let mut conn = pool.acquire().await.unwrap();
    write_observations(
        &mut conn,
        first,
        &[ObservationRow {
            reference_game_id: ids[0],
            color: Color::White,
            player_hash: "a".repeat(64),
            band: Band::B2,
            time_class: TimeClass::Blitz,
            player_rating: 1500,
            opponent_rating: 1550,
            metrics: metrics(),
        }],
    )
    .await
    .unwrap();
    write_band_reference(&mut conn, first, &cell(Band::B2, 0.2, 20))
        .await
        .unwrap();
    drop(conn);
    activate(&pool, first).await;
    let second = band_build(&pool, "2026-10").await;
    write(&pool, second, &cell(Band::B2, 0.3, 20)).await;
    activate(&pool, second).await;
    for table in [
        "band_reference",
        "reference_observations",
        "reference_games",
    ] {
        let n: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "SELECT count(*) FROM {table} WHERE build_id = $1"
        )))
        .bind(first)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(n, 0, "{table}");
    }
}

/// Plan guard, custom and generic plans: a band-reference cell read uses `band_reference_pkey`
/// and the outstanding-game count uses `reference_games_build_status_idx`, with no seq scan.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn band_reference_reads_use_their_indexes(pool: PgPool) {
    sqlx::raw_sql(
        "INSERT INTO reference_builds (kind, status, source_month, source_sha256) \
           SELECT 'band_reference', 'superseded', '2020-' || g, 'x' FROM generate_series(1, 20) g; \
         INSERT INTO reference_builds (kind, status, source_month, source_sha256) \
           VALUES ('band_reference', 'active', '2026-10', 'a'), \
                  ('band_reference', 'building', '2026-11', 'b'); \
         INSERT INTO band_reference (build_id, band, time_class, metric, median, p25, p40, p75, \
             opp_slope_per_100, n_players, n_games, low_sample) \
           SELECT b.id, band::band, tc::time_class, 'm' || m, 0.5, 0.2, 0.4, 0.7, 0, 20, 200, false \
           FROM reference_builds b, unnest(ARRAY['B0', 'B1', 'B2', 'B3', 'B4', 'B5']) band, \
                unnest(ARRAY['bullet', 'blitz', 'rapid', 'classical', 'daily']) tc, \
                generate_series(1, 80) m \
           WHERE b.status IN ('active', 'building'); \
         INSERT INTO reference_games (build_id, lichess_game_id, pgn, time_class, white_hash, \
             black_hash, result, played_at, status) \
           SELECT b.id, 'g' || i, '1. e4 *', 'blitz', 'w', 'b', '*', now(), \
                  CASE WHEN i % 20 = 0 THEN 'pending' ELSE 'done' END::analysis_status \
           FROM reference_builds b, generate_series(1, 6000) i \
           WHERE b.status IN ('active', 'building')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::raw_sql("VACUUM ANALYZE")
        .execute(&pool)
        .await
        .unwrap();
    let build: Uuid = sqlx::query_scalar(
        "SELECT id FROM reference_builds WHERE kind = 'band_reference' AND status = 'building'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let mut conn = pool.acquire().await.unwrap();
    let reads = [
        (
            "q_cell",
            BAND_CELL_SQL,
            format!("'{build}', 'B2', 'blitz'"),
            "band_reference_pkey",
        ),
        (
            "q_pending",
            PENDING_REFERENCE_SQL,
            format!("'{build}'"),
            "reference_games_build_status_idx",
        ),
    ];
    let mut failures = Vec::new();
    for (name, sql, args, uses) in reads {
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
            if !plan.contains(&format!(" {uses} ")) || plan.contains("Seq Scan") {
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

//! R2 Task 17: reference games, observations and band reference storage
//! (`refdata::{reference_games, band_reference}`). TS M2 Task 14's `loadBandReference` storage
//! cases with the per-metric fallback ruling (ADR-R2-23), supersede, the write guards, plus a
//! plan guard on the hot reads.

use chess_core::{Color, TimeClass};
use db::games::AnalysisStatus;
use profile_core::types::{
    Band, BandReferenceStat, BaseMetric, MetricId, PLAYER_GAME_METRICS_VERSION, PlayerGameMetrics,
    WeaknessKey,
};
use refdata::band_reference::{
    BAND_REFERENCE_SQL, BandReferenceRow, LoadedBandReference, MAX_FALLBACK_BANDS,
    load_band_reference, write_band_reference,
};
use refdata::builds::{
    BuildKind, BuildStatus, NewBuild, RefdataError, activate_build, create_build, fail_build,
    set_source_sha,
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

fn row(band: Band, metric: MetricId, median: f64, n_players: u32) -> BandReferenceRow {
    BandReferenceRow {
        band,
        time_class: TimeClass::Blitz,
        stat: stat(metric, median, n_players),
    }
}

fn cell(band: Band, median: f64, n_players: u32) -> Vec<BandReferenceRow> {
    [W1, ACC]
        .into_iter()
        .map(|m| row(band, m, median, n_players))
        .collect()
}

/// What a `real` column gives back.
fn as_real(x: f64) -> f64 {
    f64::from(x as f32)
}

async fn write(pool: &PgPool, build: Uuid, rows: &[BandReferenceRow]) {
    let mut tx = pool.begin().await.unwrap();
    write_band_reference(&mut tx, build, rows).await.unwrap();
    tx.commit().await.unwrap();
}

async fn load(pool: &PgPool, band: Band) -> Option<LoadedBandReference> {
    load_band_reference(pool, band, TimeClass::Blitz)
        .await
        .unwrap()
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn band_reference_round_trips(pool: PgPool) {
    let id = band_build(&pool, "2026-10").await;
    write(&pool, id, &cell(Band::B2, 0.03, 20)).await;
    activate(&pool, id).await;
    let r = load(&pool, Band::B2).await.unwrap();
    assert_eq!(r.build_id, id);
    assert_eq!(r.requested, Band::B2);
    assert_eq!(r.time_class, TimeClass::Blitz);
    assert_eq!(r.stats.len(), 2);
    let mut want = stat(W1, as_real(0.03), 20);
    want.p25 = as_real(0.03 * 0.5);
    want.p40 = as_real(0.03 * 0.8);
    want.p75 = as_real(0.03 * 1.5);
    want.pooled_rate = Some(as_real(0.03));
    assert_eq!(r.stats[&W1].stat, want);
    assert_eq!(r.served_from(W1), Some(Band::B2));
    assert!(!r.is_fallback(W1));
    assert_eq!(r.reference()[&W1], want);
    assert_eq!(r.reference()[&ACC].opps_per_game, None);
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
    let r = load(&pool, Band::B2).await.unwrap();
    assert_eq!(r.stats[&W1].stat.median, 0.5);
    assert_eq!(r.stats[&W1].stat.n_players, 30);
}

/// The review's case: B3's W1 row is low-sample while B3's other metrics are fine. W1 must come
/// from B2 (labelled as such), never from B3's low-sample row; the rest stays B3.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn b3_low_sample_w1_falls_back_per_metric(pool: PgPool) {
    let id = band_build(&pool, "2026-10").await;
    write(
        &pool,
        id,
        &[
            row(Band::B3, W1, 0.9, 5),
            row(Band::B3, ACC, 0.8, 20),
            row(Band::B2, W1, 0.2, 20),
            row(Band::B2, ACC, 0.3, 20),
        ],
    )
    .await;
    activate(&pool, id).await;
    let r = load(&pool, Band::B3).await.unwrap();
    assert_eq!(r.served_from(W1), Some(Band::B2));
    assert!(r.is_fallback(W1));
    assert_eq!(r.stats[&W1].stat.median, as_real(0.2));
    assert_eq!(r.served_from(ACC), Some(Band::B3));
    assert!(!r.is_fallback(ACC));
    assert_eq!(r.stats[&ACC].stat.median, as_real(0.8));
    assert!(r.stats.values().all(|s| !s.stat.low_sample));
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn fallback_reaches_at_most_two_bands(pool: PgPool) {
    assert_eq!(MAX_FALLBACK_BANDS, 2);
    let id = band_build(&pool, "2026-10").await;
    // W1 is usable only in B0; ACC only in B5.
    write(
        &pool,
        id,
        &[row(Band::B0, W1, 0.7, 20), row(Band::B5, ACC, 0.6, 20)],
    )
    .await;
    activate(&pool, id).await;
    // B2: W1 from B0 (2 away); ACC in B5 is 3 away, so absent.
    let r = load(&pool, Band::B2).await.unwrap();
    assert_eq!(r.served_from(W1), Some(Band::B0));
    assert_eq!(r.served_from(ACC), None);
    // B3: both are exactly within reach the other way round.
    let r = load(&pool, Band::B3).await.unwrap();
    assert_eq!(r.served_from(W1), None);
    assert_eq!(r.served_from(ACC), Some(Band::B5));
    // B4 reaches B5 only; B0 is 4 away.
    let r = load(&pool, Band::B4).await.unwrap();
    assert_eq!(r.stats.keys().copied().collect::<Vec<_>>(), vec![ACC]);
    assert_eq!(r.served_from(ACC), Some(Band::B5));
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn nearest_band_wins_and_ties_go_lower(pool: PgPool) {
    let id = band_build(&pool, "2026-10").await;
    let mut rows = cell(Band::B0, 0.7, 20);
    rows.extend(cell(Band::B2, 0.2, 20));
    rows.extend(cell(Band::B3, 0.9, 5)); // low sample
    write(&pool, id, &rows).await;
    activate(&pool, id).await;
    // B1 is one away from both B0 and B2: the lower band wins.
    let r = load(&pool, Band::B1).await.unwrap();
    assert_eq!(r.served_from(W1), Some(Band::B0));
    // B3 (low) → B2 (one away) rather than B0 (three away).
    let r = load(&pool, Band::B3).await.unwrap();
    assert_eq!(r.served_from(W1), Some(Band::B2));
    // B5: B3 is low and B4 empty, B2 is three away: nothing.
    assert!(load(&pool, Band::B5).await.is_none());
    // A usable band is served as itself.
    assert_eq!(
        load(&pool, Band::B2).await.unwrap().served_from(W1),
        Some(Band::B2)
    );
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
        assert!(load(&pool, b).await.is_none(), "{b:?}");
    }
    assert!(
        load_band_reference(&pool, Band::B3, TimeClass::Rapid)
            .await
            .unwrap()
            .is_some()
    );
}

/// A cell holding only metrics this version does not know falls back like an empty one.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn unknown_metrics_are_ignored_and_fall_back(pool: PgPool) {
    let id = band_build(&pool, "2026-10").await;
    write(&pool, id, &[row(Band::B2, W1, 0.2, 20)]).await;
    sqlx::query(
        "INSERT INTO band_reference (build_id, band, time_class, metric, median, p25, p40, p75, \
           opp_slope_per_100, n_players, n_games, low_sample) \
         VALUES ($1, 'B3', 'blitz', 'metric_from_the_future', 1, 1, 1, 1, 0, 50, 500, false)",
    )
    .bind(id)
    .execute(&pool)
    .await
    .unwrap();
    activate(&pool, id).await;
    let r = load(&pool, Band::B3).await.unwrap();
    assert_eq!(r.stats.len(), 1);
    assert_eq!(r.served_from(W1), Some(Band::B2));
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn only_the_active_build_is_read(pool: PgPool) {
    let building = band_build(&pool, "2026-10").await;
    write(&pool, building, &cell(Band::B2, 0.2, 20)).await;
    assert!(load(&pool, Band::B2).await.is_none());
    activate(&pool, building).await;
    let next = band_build(&pool, "2026-11").await;
    write(&pool, next, &cell(Band::B2, 0.4, 20)).await;
    let r = load(&pool, Band::B2).await.unwrap();
    assert_eq!(
        (r.build_id, r.stats[&W1].stat.median),
        (building, as_real(0.2))
    );
    activate(&pool, next).await;
    let r = load(&pool, Band::B2).await.unwrap();
    assert_eq!((r.build_id, r.stats[&W1].stat.median), (next, as_real(0.4)));
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn band_reference_writes_need_a_building_build(pool: PgPool) {
    let id = band_build(&pool, "2026-10").await;
    write(&pool, id, &cell(Band::B2, 0.2, 20)).await;
    activate(&pool, id).await;
    let mut tx = pool.begin().await.unwrap();
    assert!(matches!(
        write_band_reference(&mut tx, id, &cell(Band::B2, 0.9, 20)).await,
        Err(RefdataError::NotBuilding {
            status: BuildStatus::Active,
            ..
        })
    ));
    tx.rollback().await.unwrap();
    assert_eq!(
        load(&pool, Band::B2).await.unwrap().stats[&W1].stat.median,
        as_real(0.2)
    );
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

fn obs(game: Uuid, color: Color, m: PlayerGameMetrics) -> ObservationRow {
    ObservationRow {
        reference_game_id: game,
        color,
        player_hash: "a".repeat(64),
        band: Band::B2,
        time_class: TimeClass::Blitz,
        player_rating: 1500,
        opponent_rating: 1550,
        metrics: m,
    }
}

async fn observe(pool: &PgPool, build: Uuid, rows: &[ObservationRow]) -> Result<(), RefdataError> {
    let mut tx = pool.begin().await.unwrap();
    write_observations(&mut tx, build, rows).await?;
    tx.commit().await.unwrap();
    Ok(())
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
    let r: (String, String, Option<i32>, String) = sqlx::query_as(
        "SELECT status::text, time_class::text, base_seconds, result::text \
         FROM reference_games WHERE id = $1",
    )
    .bind(ids[0])
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        r,
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

/// After finalisation a late worker's status update is refused (it treats `NotBuilding` as
/// "already done"), and the game keeps its status.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn status_updates_need_a_building_build(pool: PgPool) {
    let id = band_build(&pool, "2026-10").await;
    let ids = insert_reference_games(&pool, id, &[game("g1")])
        .await
        .unwrap();
    activate(&pool, id).await;
    let mut conn = pool.acquire().await.unwrap();
    assert!(matches!(
        set_reference_game_status(&mut conn, ids[0], AnalysisStatus::Done, None).await,
        Err(RefdataError::NotBuilding {
            status: BuildStatus::Active,
            ..
        })
    ));
    let status: String =
        sqlx::query_scalar("SELECT status::text FROM reference_games WHERE id = $1")
            .bind(ids[0])
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(status, "pending");
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn observations_upsert_per_game_and_colour(pool: PgPool) {
    let id = band_build(&pool, "2026-10").await;
    let ids = insert_reference_games(&pool, id, &[game("g1")])
        .await
        .unwrap();
    observe(
        &pool,
        id,
        &[
            obs(ids[0], Color::White, metrics()),
            obs(ids[0], Color::Black, metrics()),
        ],
    )
    .await
    .unwrap();
    let mut changed = metrics();
    changed.blunders = 5;
    observe(&pool, id, &[obs(ids[0], Color::White, changed.clone())])
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

/// An observation is written only under its game's own build, and never moves between builds.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn observations_must_match_their_games_build(pool: PgPool) {
    let a = band_build(&pool, "2026-10").await;
    let b = band_build(&pool, "2026-11").await;
    let ga = insert_reference_games(&pool, a, &[game("g1")])
        .await
        .unwrap()[0];
    let gb = insert_reference_games(&pool, b, &[game("g2")])
        .await
        .unwrap()[0];
    observe(&pool, a, &[obs(ga, Color::White, metrics())])
        .await
        .unwrap();
    // A row for b's game under a, alongside a valid one: the whole write is refused.
    assert!(matches!(
        observe(
            &pool,
            a,
            &[
                obs(ga, Color::Black, metrics()),
                obs(gb, Color::White, metrics())
            ]
        )
        .await,
        Err(RefdataError::BuildMismatch { game, build }) if game == gb && build == a
    ));
    // a's game under b is refused too.
    assert!(matches!(
        observe(&pool, b, &[obs(ga, Color::White, metrics())]).await,
        Err(RefdataError::BuildMismatch { .. })
    ));
    let rows: Vec<(Uuid, Uuid, String)> = sqlx::query_as(
        "SELECT build_id, reference_game_id, color::text FROM reference_observations",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(rows, vec![(a, ga, "white".into())]);
    // A finished build takes no observations.
    fail_build(&pool, b, "x").await.unwrap();
    assert!(matches!(
        observe(&pool, b, &[obs(gb, Color::White, metrics())]).await,
        Err(RefdataError::NotBuilding { .. })
    ));
}

/// Superseding a band-reference build deletes its rows in every reference table.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn superseded_band_build_loses_all_rows(pool: PgPool) {
    let first = band_build(&pool, "2026-09").await;
    let ids = insert_reference_games(&pool, first, &[game("g1")])
        .await
        .unwrap();
    observe(&pool, first, &[obs(ids[0], Color::White, metrics())])
        .await
        .unwrap();
    write(&pool, first, &cell(Band::B2, 0.2, 20)).await;
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

/// Plan guard, custom and generic plans: the band-reference read uses `band_reference_pkey`
/// with no seq scan on `band_reference` (the active-build lookup on the few-row
/// `reference_builds` may be one), and the outstanding-game count uses
/// `reference_games_build_status_idx` with no seq scan at all.
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
           SELECT b.id, band::band, tc::time_class, 'm' || m, 0.5, 0.2, 0.4, 0.7, 0, 20, 200, \
                  m % 10 = 0 \
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
            "q_reference",
            BAND_REFERENCE_SQL,
            "'{B1,B2,B3,B4,B5}', 'blitz'".to_string(),
            "band_reference_pkey",
            "Seq Scan on band_reference",
        ),
        (
            "q_pending",
            PENDING_REFERENCE_SQL,
            format!("'{build}'"),
            "reference_games_build_status_idx",
            "Seq Scan",
        ),
    ];
    let mut failures = Vec::new();
    for (name, sql, args, uses, forbidden) in reads {
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
            if !plan.contains(&format!(" {uses} ")) || plan.contains(forbidden) {
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

//! R2 Task 3: the profile-engine migration (`20261121020000_profile_engine.sql`), the `imported`
//! status, the row helpers in `db::profile_rows` and `db::stage_metrics`. Port of TS M2 Task 2's
//! `packages/db/test/profile-schema.test.ts` (the `truncateAllTables` case is dropped: every
//! `#[sqlx::test]` gets its own database), plus the R2 additions.

use std::collections::BTreeMap;

use db::accounts::find_or_create_account;
use db::games::{AnalysisStatus, NewGame, get_game, upsert_game};
use db::profile_rows::{
    GameMetricsRow, KpiSnapshotRow, SeriesPointRow, StateHistoryRow, WeaknessEstimateRow,
    append_kpi_snapshot, append_state_history, insert_game_metrics, replace_series_points,
    upsert_weakness_estimate,
};
use db::stage_metrics::{StageMetrics, record_stage_metrics};
use serde_json::json;
use sqlx::PgPool;
use time::OffsetDateTime;
use time::macros::datetime;
use uuid::Uuid;

const PLAYED_AT: OffsetDateTime = datetime!(2026-01-01 00:00:00 UTC);

async fn make_account(pool: &PgPool) -> Uuid {
    find_or_create_account(pool, "lichess", "Magnus")
        .await
        .unwrap()
        .id
}

/// An exhaustive `NewGame` literal, as the R1 callers write it (status `Pending`).
fn new_game(account_id: Uuid, platform_game_id: &str) -> NewGame {
    NewGame {
        account_id,
        platform: "lichess".into(),
        platform_game_id: platform_game_id.into(),
        pgn: "1. e4 e5 *".into(),
        played_at: PLAYED_AT,
        time_class: "blitz".into(),
        player_color: "white".into(),
        white_name: "Magnus".into(),
        black_name: "Hikaru".into(),
        result: "*".into(),
        rated: true,
        base_seconds: None,
        increment_seconds: None,
        white_rating: None,
        black_rating: None,
        termination: None,
        eco: None,
        opening_name: None,
        lichess_evals: None,
        status: AnalysisStatus::Pending,
        ply_count: None,
    }
}

async fn make_game(pool: &PgPool, account_id: Uuid, platform_game_id: &str) -> Uuid {
    upsert_game(pool, &new_game(account_id, platform_game_id))
        .await
        .unwrap()
        .id
}

async fn count(pool: &PgPool, sql: &str, id: Uuid) -> i64 {
    sqlx::query_scalar(sqlx::AssertSqlSafe(sql.to_owned()))
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn count_all(pool: &PgPool, table: &str) -> i64 {
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT count(*) FROM {table}")))
        .fetch_one(pool)
        .await
        .unwrap()
}

fn kpi_row(account_id: Uuid) -> KpiSnapshotRow {
    KpiSnapshotRow {
        user_id: None,
        account_id,
        time_class: "blitz".into(),
        metric: "blunder_rate".into(),
        window_kind: "last_n".into(),
        window_key: "last50".into(),
        value: Some(0.04),
        value_adjusted: Some(0.035),
        n: 50,
        ci_low: Some(0.02),
        ci_high: Some(0.06),
        protocol: "sf19-n300k-v1".into(),
    }
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn imported_status_round_trips(pool: PgPool) {
    let acc = make_account(&pool).await;
    let g = NewGame {
        status: AnalysisStatus::Imported,
        ply_count: Some(84),
        ..new_game(acc, "imp1")
    };
    let up = upsert_game(&pool, &g).await.unwrap();
    assert!(up.inserted);
    assert_eq!(up.analysis_status, AnalysisStatus::Imported);

    let row = get_game(&pool, up.id).await.unwrap().unwrap();
    assert_eq!(row.analysis_status, AnalysisStatus::Imported);
    assert_eq!(row.ply_count, Some(84));
    assert_eq!(row.analysis_tier, None);
    assert_eq!(row.analysis_priority, None);
    assert!(row.selected_at.is_none());
    assert!(row.analysed_at.is_none());
    assert!(row.enrichment_protocol.is_none());

    // The new columns read back once set.
    sqlx::query(
        "UPDATE games SET analysis_tier = 2, analysis_priority = 7, selected_at = $2, \
         analysed_at = $2, enrichment_protocol = 'enr-v1' WHERE id = $1",
    )
    .bind(up.id)
    .bind(PLAYED_AT)
    .execute(&pool)
    .await
    .unwrap();
    let row = get_game(&pool, up.id).await.unwrap().unwrap();
    assert_eq!(row.analysis_tier, Some(2));
    assert_eq!(row.analysis_priority, Some(7));
    assert_eq!(row.selected_at, Some(PLAYED_AT));
    assert_eq!(row.analysed_at, Some(PLAYED_AT));
    assert_eq!(row.enrichment_protocol.as_deref(), Some("enr-v1"));
    assert_eq!(
        "imported".parse::<AnalysisStatus>(),
        Ok(AnalysisStatus::Imported)
    );
    assert_eq!(AnalysisStatus::Imported.as_str(), "imported");
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn r1_callers_default_to_pending(pool: PgPool) {
    let acc = make_account(&pool).await;
    // The R1 call sites set `status: Pending, ply_count: None` explicitly.
    let up = upsert_game(&pool, &new_game(acc, "r1style")).await.unwrap();
    assert_eq!(up.analysis_status, AnalysisStatus::Pending);
    let row = get_game(&pool, up.id).await.unwrap().unwrap();
    assert_eq!(row.analysis_status, AnalysisStatus::Pending);
    assert_eq!(row.ply_count, None);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn one_row_per_new_table(pool: PgPool) {
    let acc = make_account(&pool).await;
    let game = make_game(&pool, acc, "g1").await;
    let mut conn = pool.acquire().await.unwrap();

    // Helpers used by Tasks 31-33.
    insert_game_metrics(
        &mut conn,
        &GameMetricsRow {
            game_id: game,
            account_id: acc,
            time_class: "blitz".into(),
            played_at: PLAYED_AT,
            protocol: "sf19-n300k-v1".into(),
            enrichment_protocol: Some("enr-v1".into()),
            difficulty_model: None,
            metrics: json!({ "version": 1 }),
        },
    )
    .await
    .unwrap();
    replace_series_points(
        &mut conn,
        game,
        &[SeriesPointRow {
            account_id: acc,
            time_class: "blitz".into(),
            series_key: "W1".into(),
            played_at: PLAYED_AT,
            events: 1,
            opportunities: 3,
            epl: 12.5,
            protocol: "enr-v1".into(),
        }],
    )
    .await
    .unwrap();
    upsert_weakness_estimate(
        &mut conn,
        &WeaknessEstimateRow {
            account_id: acc,
            time_class: "blitz".into(),
            series_key: "W1".into(),
            k: 1.0,
            n: 3.0,
            n_unweighted: 3,
            k_unweighted: 1.0,
            alpha: 2.0,
            beta: 3.0,
            mean: 0.4,
            ci_low: 0.2,
            ci_high: 0.6,
            band_median: Some(0.3),
            band_p40: Some(0.25),
            confidence: 0.5,
            excess: Some(0.1),
            priority: 0.2,
            percentile: Some(70.0),
            state: "emerging".into(),
            p_imp: None,
        },
    )
    .await
    .unwrap();
    let hist_id = append_state_history(
        &mut conn,
        &StateHistoryRow {
            account_id: acc,
            time_class: "blitz".into(),
            series_key: "W1".into(),
            kind: "transition".into(),
            from_state: None,
            to_state: "emerging".into(),
            at_game_id: Some(game),
            at_played_at: Some(PLAYED_AT),
            onset_game_id: None,
            onset_at: None,
            onset_low: None,
            onset_high: None,
            p_change: Some(0.9),
            rate_before: Some(0.1),
            rate_after: Some(0.3),
            evidence: json!({}),
            cause_hints: json!([]),
            detector_version: "tl-v1".into(),
            protocol: "enr-v1".into(),
        },
    )
    .await
    .unwrap();
    assert!(hist_id > 0);
    append_kpi_snapshot(&mut conn, &kpi_row(acc)).await.unwrap();
    drop(conn);

    // The remaining tables, by plain SQL.
    let build: Uuid = sqlx::query_scalar(
        "INSERT INTO reference_builds (kind, status, source_month, source_sha256, source_url, \
         params, games_scanned) VALUES ('opening_explorer', 'building', '2026-10', 'ab', \
         'https://database.lichess.org/x.pgn.zst', '{}', 0) RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let ref_game: Uuid = sqlx::query_scalar(
        "INSERT INTO reference_games (build_id, lichess_game_id, pgn, time_class, base_seconds, \
         increment_seconds, white_rating, black_rating, white_hash, black_hash, result, \
         termination, played_at) VALUES ($1, 'abcd1234', '1. e4 *', 'blitz', 180, 2, 1500, \
         1510, 'h1', 'h2', '*', 'Normal', $2) RETURNING id",
    )
    .bind(build)
    .bind(PLAYED_AT)
    .fetch_one(&pool)
    .await
    .unwrap();
    let statements = [
        "INSERT INTO move_features (game_id, ply, best_pv, refutation_pv, enrichment_protocol) \
         VALUES ($1, 1, '{e2e4}', '{}', 'enr-v1')",
        "INSERT INTO position_multipv (fen_key, protocol, lines) \
         VALUES ('k', 'sf19-n300k-mpv3-v1', '[]')",
        "INSERT INTO position_threats (fen_key, protocol, score_cp, pv) \
         VALUES ('k', 'sf19-n50k-null-v1', 30, '{e7e5}')",
        "INSERT INTO position_tb (fen_key, wdl) VALUES ('k', 2)",
        "INSERT INTO game_openings (game_id, account_id, time_class, color, fen_keys, ucis, sans, \
         line_kind) SELECT $1, account_id, 'blitz', 'white', '{a}', '{e2e4}', '{e4}', 'chosen' \
         FROM games WHERE id = $1",
        "INSERT INTO repertoire_nodes (account_id, time_class, color, fen_key, games, stats) \
         SELECT account_id, 'blitz', 'white', 'a', 1, '{}' FROM games WHERE id = $1",
        "INSERT INTO opening_lines (account_id, time_class, color, line_key, kind, games, verdict, \
         summary) SELECT account_id, 'blitz', 'white', 'white:italian', 'chosen', 1, 'keep', '{}' \
         FROM games WHERE id = $1",
        "INSERT INTO game_previews (game_id, protocol, white_accuracy, black_accuracy, moves) \
         VALUES ($1, 'sf19-n30k-preview-v1', 80, 70, '[]')",
    ];
    for sql in statements {
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(game)
            .execute(&pool)
            .await
            .unwrap();
    }
    let build_statements = [
        "INSERT INTO opening_explorer (build_id, fen_key, band, time_class, uci, white, draws, \
         black) VALUES ($1, 'a', 'B2', 'blitz', 'e2e4', 1, 0, 1)",
        "INSERT INTO band_reference (build_id, band, time_class, metric, median, p25, p40, p75, \
         opp_slope_per_100, n_players, n_games, low_sample) \
         VALUES ($1, 'B2', 'blitz', 'W1', 0.02, 0.01, 0.015, 0.03, 0.001, 40, 400, false)",
    ];
    for sql in build_statements {
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(build)
            .execute(&pool)
            .await
            .unwrap();
    }
    sqlx::query(
        "INSERT INTO reference_observations (build_id, reference_game_id, color, player_hash, \
         band, time_class, player_rating, opponent_rating, metrics) \
         VALUES ($1, $2, 'white', 'h1', 'B2', 'blitz', 1500, 1510, '{}')",
    )
    .bind(build)
    .bind(ref_game)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE linked_accounts SET analysis_scheduled_at = now() WHERE id = $1")
        .bind(acc)
        .execute(&pool)
        .await
        .unwrap();

    for table in [
        "move_features",
        "position_multipv",
        "position_threats",
        "position_tb",
        "game_metrics",
        "weakness_series_point",
        "weakness_estimates",
        "weakness_state_history",
        "kpi_snapshot",
        "reference_builds",
        "opening_explorer",
        "reference_games",
        "reference_observations",
        "band_reference",
        "game_openings",
        "repertoire_nodes",
        "opening_lines",
        "game_previews",
    ] {
        assert_eq!(count_all(&pool, table).await, 1, "{table}");
    }

    // The new enums carry exactly the documented labels.
    let labels: Vec<String> = sqlx::query_scalar(
        "SELECT e.enumlabel::text FROM pg_enum e JOIN pg_type t ON t.oid = e.enumtypid \
         WHERE t.typname = 'build_status' ORDER BY e.enumsortorder",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        labels,
        ["building", "active", "superseded", "failed", "unverified"]
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn upserts_and_replacements_keep_one_current_row(pool: PgPool) {
    let acc = make_account(&pool).await;
    let game = make_game(&pool, acc, "g1").await;
    let mut conn = pool.acquire().await.unwrap();
    let point = |key: &str, events: i32| SeriesPointRow {
        account_id: acc,
        time_class: "blitz".into(),
        series_key: key.into(),
        played_at: PLAYED_AT,
        events,
        opportunities: 4,
        epl: 0.0,
        protocol: "enr-v1".into(),
    };
    replace_series_points(&mut conn, game, &[point("W1", 1), point("W2", 0)])
        .await
        .unwrap();
    replace_series_points(&mut conn, game, &[point("W1", 2)])
        .await
        .unwrap();
    let rows: Vec<(String, i32)> =
        sqlx::query_as("SELECT series_key, events FROM weakness_series_point WHERE game_id = $1")
            .bind(game)
            .fetch_all(&mut *conn)
            .await
            .unwrap();
    assert_eq!(rows, [("W1".to_owned(), 2)]);

    let metrics = |v: i64| GameMetricsRow {
        game_id: game,
        account_id: acc,
        time_class: "blitz".into(),
        played_at: PLAYED_AT,
        protocol: "sf19-n300k-v1".into(),
        enrichment_protocol: None,
        difficulty_model: None,
        metrics: json!({ "version": v }),
    };
    insert_game_metrics(&mut conn, &metrics(0)).await.unwrap();
    insert_game_metrics(&mut conn, &metrics(1)).await.unwrap();
    let version: String =
        sqlx::query_scalar("SELECT metrics->>'version' FROM game_metrics WHERE game_id = $1")
            .bind(game)
            .fetch_one(&mut *conn)
            .await
            .unwrap();
    assert_eq!(version, "1");
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn kpi_snapshot_keeps_identical_rows(pool: PgPool) {
    let acc = make_account(&pool).await;
    let mut conn = pool.acquire().await.unwrap();
    let a = append_kpi_snapshot(&mut conn, &kpi_row(acc)).await.unwrap();
    let b = append_kpi_snapshot(&mut conn, &kpi_row(acc)).await.unwrap();
    assert_ne!(a, b);
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM kpi_snapshot WHERE account_id = $1",
            acc
        )
        .await,
        2
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn cascade_from_games(pool: PgPool) {
    let acc = make_account(&pool).await;
    let game = make_game(&pool, acc, "g1").await;
    let mut conn = pool.acquire().await.unwrap();
    insert_game_metrics(
        &mut conn,
        &GameMetricsRow {
            game_id: game,
            account_id: acc,
            time_class: "blitz".into(),
            played_at: PLAYED_AT,
            protocol: "sf19-n300k-v1".into(),
            enrichment_protocol: None,
            difficulty_model: None,
            metrics: json!({}),
        },
    )
    .await
    .unwrap();
    replace_series_points(
        &mut conn,
        game,
        &[SeriesPointRow {
            account_id: acc,
            time_class: "blitz".into(),
            series_key: "W1".into(),
            played_at: PLAYED_AT,
            events: 0,
            opportunities: 1,
            epl: 0.0,
            protocol: "enr-v1".into(),
        }],
    )
    .await
    .unwrap();
    drop(conn);
    for sql in [
        "INSERT INTO move_features (game_id, ply, enrichment_protocol) VALUES ($1, 1, 'enr-v1')",
        "INSERT INTO game_openings (game_id, account_id, time_class, color, fen_keys, ucis, sans) \
         SELECT $1, account_id, 'blitz', 'white', '{}', '{}', '{}' FROM games WHERE id = $1",
        "INSERT INTO game_previews (game_id, protocol, moves) \
         VALUES ($1, 'sf19-n30k-preview-v1', '[]')",
    ] {
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(game)
            .execute(&pool)
            .await
            .unwrap();
    }

    sqlx::query("DELETE FROM games WHERE id = $1")
        .bind(game)
        .execute(&pool)
        .await
        .unwrap();
    for table in [
        "move_features",
        "game_metrics",
        "weakness_series_point",
        "game_openings",
        "game_previews",
    ] {
        let sql = format!("SELECT count(*) FROM {table} WHERE game_id = $1");
        assert_eq!(count(&pool, &sql, game).await, 0, "{table}");
    }
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn record_stage_metrics_writes_stages_json(pool: PgPool) {
    let acc = make_account(&pool).await;
    let game = make_game(&pool, acc, "g1").await;
    let mut stages = BTreeMap::new();
    stages.insert("multipv_ms".to_owned(), 12.5);
    stages.insert("threat_ms".to_owned(), 3.0);
    record_stage_metrics(
        &pool,
        &StageMetrics {
            job_id: Some(42),
            kind: "enrich-game".into(),
            game_id: Some(game),
            account_id: Some(acc),
            total_ms: 20.0,
            stages,
        },
    )
    .await
    .unwrap();
    let (kind, job_id, total, multipv): (String, Option<i64>, f64, String) = sqlx::query_as(
        "SELECT kind, job_id, total_ms, stages->>'multipv_ms' FROM job_metrics \
         WHERE game_id = $1",
    )
    .bind(game)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(kind, "enrich-game");
    assert_eq!(job_id, Some(42));
    assert_eq!(total, 20.0);
    assert_eq!(multipv, "12.5");
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn opening_line_can_be_both_chosen_and_faced(pool: PgPool) {
    let acc = make_account(&pool).await;
    for kind in ["chosen", "faced"] {
        sqlx::query(
            "INSERT INTO opening_lines (account_id, time_class, color, line_key, kind, games, \
             verdict, summary) VALUES ($1, 'blitz', 'white', 'white:italian', $2::line_kind, 3, \
             NULL, '{}')",
        )
        .bind(acc)
        .bind(kind)
        .execute(&pool)
        .await
        .unwrap();
    }
    let kinds: Vec<String> = sqlx::query_scalar(
        "SELECT kind::text FROM opening_lines WHERE account_id = $1 ORDER BY kind",
    )
    .bind(acc)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(kinds, ["chosen", "faced"]);
    // The same side twice is still a key violation.
    let dup = sqlx::query(
        "INSERT INTO opening_lines (account_id, time_class, color, line_key, kind, games, summary) \
         VALUES ($1, 'blitz', 'white', 'white:italian', 'faced', 1, '{}')",
    )
    .bind(acc)
    .execute(&pool)
    .await;
    assert!(dup.is_err());
}

fn metrics_row(game_id: Uuid, account_id: Uuid, metrics: serde_json::Value) -> GameMetricsRow {
    GameMetricsRow {
        game_id,
        account_id,
        time_class: "blitz".into(),
        played_at: PLAYED_AT,
        protocol: "sf19-n300k-v1".into(),
        enrichment_protocol: None,
        difficulty_model: None,
        metrics,
    }
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn game_metrics_version_is_generated_and_missing_reads_as_0(pool: PgPool) {
    let acc = make_account(&pool).await;
    let mut games = Vec::new();
    for (id, metrics) in [
        ("v1", json!({ "version": 1 })),
        ("v0", json!({ "version": 0 })),
        ("none", json!({ "blunders": 2 })),
    ] {
        let game = make_game(&pool, acc, id).await;
        let mut conn = pool.acquire().await.unwrap();
        insert_game_metrics(&mut conn, &metrics_row(game, acc, metrics))
            .await
            .unwrap();
        games.push(game);
    }
    let versions: Vec<i32> = sqlx::query_scalar(
        "SELECT metrics_version FROM game_metrics WHERE game_id = ANY($1) \
         ORDER BY array_position($1, game_id)",
    )
    .bind(&games)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(versions, [1, 0, 0]);
    let stale: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM game_metrics WHERE metrics_version <> 1 AND account_id = $1",
    )
    .bind(acc)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(stale, 2);

    // An upsert with the current version refreshes the generated column.
    let mut conn = pool.acquire().await.unwrap();
    insert_game_metrics(
        &mut conn,
        &metrics_row(games[2], acc, json!({ "version": 1 })),
    )
    .await
    .unwrap();
    let v: i32 = sqlx::query_scalar("SELECT metrics_version FROM game_metrics WHERE game_id = $1")
        .bind(games[2])
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(v, 1);
}

async fn insert_build(pool: &PgPool, kind: &str, status: &str) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO reference_builds (kind, status, source_month, source_sha256) \
         VALUES ($1::reference_kind, $2::build_status, '2026-10', 'ab')",
    )
    .bind(kind)
    .bind(status)
    .execute(pool)
    .await
    .map(|_| ())
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn at_most_one_active_reference_build_per_kind(pool: PgPool) {
    insert_build(&pool, "opening_explorer", "active")
        .await
        .unwrap();
    insert_build(&pool, "band_reference", "active")
        .await
        .unwrap();
    insert_build(&pool, "opening_explorer", "superseded")
        .await
        .unwrap();
    insert_build(&pool, "opening_explorer", "superseded")
        .await
        .unwrap();
    assert!(
        insert_build(&pool, "opening_explorer", "active")
            .await
            .is_err()
    );
}

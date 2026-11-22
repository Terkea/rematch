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

/// One row in every new table (account-, game- and build-scoped). Returns (account, game).
async fn seed_every_table(pool: &PgPool) -> (Uuid, Uuid) {
    let acc = make_account(pool).await;
    let game = make_game(pool, acc, "g1").await;
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
    .fetch_one(pool)
    .await
    .unwrap();
    let ref_game: Uuid = sqlx::query_scalar(
        "INSERT INTO reference_games (build_id, lichess_game_id, pgn, time_class, base_seconds, \
         increment_seconds, white_rating, black_rating, white_hash, black_hash, result, \
         termination, played_at, white_selected, black_selected) VALUES ($1, 'abcd1234', \
         '1. e4 *', 'blitz', 180, 2, 1500, 1510, 'h1', 'h2', '*', 'Normal', $2, true, true) \
         RETURNING id",
    )
    .bind(build)
    .bind(PLAYED_AT)
    .fetch_one(pool)
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
            .execute(pool)
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
            .execute(pool)
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
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("UPDATE linked_accounts SET analysis_scheduled_at = now() WHERE id = $1")
        .bind(acc)
        .execute(pool)
        .await
        .unwrap();
    (acc, game)
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn one_row_per_new_table(pool: PgPool) {
    seed_every_table(&pool).await;
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
        "INSERT INTO move_features (game_id, ply, best_pv, refutation_pv, enrichment_protocol) \
         VALUES ($1, 1, '{}', '{}', 'enr-v1')",
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

/// Deleting a linked account removes its games and everything hanging off them or the account
/// (baseline `games` FK re-added `ON DELETE CASCADE` by the profile migration). `job_metrics`
/// has no FK and keeps its rows until the 30-day prune (ADR 0036).
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn cascade_from_account(pool: PgPool) {
    let (acc, game) = seed_every_table(&pool).await;
    let other = find_or_create_account(&pool, "lichess", "Hikaru")
        .await
        .unwrap()
        .id;
    let kept = make_game(&pool, other, "kept").await;
    for g in [game, kept] {
        sqlx::query(
            "INSERT INTO moves (game_id, ply, color, san, uci, fen_before, win_before, \
             win_after, delta_w, accuracy, phase, decided) \
             VALUES ($1, 1, 'white', 'e4', 'e2e4', 'f', 0.5, 0.5, 0, 100, 'opening', false)",
        )
        .bind(g)
        .execute(&pool)
        .await
        .unwrap();
    }
    let by_account = [
        "games",
        "game_metrics",
        "weakness_series_point",
        "weakness_estimates",
        "weakness_state_history",
        "kpi_snapshot",
        "game_openings",
        "repertoire_nodes",
        "opening_lines",
    ];
    let by_game = ["moves", "move_features", "game_previews"];
    let account_sql = |t: &str| format!("SELECT count(*) FROM {t} WHERE account_id = $1");
    let game_sql = |t: &str| format!("SELECT count(*) FROM {t} WHERE game_id = $1");
    for t in by_account {
        assert_eq!(count(&pool, &account_sql(t), acc).await, 1, "{t} before");
    }
    for t in by_game {
        assert_eq!(count(&pool, &game_sql(t), game).await, 1, "{t} before");
    }

    sqlx::query("DELETE FROM linked_accounts WHERE id = $1")
        .bind(acc)
        .execute(&pool)
        .await
        .unwrap();
    for t in by_account {
        assert_eq!(count(&pool, &account_sql(t), acc).await, 0, "{t}");
    }
    for t in by_game {
        assert_eq!(count(&pool, &game_sql(t), game).await, 0, "{t}");
    }
    // The other account is untouched.
    assert_eq!(count(&pool, &account_sql("games"), other).await, 1);
    assert_eq!(count(&pool, &game_sql("moves"), kept).await, 1);
}

/// T33 compares a new estimate or KPI with the stored one by `==`: an f64 must survive the
/// round trip exactly (the columns are `double precision`, not `real`).
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn estimate_and_kpi_floats_round_trip_exactly(pool: PgPool) {
    let acc = make_account(&pool).await;
    let mut conn = pool.acquire().await.unwrap();
    let x = 0.1_f64 + 0.2; // 0.30000000000000004: not representable as f32
    let e = WeaknessEstimateRow {
        account_id: acc,
        time_class: "blitz".into(),
        series_key: "W1".into(),
        k: x,
        n: 3.0 * x,
        n_unweighted: 3,
        k_unweighted: x / 3.0,
        alpha: 1.0 + x,
        beta: 2.0 + x,
        mean: x / 7.0,
        ci_low: x / 11.0,
        ci_high: x / 13.0,
        band_median: Some(x / 17.0),
        band_p40: Some(x / 19.0),
        confidence: x / 23.0,
        excess: Some(-x / 29.0),
        priority: x / 31.0,
        percentile: Some(x * 100.0),
        state: "active".into(),
        p_imp: Some(x / 37.0),
    };
    upsert_weakness_estimate(&mut conn, &e).await.unwrap();
    type Est = (f64, f64, f64, f64, f64, f64, f64, f64);
    let got: Est = sqlx::query_as(
        "SELECT k, n, k_unweighted, alpha, beta, mean, ci_low, ci_high \
         FROM weakness_estimates WHERE account_id = $1",
    )
    .bind(acc)
    .fetch_one(&mut *conn)
    .await
    .unwrap();
    assert!(
        got == (
            e.k,
            e.n,
            e.k_unweighted,
            e.alpha,
            e.beta,
            e.mean,
            e.ci_low,
            e.ci_high
        ),
        "{got:?}"
    );
    type Rest = (
        Option<f64>,
        Option<f64>,
        f64,
        Option<f64>,
        f64,
        Option<f64>,
        Option<f64>,
    );
    let rest: Rest = sqlx::query_as(
        "SELECT band_median, band_p40, confidence, excess, priority, percentile, p_imp \
         FROM weakness_estimates WHERE account_id = $1",
    )
    .bind(acc)
    .fetch_one(&mut *conn)
    .await
    .unwrap();
    assert!(
        rest == (
            e.band_median,
            e.band_p40,
            e.confidence,
            e.excess,
            e.priority,
            e.percentile,
            e.p_imp
        ),
        "{rest:?}"
    );

    let mut kpi = kpi_row(acc);
    kpi.value = Some(x);
    kpi.value_adjusted = Some(x / 3.0);
    kpi.ci_low = Some(x / 7.0);
    kpi.ci_high = Some(x / 11.0);
    let id = append_kpi_snapshot(&mut conn, &kpi).await.unwrap();
    let got: (Option<f64>, Option<f64>, Option<f64>, Option<f64>) = sqlx::query_as(
        "SELECT value, value_adjusted, ci_low, ci_high FROM kpi_snapshot WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&mut *conn)
    .await
    .unwrap();
    assert!(
        got == (kpi.value, kpi.value_adjusted, kpi.ci_low, kpi.ci_high),
        "{got:?}"
    );
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

// Plan guards ------------------------------------------------------------------------------
//
// Every index of the profile migration that serves a hot read is checked with EXPLAIN on a
// seeded database, in both the custom and the generic (cached prepared statement) plan: the
// read must use the named index, never a Seq Scan, and an ordered read must not Sort. The query
// shapes are the readers' (task in brackets); a reader with a different shape adds its own guard.

/// About 40k games over 100 accounts (400 each, 100 per time class) and proportionate rows in every profile table. The test
/// then runs `VACUUM ANALYZE` so index-only scans are costed with a set visibility map.
const PLAN_SEED_SQL: &str = r#"
INSERT INTO linked_accounts (platform, username, display_name)
  SELECT 'lichess', 'p' || a, 'p' || a FROM generate_series(1, 100) a;
-- ply_count holds the per-account game number g (1..400) for the inserts below.
INSERT INTO games (account_id, platform, platform_game_id, pgn, played_at, time_class, rated,
                   player_color, white_name, black_name, result, analysis_status,
                   analysis_started_at, ply_count)
  SELECT a.id, 'lichess', a.username || '-' || g, '1. e4 *', now() - g * interval '1 hour',
         (ARRAY['bullet', 'blitz', 'rapid', 'classical'])[1 + g % 4]::time_class, true,
         'white', 'w', 'b', '*',
         CASE WHEN g % 100 < 60 THEN 'imported' WHEN g % 100 < 80 THEN 'done'
              WHEN g % 100 < 90 THEN 'pending' ELSE 'analysing' END::analysis_status,
         CASE WHEN g % 100 = 99 THEN now() - interval '10 minutes'
              WHEN g % 100 >= 90 THEN now() - interval '5 hours'
              WHEN g % 100 >= 60 AND g % 100 < 80
                THEN now() - interval '3 days' - g * interval '1 minute'
         END,
         g
  FROM linked_accounts a, generate_series(1, 400) g;
INSERT INTO game_metrics (game_id, account_id, time_class, played_at, protocol, metrics)
  SELECT id, account_id, time_class, played_at, 'p',
         CASE WHEN ply_count % 100 = 0 THEN '{"version": 0}' ELSE '{"version": 1}' END::jsonb
  FROM games;
INSERT INTO weakness_series_point (account_id, time_class, series_key, game_id, played_at,
                                   events, opportunities, epl, protocol)
  SELECT account_id, time_class, 'w' || ((ply_count + k) % 10), id, played_at, 1, 3, 0.5, 'p'
  FROM games, generate_series(0, 2) k;
INSERT INTO weakness_state_history (account_id, time_class, series_key, kind, to_state,
                                    evidence, detector_version, protocol)
  SELECT a.id, tc::time_class, 'w' || k, 'transition', 'active', '{}', 'd1', 'p'
  FROM linked_accounts a, unnest(ARRAY['bullet', 'blitz', 'rapid', 'classical']) tc,
       generate_series(0, 9) k, generate_series(1, 5) r;
INSERT INTO kpi_snapshot (account_id, time_class, metric, window_kind, window_key, value, n,
                          protocol, computed_at)
  SELECT a.id, tc::time_class, 'm' || m, w::kpi_window, w || '-' || r, 0.5, 50, 'p',
         now() - r * interval '1 day'
  FROM linked_accounts a, unnest(ARRAY['bullet', 'blitz', 'rapid', 'classical']) tc,
       generate_series(1, 5) m, unnest(ARRAY['last_n', 'week']) w, generate_series(1, 5) r;
INSERT INTO reference_builds (kind, status, source_month, source_sha256)
  VALUES ('opening_explorer', 'active', '2026-10', 'a'),
         ('opening_explorer', 'superseded', '2026-09', 'b'),
         ('band_reference', 'active', '2026-10', 'c'),
         ('band_reference', 'superseded', '2026-09', 'd');
INSERT INTO opening_explorer (build_id, fen_key, band, time_class, uci, white, draws, black)
  SELECT b.id, 'f' || f, band::band, tc::time_class, uci, 5, 2, 3
  FROM reference_builds b, generate_series(1, 2000) f, unnest(ARRAY['B1', 'B2', 'B3']) band,
       unnest(ARRAY['blitz', 'rapid']) tc, unnest(ARRAY['e2e4', 'd2d4']) uci
  WHERE b.kind = 'opening_explorer';
INSERT INTO reference_games (build_id, lichess_game_id, pgn, time_class, white_hash, black_hash,
                             result, played_at, status, white_selected, black_selected)
  SELECT b.id, 'r' || i, '1. e4 *',
         (ARRAY['bullet', 'blitz', 'rapid', 'classical'])[1 + i % 4]::time_class,
         'hw' || i, 'hb' || i, '1-0', now(),
         CASE WHEN i % 20 = 0 THEN 'pending' ELSE 'done' END::analysis_status, true, true
  FROM reference_builds b, generate_series(1, 10000) i
  WHERE b.kind = 'band_reference';
INSERT INTO reference_observations (build_id, reference_game_id, color, player_hash, band,
                                    time_class, player_rating, opponent_rating, metrics)
  SELECT r.build_id, r.id, c::color, 'h', ('B' || (abs(hashtext(r.lichess_game_id)) % 6))::band,
         r.time_class, 1500, 1500, '{}'
  FROM reference_games r, unnest(ARRAY['white', 'black']) c;
INSERT INTO game_openings (game_id, account_id, time_class, color, fen_keys, ucis, sans,
                           line_key, line_kind)
  SELECT id, account_id, time_class,
         CASE WHEN ply_count % 2 = 0 THEN 'white' ELSE 'black' END::color, '{}', '{}', '{}',
         'l' || (ply_count % 50),
         CASE WHEN ply_count % 3 = 0 THEN 'faced' ELSE 'chosen' END::line_kind
  FROM games;
INSERT INTO job_metrics (kind, total_ms, created_at)
  SELECT (ARRAY['analyse-game', 'build-profile', 'enrich-game', 'reference-game'])[1 + i % 4],
         10, now() - i * interval '25 seconds'
  FROM generate_series(1, 100000) i;
"#;

/// One hot read: `PREPARE {name}({types}) AS {sql}`, run as `EXECUTE {name}({args})`.
struct HotRead {
    name: &'static str,
    types: &'static str,
    sql: &'static str,
    /// `{acc}`, `{game}`, `{build}` and `{refbuild}` are replaced by seeded ids.
    args: &'static str,
    /// The plan must use exactly this index.
    uses: &'static str,
    /// The read returns rows in index order: the plan must not sort.
    ordered: bool,
}

const HOT_READS: &[HotRead] = &[
    HotRead {
        // [T27] the fair-scheduling `served` CTE: every account, last hour.
        name: "fair_served",
        types: "float8",
        sql: "SELECT account_id, count(*) FROM games \
              WHERE analysis_started_at > now() - $1 * interval '1 millisecond' \
              GROUP BY account_id",
        args: "3600000",
        uses: "games_started_account_idx",
        ordered: false,
    },
    HotRead {
        // [T27/T30] the account's games in one status and time class (selection scan).
        name: "account_status_tc",
        types: "uuid, analysis_status, time_class",
        sql: "SELECT id, played_at FROM games \
              WHERE account_id = $1 AND analysis_status = $2 AND time_class = $3",
        args: "'{acc}', 'imported', 'blitz'",
        uses: "games_account_status_tc_idx",
        ordered: false,
    },
    HotRead {
        // [T33] the last N metrics of an account and time class.
        name: "metrics_window",
        types: "uuid, time_class, int8",
        sql: "SELECT metrics FROM game_metrics WHERE account_id = $1 AND time_class = $2 \
              ORDER BY played_at DESC LIMIT $3",
        args: "'{acc}', 'blitz', 50",
        uses: "game_metrics_account_tc_played_idx",
        ordered: true,
    },
    HotRead {
        // [T31/T33] the account's games whose metrics have a stale version.
        name: "stale_metrics",
        types: "uuid, int4",
        sql: "SELECT game_id FROM game_metrics WHERE account_id = $1 AND metrics_version <> $2",
        args: "'{acc}', 1",
        uses: "game_metrics_version_idx",
        ordered: false,
    },
    HotRead {
        // [T33] every series of an account and time class, in played order.
        name: "series_points",
        types: "uuid, time_class",
        sql: "SELECT series_key, played_at, events, opportunities, epl \
              FROM weakness_series_point WHERE account_id = $1 AND time_class = $2 \
              ORDER BY series_key, played_at",
        args: "'{acc}', 'blitz'",
        uses: "weakness_series_point_series_played_idx",
        // It reads the whole account and time class: a bitmap scan plus a small sort beats
        // index order there, so only the index is required.
        ordered: false,
    },
    HotRead {
        // `profile_rows::replace_series_points` (and the cascade from `games`).
        name: "series_replace",
        types: "uuid",
        sql: "DELETE FROM weakness_series_point WHERE game_id = $1",
        args: "'{game}'",
        uses: "weakness_series_point_game_idx",
        ordered: false,
    },
    HotRead {
        // [T33] the latest state row of a series (append only on change).
        name: "latest_state",
        types: "uuid, time_class, text",
        sql: "SELECT to_state FROM weakness_state_history \
              WHERE account_id = $1 AND time_class = $2 AND series_key = $3 \
              ORDER BY id DESC LIMIT 1",
        args: "'{acc}', 'blitz', 'w3'",
        uses: "weakness_state_history_series_idx",
        ordered: true,
    },
    HotRead {
        // [T33, report] the latest KPI snapshot of a metric window.
        name: "latest_kpi",
        types: "uuid, time_class, text, text",
        sql: "SELECT value, n FROM kpi_snapshot WHERE account_id = $1 AND time_class = $2 \
              AND metric = $3 AND window_key = $4 ORDER BY computed_at DESC LIMIT 1",
        args: "'{acc}', 'blitz', 'm2', 'week-3'",
        uses: "kpi_snapshot_lookup_idx",
        ordered: true,
    },
    HotRead {
        // [T17 load_explorer_index] a batch of positions in one band and time class.
        name: "explorer_batch",
        types: "uuid, band, time_class, text[]",
        sql: "SELECT fen_key, uci, white, draws, black FROM opening_explorer \
              WHERE build_id = $1 AND band = $2 AND time_class = $3 AND fen_key = ANY($4)",
        args: "'{build}', 'B2', 'blitz', ARRAY['f1', 'f17', 'f1999']",
        uses: "opening_explorer_pkey",
        ordered: false,
    },
    HotRead {
        // [T17 pending_reference_count] a build's pending reference games.
        name: "pending_refs",
        types: "uuid, analysis_status",
        sql: "SELECT count(*) FROM reference_games WHERE build_id = $1 AND status = $2",
        args: "'{refbuild}', 'pending'",
        uses: "reference_games_build_status_idx",
        ordered: false,
    },
    HotRead {
        // [T21/T32] one band-reference cell's observations.
        name: "observation_cell",
        types: "uuid, band, time_class",
        sql: "SELECT metrics FROM reference_observations \
              WHERE build_id = $1 AND band = $2 AND time_class = $3",
        args: "'{refbuild}', 'B2', 'blitz'",
        uses: "reference_observations_cell_idx",
        ordered: false,
    },
    HotRead {
        // [T18 rebuild_repertoire] an account's openings per time class and colour.
        name: "account_openings",
        types: "uuid, time_class, color",
        sql: "SELECT game_id, line_key FROM game_openings \
              WHERE account_id = $1 AND time_class = $2 AND color = $3",
        args: "'{acc}', 'blitz', 'white'",
        uses: "game_openings_account_idx",
        ordered: false,
    },
    HotRead {
        // [T18/T31] the games of one opening line and side.
        name: "line_games",
        types: "uuid, time_class, text, line_kind",
        sql: "SELECT game_id FROM game_openings WHERE account_id = $1 AND time_class = $2 \
              AND line_key = $3 AND line_kind = $4",
        args: "'{acc}', 'blitz', 'l7', 'chosen'",
        uses: "game_openings_line_idx",
        ordered: false,
    },
    HotRead {
        // [T30 account_eta] analyse-game throughput over the last 10 minutes.
        name: "eta_rate",
        types: "text, float8",
        sql: "SELECT count(*) FROM job_metrics \
              WHERE kind = $1 AND created_at > now() - $2 * interval '1 millisecond'",
        args: "'analyse-game', 600000",
        uses: "job_metrics_kind_created_idx",
        ordered: false,
    },
];

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn hot_reads_use_their_indexes(pool: PgPool) {
    sqlx::raw_sql(PLAN_SEED_SQL).execute(&pool).await.unwrap();
    sqlx::raw_sql("VACUUM ANALYZE")
        .execute(&pool)
        .await
        .unwrap();
    // Trimmed by review: no reader needs them (the explorer PK serves every explorer lookup).
    let dropped: Vec<String> = sqlx::query_scalar(
        "SELECT indexname::text FROM pg_indexes WHERE indexname = ANY($1) ORDER BY 1",
    )
    .bind([
        "games_account_started_idx",
        "opening_explorer_lookup_idx",
        "reference_builds_kind_status_idx",
    ])
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(dropped.is_empty(), "{dropped:?}");
    let (acc, game): (Uuid, Uuid) =
        sqlx::query_as("SELECT account_id, id FROM games ORDER BY platform_game_id LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();
    let build_of = |kind: &'static str| {
        let pool = pool.clone();
        async move {
            sqlx::query_scalar::<_, Uuid>(
                "SELECT id FROM reference_builds \
                 WHERE kind = $1::reference_kind AND status = 'active'",
            )
            .bind(kind)
            .fetch_one(&pool)
            .await
            .unwrap()
        }
    };
    let build = build_of("opening_explorer").await;
    let refbuild = build_of("band_reference").await;
    let mut conn = pool.acquire().await.unwrap();
    let mut failures = Vec::new();
    for r in HOT_READS {
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
            "PREPARE {}({}) AS {}",
            r.name, r.types, r.sql
        )))
        .execute(&mut *conn)
        .await
        .unwrap();
        let args = r
            .args
            .replace("{acc}", &acc.to_string())
            .replace("{game}", &game.to_string())
            .replace("{refbuild}", &refbuild.to_string())
            .replace("{build}", &build.to_string());
        for mode in ["force_custom_plan", "force_generic_plan"] {
            sqlx::query("SELECT set_config('plan_cache_mode', $1, false)")
                .bind(mode)
                .execute(&mut *conn)
                .await
                .unwrap();
            let plan: Vec<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
                "EXPLAIN EXECUTE {}({args})",
                r.name
            )))
            .fetch_all(&mut *conn)
            .await
            .unwrap();
            let plan = plan.join("\n");
            let uses = plan.contains(&format!(" {} ", r.uses));
            let sorts = r.ordered && plan.contains("Sort");
            if !uses || plan.contains("Seq Scan") || sorts {
                failures.push(format!("{} {mode}:\n{plan}", r.name));
            }
        }
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!("DEALLOCATE {}", r.name)))
            .execute(&mut *conn)
            .await
            .unwrap();
    }
    sqlx::raw_sql("RESET plan_cache_mode")
        .execute(&mut *conn)
        .await
        .unwrap();
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

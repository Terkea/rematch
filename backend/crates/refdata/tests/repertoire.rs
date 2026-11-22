//! R2 Task 18: `refdata::repertoire::rebuild_repertoire` (TS M2 Task 13's DB case: 12 imported
//! games, rebuilt twice), plus the version gate on `game_metrics`, the explorer band fallback,
//! R-4 band resolution and the parity of the two `in_theory` copies.

use std::collections::HashMap;

use chess_core::TimeClass;
use chess_core::board::Position;
use chess_core::fen::fen_key;
use db::profile_rows::{GameMetricsRow, insert_game_metrics};
use profile_core::openings::lines::BandSource;
use profile_core::openings::repertoire as pure;
use profile_core::types::{Band, PLAYER_GAME_METRICS_VERSION, PlayerGameMetrics};
use refdata::builds::{BuildKind, NewBuild, activate_build, create_build, set_source_sha};
use refdata::explorer::{self, ExplorerCounts, ExplorerKey, flush_explorer};
use refdata::repertoire::{explorer_band_order, player_band, rebuild_repertoire};
use serde_json::{Value, json};
use sqlx::PgPool;
use testkit::{GameSeed, insert_account, insert_game};
use time::macros::datetime;
use uuid::Uuid;

const ITALIAN_PGN: &str = "1. e4 e5 2. Nf3 Nc6 3. Bc4 1-0";
const CARO_PGN: &str = "1. e4 c6 2. d4 d5 3. e5 Bf5 1-0";

async fn seed(
    pool: &PgPool,
    account: Uuid,
    pgn: &str,
    color: &'static str,
    status: &'static str,
) -> Uuid {
    insert_game(
        pool,
        account,
        GameSeed {
            pgn: pgn.to_owned(),
            player_color: color,
            status,
            ..GameSeed::default()
        },
    )
    .await
}

async fn count(pool: &PgPool, table: &str, account: Uuid) -> i64 {
    let sql = match table {
        "game_openings" => "SELECT count(*) FROM game_openings WHERE account_id = $1",
        "repertoire_nodes" => "SELECT count(*) FROM repertoire_nodes WHERE account_id = $1",
        "opening_lines" => "SELECT count(*) FROM opening_lines WHERE account_id = $1",
        _ => panic!("unknown table {table}"),
    };
    sqlx::query_scalar(sql)
        .bind(account)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// (line_key, kind, games, verdict, summary).
type LineRow = (String, String, i32, Option<String>, Value);

async fn lines(pool: &PgPool, account: Uuid) -> Vec<LineRow> {
    sqlx::query_as(
        "SELECT line_key, kind::text, games, verdict::text, summary FROM opening_lines \
         WHERE account_id = $1 AND time_class = 'blitz' ORDER BY line_key, kind",
    )
    .bind(account)
    .fetch_all(pool)
    .await
    .unwrap()
}

fn key_after(sans: &[&str]) -> String {
    let mut p = Position::start();
    for s in sans {
        p.play_san(s).unwrap();
    }
    fen_key(&p.fen())
}

/// One explorer row: position, band, move and (white, draws, black).
type Row<'a> = (String, Band, &'a str, (i64, i64, i64));

async fn explorer_build(pool: &PgPool, rows: &[Row<'_>]) {
    let id = create_build(
        pool,
        &NewBuild {
            kind: BuildKind::OpeningExplorer,
            source_month: "2026-10".into(),
            source_url: None,
            params: json!({}),
            protocol: None,
            enrichment_protocol: None,
            difficulty_model: None,
        },
    )
    .await
    .unwrap();
    let rows: Vec<_> = rows
        .iter()
        .map(|(fen, band, uci, (white, draws, black))| {
            (
                ExplorerKey {
                    fen_key: fen.clone(),
                    band: *band,
                    time_class: TimeClass::Blitz,
                    uci: (*uci).to_owned(),
                },
                ExplorerCounts {
                    white: *white,
                    draws: *draws,
                    black: *black,
                },
            )
        })
        .collect();
    flush_explorer(pool, id, &rows).await.unwrap();
    set_source_sha(pool, id, &"ab".repeat(32), 1).await.unwrap();
    activate_build(pool, id).await.unwrap();
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn rebuild_twice_gives_the_same_rows(pool: PgPool) {
    let account = insert_account(&pool, "lichess", "hero").await;
    let statuses = [
        "imported", "imported", "imported", "pending", "done", "failed",
    ];
    for i in 0..12 {
        seed(
            &pool,
            account,
            ITALIAN_PGN,
            "white",
            statuses[i % statuses.len()],
        )
        .await;
    }
    // A rapid line and another account are never touched.
    let other = insert_account(&pool, "lichess", "other").await;
    seed(&pool, other, ITALIAN_PGN, "white", "imported").await;
    rebuild_repertoire(&pool, other, TimeClass::Blitz)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO opening_lines (account_id, time_class, color, line_key, kind, games, summary) \
         VALUES ($1, 'rapid', 'white', 'white:x', 'chosen', 1, '{}')",
    )
    .bind(account)
    .execute(&pool)
    .await
    .unwrap();

    let first = rebuild_repertoire(&pool, account, TimeClass::Blitz)
        .await
        .unwrap();
    let counts1 = (
        count(&pool, "game_openings", account).await,
        count(&pool, "repertoire_nodes", account).await,
        count(&pool, "opening_lines", account).await,
    );
    let second = rebuild_repertoire(&pool, account, TimeClass::Blitz)
        .await
        .unwrap();
    let counts2 = (
        count(&pool, "game_openings", account).await,
        count(&pool, "repertoire_nodes", account).await,
        count(&pool, "opening_lines", account).await,
    );
    assert_eq!((first, second), (1, 1));
    assert_eq!(counts1, counts2);
    // 12 games of every status; 6 positions (start + 5 plies); 1 blitz line + the rapid row.
    assert_eq!(counts1, (12, 6, 2));
    assert_eq!(count(&pool, "opening_lines", other).await, 1);

    let ls = lines(&pool, account).await;
    assert_eq!(ls.len(), 1);
    let (key, kind, games, verdict, summary) = &ls[0];
    assert_eq!(
        (key.as_str(), kind.as_str(), *games),
        ("white:italian-game", "chosen", 12)
    );
    assert_eq!(verdict.as_deref(), Some("keep"));
    assert_eq!(summary["comparison"]["playerBand"], "B2");
    assert_eq!(summary["comparison"]["bandSource"], "rating");
    assert_eq!(summary["comparison"]["explorerBand"], Value::Null);
    assert_eq!(summary["deviationPly"], Value::Null);

    let node: (i32, Value) = sqlx::query_as(
        "SELECT games, stats FROM repertoire_nodes WHERE account_id = $1 AND fen_key = $2",
    )
    .bind(account)
    .bind(key_after(&["e4", "e5", "Nf3", "Nc6", "Bc4"]))
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(node.0, 12);
    assert_eq!(node.1["score"], 1.0);
    assert_eq!(node.1["perfVsExpected"], 0.5);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn chosen_and_faced_rows_stay_apart(pool: PgPool) {
    let account = insert_account(&pool, "lichess", "hero").await;
    for _ in 0..2 {
        seed(&pool, account, ITALIAN_PGN, "white", "imported").await;
        seed(&pool, account, ITALIAN_PGN, "black", "imported").await;
    }
    // Old Indian: first named on White's 2.c4, owned by Black.
    seed(&pool, account, "1. d4 d6 2. c4 1-0", "black", "imported").await;
    assert_eq!(
        rebuild_repertoire(&pool, account, TimeClass::Blitz)
            .await
            .unwrap(),
        3
    );
    let got: Vec<(String, String, i32)> = lines(&pool, account)
        .await
        .into_iter()
        .map(|(k, kind, g, v, _)| {
            assert_eq!(v, None, "under 8 games: no verdict");
            (k, kind, g)
        })
        .collect();
    assert_eq!(
        got,
        vec![
            ("black:italian-game".into(), "faced".into(), 2),
            ("black:old-indian-defense".into(), "chosen".into(), 1),
            ("white:italian-game".into(), "chosen".into(), 2),
        ]
    );
    let entry: (Option<String>, Option<i32>) = sqlx::query_as(
        "SELECT o.line_kind::text, o.line_entry_ply FROM game_openings o \
         WHERE o.line_key = 'black:old-indian-defense'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(entry, (Some("chosen".into()), Some(3)));
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn unparsable_pgn_gets_an_empty_row_and_no_evidence(pool: PgPool) {
    let account = insert_account(&pool, "lichess", "hero").await;
    let bad = seed(
        &pool,
        account,
        "1. e4 Ke7?? 2. Qxe8 1-0",
        "white",
        "imported",
    )
    .await;
    seed(&pool, account, ITALIAN_PGN, "white", "imported").await;
    rebuild_repertoire(&pool, account, TimeClass::Blitz)
        .await
        .unwrap();
    let row: (Vec<String>, Option<String>) =
        sqlx::query_as("SELECT fen_keys, line_key FROM game_openings WHERE game_id = $1")
            .bind(bad)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(row, (Vec::new(), None));
    let root: i32 = sqlx::query_scalar(
        "SELECT games FROM repertoire_nodes WHERE account_id = $1 AND fen_key = $2",
    )
    .bind(account)
    .bind(key_after(&[]))
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(root, 1);
}

/// 8 black Caro-Kann games (all lost, expected 0.5) whose 3...Bf5 lost ΔW 12, with metrics of
/// `version`.
async fn caro_with_metrics(pool: &PgPool, account: Uuid, version: u32) {
    for _ in 0..8 {
        let id = seed(pool, account, CARO_PGN, "black", "done").await;
        let metrics = PlayerGameMetrics {
            version,
            ..PlayerGameMetrics::default()
        };
        let mut conn = pool.acquire().await.unwrap();
        insert_game_metrics(
            &mut conn,
            &GameMetricsRow {
                game_id: id,
                account_id: account,
                time_class: "blitz".into(),
                played_at: datetime!(2026-01-01 0:00 UTC),
                protocol: "sf19-n300k-v1".into(),
                enrichment_protocol: None,
                difficulty_model: None,
                metrics: serde_json::to_value(&metrics).unwrap(),
            },
        )
        .await
        .unwrap();
        let mut p = Position::start();
        for (i, san) in ["e4", "c6", "d4", "d5", "e5", "Bf5"].iter().enumerate() {
            let before = p.fen();
            let color = if i % 2 == 0 { "white" } else { "black" };
            let played = p.play_san(san).unwrap();
            let dw = if i == 5 { 12.0 } else { 0.0 };
            sqlx::query(
                "INSERT INTO moves (game_id, ply, color, san, uci, fen_before, win_before, \
                 win_after, delta_w, accuracy, phase, decided) \
                 VALUES ($1, $2, $3::color, $4, $5, $6, 50, 50, $7, 90, 'opening', false)",
            )
            .bind(id)
            .bind(i32::try_from(i).unwrap() + 1)
            .bind(color)
            .bind(&played.san)
            .bind(&played.uci)
            .bind(&before)
            .bind(dw)
            .execute(pool)
            .await
            .unwrap();
        }
    }
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn current_metrics_give_a_fix(pool: PgPool) {
    let account = insert_account(&pool, "lichess", "hero").await;
    caro_with_metrics(&pool, account, PLAYER_GAME_METRICS_VERSION).await;
    rebuild_repertoire(&pool, account, TimeClass::Blitz)
        .await
        .unwrap();
    let ls = lines(&pool, account).await;
    let (key, kind, _, verdict, summary) = &ls[0];
    assert_eq!(key, "black:caro-kann-defense-advance-variation");
    assert_eq!(kind, "chosen");
    assert_eq!(verdict.as_deref(), Some("fix"));
    assert_eq!(summary["fixSpot"]["games"], 8);
    assert_eq!(summary["fixSpot"]["uci"], "c8f5");
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn stale_metrics_are_not_evidence(pool: PgPool) {
    let account = insert_account(&pool, "lichess", "hero").await;
    caro_with_metrics(&pool, account, PLAYER_GAME_METRICS_VERSION + 1).await;
    rebuild_repertoire(&pool, account, TimeClass::Blitz)
        .await
        .unwrap();
    let ls = lines(&pool, account).await;
    let (_, _, _, verdict, summary) = &ls[0];
    // The losses still count (they come from the games), but the ΔW moves do not.
    assert_eq!(verdict.as_deref(), Some("learn"));
    assert_eq!(summary["fixSpot"], Value::Null);
    assert_eq!(summary["errorSpot"], Value::Null);
    assert_eq!(summary["evalGames"], 0);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn explorer_fallback_band_is_recorded_and_book_exits_refreshed(pool: PgPool) {
    let account = insert_account(&pool, "lichess", "hero").await;
    for _ in 0..8 {
        seed(&pool, account, ITALIAN_PGN, "white", "imported").await;
    }
    // The player is B3 (1800); the explorer only has B2 data for the start position.
    sqlx::query("UPDATE games SET white_rating = 1800 WHERE account_id = $1")
        .bind(account)
        .execute(&pool)
        .await
        .unwrap();
    explorer_build(
        &pool,
        &[
            (key_after(&[]), Band::B2, "e2e4", (30, 10, 20)),
            (key_after(&[]), Band::B5, "d2d4", (300, 100, 200)),
        ],
    )
    .await;
    rebuild_repertoire(&pool, account, TimeClass::Blitz)
        .await
        .unwrap();
    let ls = lines(&pool, account).await;
    let c = &ls[0].4["comparison"];
    assert_eq!(c["playerBand"], "B3");
    assert_eq!(c["explorerBand"], "B2");
    assert_eq!(c["explorerFallback"], true);
    // 1.e4 is theory (60 games); the position after it is unknown, so theory ran out at ply 2
    // on Black's move: a book exit, but no deviation by the player.
    let exits: Vec<(Option<i32>, Option<i32>)> = sqlx::query_as(
        "SELECT book_exit_ply, player_deviation_ply FROM game_openings WHERE account_id = $1",
    )
    .bind(account)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(exits, vec![(Some(2), None); 8]);
    assert_eq!(ls[0].4["deviationPly"], Value::Null);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn explorer_more_than_two_bands_away_is_not_used(pool: PgPool) {
    let account = insert_account(&pool, "lichess", "hero").await;
    seed(&pool, account, ITALIAN_PGN, "white", "imported").await;
    // B2 player, explorer data only in B5.
    explorer_build(
        &pool,
        &[(key_after(&[]), Band::B5, "e2e4", (300, 100, 200))],
    )
    .await;
    rebuild_repertoire(&pool, account, TimeClass::Blitz)
        .await
        .unwrap();
    let ls = lines(&pool, account).await;
    let c = &ls[0].4["comparison"];
    assert_eq!(c["explorerBand"], Value::Null);
    assert_eq!(c["explorerFallback"], false);
    let exit: Option<i32> =
        sqlx::query_scalar("SELECT book_exit_ply FROM game_openings WHERE account_id = $1")
            .bind(account)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(exit, None, "no theory evidence: no book exit claim");
}

#[test]
fn explorer_band_order_is_nearest_first_lower_on_ties() {
    assert_eq!(
        explorer_band_order(Band::B3),
        vec![Band::B3, Band::B2, Band::B4, Band::B1, Band::B5]
    );
    assert_eq!(
        explorer_band_order(Band::B0),
        vec![Band::B0, Band::B1, Band::B2]
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn band_from_rating_then_opponents_then_b1(pool: PgPool) {
    let account = insert_account(&pool, "lichess", "hero").await;
    assert_eq!(
        player_band(&pool, account, TimeClass::Blitz).await.unwrap(),
        (Band::B1, BandSource::Default)
    );
    let g = seed(&pool, account, ITALIAN_PGN, "black", "imported").await;
    // Own (black) rating 2100 → B4.
    sqlx::query("UPDATE games SET black_rating = 2100, white_rating = 1000 WHERE id = $1")
        .bind(g)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        player_band(&pool, account, TimeClass::Blitz).await.unwrap(),
        (Band::B4, BandSource::Rating)
    );
    sqlx::query("UPDATE games SET black_rating = NULL WHERE id = $1")
        .bind(g)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        player_band(&pool, account, TimeClass::Blitz).await.unwrap(),
        (Band::B1, BandSource::OpponentMean)
    );
}

#[test]
fn both_in_theory_copies_agree() {
    let cases: [&[(&str, i64)]; 5] = [
        &[("a", 45), ("b", 5)],
        &[("a", 96), ("b", 4)],
        &[("a", 95), ("b", 5)],
        &[("a", 44), ("b", 5)],
        &[],
    ];
    for moves in cases {
        let stored: HashMap<String, Vec<explorer::ExplorerMove>> = HashMap::from([(
            "p".to_owned(),
            moves
                .iter()
                .map(|(u, n)| explorer::ExplorerMove {
                    uci: (*u).into(),
                    white: *n,
                    draws: 0,
                    black: 0,
                    total: *n,
                })
                .collect(),
        )]);
        let mirrored: pure::ExplorerIndex = HashMap::from([(
            "p".to_owned(),
            moves
                .iter()
                .map(|(u, n)| pure::ExplorerMove {
                    uci: (*u).into(),
                    white: *n,
                    draws: 0,
                    black: 0,
                    total: *n,
                })
                .collect(),
        )]);
        for uci in ["a", "b", "c"] {
            assert_eq!(
                explorer::in_theory(&stored, "p", uci),
                pure::in_theory(&mirrored, "p", uci),
                "{moves:?} {uci}"
            );
        }
    }
    assert_eq!(explorer::THEORY_MIN_TOTAL, pure::THEORY_MIN_TOTAL);
    assert_eq!(explorer::THEORY_MIN_SHARE_PCT, pure::THEORY_MIN_SHARE_PCT);
}

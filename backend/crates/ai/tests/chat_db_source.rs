//! Task 25: `DbCoachDataSource`, the chat's view of ONE account's data in Postgres, and
//! `latest_coaching_input`. Every query is scoped to the account; another account's rows are
//! simply not found.

#[path = "support/chat.rs"]
mod support;

use std::sync::Arc;

use ai::chat::anchor::ChatAnchor;
use ai::chat::data_source::{CoachDataSource, ExamplesQuery, GameResult, GamesQuery};
use ai::chat::db_source::{ChatScope, DbCoachDataSource, latest_coaching_input};
use ai::input::types::{CoachingInput, TimeClass};
use ai::output::codes::SectionKey;
use serde_json::json;
use sqlx::PgPool;
use support::{account, seed_exemplar_positions, seed_game, seed_move, store_input};
use uuid::Uuid;

const START: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
const AFTER_E4: &str = "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1";

fn ds(pool: &PgPool, account_id: Uuid, input: CoachingInput) -> DbCoachDataSource {
    DbCoachDataSource::new(
        pool.clone(),
        ChatScope {
            account_id,
            time_class: TimeClass::Rapid,
            input: Arc::new(input),
        },
    )
}

/// A report of `account_id` with a stored `overview` section; returns the report id.
async fn seed_report(pool: &PgPool, account_id: Uuid) -> Uuid {
    let run: Uuid = sqlx::query_scalar(
        "INSERT INTO report_runs (account_id, time_class, trigger, plan, prompt_version, \
         is_first_report) VALUES ($1, 'rapid', 'manual', 'free', 'report@1', true) RETURNING id",
    )
    .bind(account_id)
    .fetch_one(pool)
    .await
    .unwrap();
    let report: Uuid = sqlx::query_scalar(
        "INSERT INTO reports (account_id, time_class, version, report_run_id, schema_version, \
         computed, status) VALUES ($1, 'rapid', 1, $2, 'v1', '{}', 'complete') RETURNING id",
    )
    .bind(account_id)
    .bind(run)
    .fetch_one(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO report_sections (report_id, section, status, content, attempts, model) \
         VALUES ($1, 'overview', 'ready', $2, 1, 'claude-sonnet-5-5')",
    )
    .bind(report)
    .bind(json!({"headline": {"text": "Your overview"}}))
    .execute(pool)
    .await
    .unwrap();
    report
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn data_source_never_returns_another_accounts_rows(pool: PgPool) {
    let a = account(&pool, "alice").await;
    let b = account(&pool, "bob").await;
    let input_a = store_input(&pool, a).await;
    let gb = seed_game(&pool, b, "li-b-0").await;
    seed_move(&pool, gb, 1, START, "e4", "e2e4", Some("e2e4")).await;
    sqlx::query(
        "INSERT INTO position_evals (fen_key, protocol, score_cp, best_uci, pv, source) \
         VALUES ($1, $2, 30, 'e2e4', ARRAY['e2e4','e7e5'], 'stockfish')",
    )
    .bind(chess_core::fen::fen_key(START))
    .bind(engine::ANALYSIS_PROTOCOL)
    .execute(&pool)
    .await
    .unwrap();
    let report_b = seed_report(&pool, b).await;

    let src = ds(&pool, a, input_a);
    assert_eq!(src.get_position("li-b-0", 1).await.unwrap(), None);
    let games = src
        .get_games(GamesQuery {
            limit: 20,
            ..GamesQuery::default()
        })
        .await
        .unwrap();
    assert!(games.is_empty(), "{games:?}");
    let anchors = [
        ChatAnchor::Move {
            game_id: "li-b-0".into(),
            ply: 1,
        },
        ChatAnchor::Position {
            game_id: "li-b-0".into(),
            ply: 1,
        },
        ChatAnchor::Game {
            game_id: "li-b-0".into(),
        },
        ChatAnchor::ReportSection {
            report_id: report_b,
            section: SectionKey::Overview,
        },
        ChatAnchor::Weakness {
            weakness_id: "W12".into(),
        },
        ChatAnchor::OpeningLine {
            line_key: "w-not-mine".into(),
        },
    ];
    for anchor in &anchors {
        assert_eq!(
            src.resolve_anchor(anchor).await.unwrap(),
            None,
            "{anchor:?}"
        );
    }

    // B sees its own game and position.
    let input_b: CoachingInput =
        serde_json::from_value(ai::testing::make_input(json!({"accountId": b}))).unwrap();
    let src_b = ds(&pool, b, input_b);
    let p = src_b.get_position("li-b-0", 1).await.unwrap().unwrap();
    assert_eq!(p.fen, START);
    assert_eq!(p.fen_after_played, AFTER_E4);
    assert_eq!(p.pv, vec!["e2e4".to_owned(), "e7e5".to_owned()]);
    assert_eq!(p.best.as_ref().map(|m| m.san.as_str()), Some("e4"));
    assert_eq!(
        src_b
            .get_games(GamesQuery {
                limit: 5,
                ..GamesQuery::default()
            })
            .await
            .unwrap()
            .len(),
        1
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn resolve_anchor_records_carry_the_anchor_ids(pool: PgPool) {
    let a = account(&pool, "alice").await;
    let input = store_input(&pool, a).await;
    seed_exemplar_positions(&pool, a, &input).await;
    let src = ds(&pool, a, input);

    let mv = src
        .resolve_anchor(&ChatAnchor::Move {
            game_id: "li-w1-0".into(),
            ply: 5,
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        (mv["gameId"].clone(), mv["ply"].clone()),
        (json!("li-w1-0"), json!(5))
    );
    assert!(mv["fen"].is_string() && mv["fenAfterPlayed"].is_string());
    assert_eq!(mv["tags"], json!(["hung_piece"]));

    let g = src
        .resolve_anchor(&ChatAnchor::Game {
            game_id: "li-w1-0".into(),
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(g["gameId"], json!("li-w1-0"));

    let w = src
        .resolve_anchor(&ChatAnchor::Weakness {
            weakness_id: "W1".into(),
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(w["id"], json!("W1"));

    let o = src
        .resolve_anchor(&ChatAnchor::OpeningLine {
            line_key: "w-italian".into(),
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(o["lineKey"], json!("w-italian"));
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn report_section_anchor_resolves_to_its_stored_content(pool: PgPool) {
    let a = account(&pool, "alice").await;
    let input = store_input(&pool, a).await;
    let report = seed_report(&pool, a).await;
    let src = ds(&pool, a, input);
    let r = src
        .resolve_anchor(&ChatAnchor::ReportSection {
            report_id: report,
            section: SectionKey::Overview,
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(r["reportId"], json!(report.to_string()));
    assert_eq!(r["section"], json!("overview"));
    assert_eq!(r["status"], json!("ready"));
    assert_eq!(r["content"], json!({"headline": {"text": "Your overview"}}));
    // A section that was never stored is not found.
    assert_eq!(
        src.resolve_anchor(&ChatAnchor::ReportSection {
            report_id: report,
            section: SectionKey::Openings,
        })
        .await
        .unwrap(),
        None
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn find_examples_returns_other_exemplars_for_a_weakness(pool: PgPool) {
    let a = account(&pool, "alice").await;
    let input = store_input(&pool, a).await;
    seed_exemplar_positions(&pool, a, &input).await;
    let src = ds(&pool, a, input);

    let w1 = src
        .find_examples(ExamplesQuery {
            weakness_id: Some("W1".into()),
            limit: 10,
            ..ExamplesQuery::default()
        })
        .await
        .unwrap();
    let ids: Vec<(String, u32)> = w1.iter().map(|p| (p.game_id.clone(), p.ply)).collect();
    assert_eq!(
        ids,
        vec![
            ("li-w1-0".to_owned(), 5),
            ("li-w1-1".to_owned(), 9),
            ("li-w1-2".to_owned(), 13)
        ]
    );
    assert!(w1.iter().all(|p| p.tags == vec!["hung_piece".to_owned()]));

    // Capped by the limit.
    let two = src
        .find_examples(ExamplesQuery {
            weakness_id: Some("W1".into()),
            limit: 2,
            ..ExamplesQuery::default()
        })
        .await
        .unwrap();
    assert_eq!(two.len(), 2);

    // A motif searches every exemplar of the input, deduplicated.
    let tactics = src
        .find_examples(ExamplesQuery {
            motif: Some("tactics".into()),
            limit: 10,
            ..ExamplesQuery::default()
        })
        .await
        .unwrap();
    let ids: Vec<&str> = tactics.iter().map(|p| p.game_id.as_str()).collect();
    assert_eq!(
        ids,
        vec!["li-s2", "li-c1-0", "li-c1-1", "li-c1-2", "li-c1-3"]
    );

    let line = src
        .find_examples(ExamplesQuery {
            line_key: Some("b-sicilian-najdorf".into()),
            limit: 10,
            ..ExamplesQuery::default()
        })
        .await
        .unwrap();
    assert_eq!(line.len(), 1);
    assert_eq!(line[0].game_id, "li-op1");

    let none = src
        .find_examples(ExamplesQuery {
            weakness_id: Some("W12".into()),
            limit: 10,
            ..ExamplesQuery::default()
        })
        .await
        .unwrap();
    assert!(none.is_empty());
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn get_games_orders_by_played_at_and_clamps_the_limit(pool: PgPool) {
    let a = account(&pool, "alice").await;
    let input = store_input(&pool, a).await;
    for i in 0..25 {
        let g = seed_game(&pool, a, &format!("li-g-{i}")).await;
        sqlx::query(
            "UPDATE games SET played_at = '2026-01-01T00:00:00Z'::timestamptz + make_interval(days => $2), \
             opening_name = $3 WHERE id = $1",
        )
        .bind(g)
        .bind(i)
        .bind(if i == 3 { "Sicilian 100%_Main" } else { "Italian Game" })
        .execute(&pool)
        .await
        .unwrap();
    }
    let src = ds(&pool, a, input);
    let games = src
        .get_games(GamesQuery {
            limit: 50,
            ..GamesQuery::default()
        })
        .await
        .unwrap();
    assert_eq!(games.len(), 20);
    assert_eq!(games[0].game_id, "li-g-24");
    assert!(games.windows(2).all(|w| w[0].played_at > w[1].played_at));
    assert_eq!(games[0].result, GameResult::Win);

    // `%` and `_` are literal in openingContains (T22 hand-off).
    let like = |s: &str| GamesQuery {
        opening_contains: Some(s.to_owned()),
        limit: 20,
        ..GamesQuery::default()
    };
    assert_eq!(src.get_games(like("%")).await.unwrap().len(), 1);
    assert_eq!(src.get_games(like("0%_M")).await.unwrap().len(), 1);
    assert_eq!(src.get_games(like("_")).await.unwrap().len(), 1);
    assert_eq!(src.get_games(like("italian")).await.unwrap().len(), 20);

    let losses = src
        .get_games(GamesQuery {
            result: Some(GameResult::Loss),
            limit: 20,
            ..GamesQuery::default()
        })
        .await
        .unwrap();
    assert!(losses.is_empty());
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn cached_eval_reads_position_evals(pool: PgPool) {
    let a = account(&pool, "alice").await;
    let input = store_input(&pool, a).await;
    sqlx::query(
        "INSERT INTO position_evals (fen_key, protocol, score_mate, best_uci, pv, source) \
         VALUES ($1, $2, 3, 'e7e5', ARRAY['e7e5'], 'stockfish')",
    )
    .bind(chess_core::fen::fen_key(AFTER_E4))
    .bind(engine::ANALYSIS_PROTOCOL)
    .execute(&pool)
    .await
    .unwrap();
    let src = ds(&pool, a, input);
    let e = src.get_cached_eval(AFTER_E4).await.unwrap().unwrap();
    assert_eq!(e.fen, AFTER_E4);
    assert_eq!(e.score, ai::input::types::Score::Mate(3));
    assert_eq!(e.best_uci.as_deref(), Some("e7e5"));
    assert_eq!(src.get_cached_eval(START).await.unwrap(), None);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn latest_coaching_input_reads_the_newest_snapshot_of_the_account(pool: PgPool) {
    let a = account(&pool, "alice").await;
    let b = account(&pool, "bob").await;
    assert_eq!(
        latest_coaching_input(&pool, a, TimeClass::Rapid)
            .await
            .unwrap(),
        None
    );
    store_input(&pool, b).await;
    assert_eq!(
        latest_coaching_input(&pool, a, TimeClass::Rapid)
            .await
            .unwrap(),
        None
    );
    let stored = store_input(&pool, a).await;
    let got = latest_coaching_input(&pool, a, TimeClass::Rapid)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(got, stored);
    assert_eq!(
        latest_coaching_input(&pool, a, TimeClass::Blitz)
            .await
            .unwrap(),
        None
    );
}

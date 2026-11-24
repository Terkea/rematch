//! Task 25: `DbCoachDataSource`, the chat's view of ONE account's data in Postgres, and
//! `latest_coaching_input`. Every query is scoped to the account; another account's rows are
//! simply not found.

#[path = "support/chat.rs"]
mod support;

use std::sync::Arc;

use ai::chat::anchor::ChatAnchor;
use ai::chat::data_source::{
    CoachDataSource, ExampleSort, ExamplesQuery, GameResult, GamesQuery, Judgement, MoveFilter,
    Phase, Standing,
};
use ai::chat::db_source::{ChatScope, DbCoachDataSource, InputSource, latest_coaching_input};
use ai::input::types::{CoachingInput, PlayerColor, TimeClass};
use ai::output::codes::SectionKey;
use serde_json::json;
use sqlx::PgPool;
use support::{
    account, seed_exemplar_positions, seed_game, seed_move, store_input, store_report_input,
    store_snapshot, v2_raw,
};
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

async fn put_eval(pool: &PgPool, fen: &str, protocol: &str, cp: i32, pv: &[&str]) {
    sqlx::query(
        "INSERT INTO position_evals (fen_key, protocol, score_cp, best_uci, pv, source)          VALUES ($1, $2, $3, $4, $5, 'stockfish')",
    )
    .bind(chess_core::fen::fen_key(fen))
    .bind(protocol)
    .bind(cp)
    .bind(pv[0])
    .bind(pv.iter().map(|m| (*m).to_owned()).collect::<Vec<_>>())
    .execute(pool)
    .await
    .unwrap();
}

/// ADR 0113: a game analysed at 300,000 nodes keeps its own engine line after the protocol moved
/// to 1 million, as the web reads it (`games.analysis_protocol`).
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn position_reads_the_engine_line_of_the_games_own_protocol(pool: PgPool) {
    let a = account(&pool, "alice").await;
    let input = store_input(&pool, a).await;
    let g = seed_game(&pool, a, "li-a-old").await;
    seed_move(&pool, g, 1, START, "e4", "e2e4", Some("e2e4")).await;
    sqlx::query("UPDATE games SET analysis_protocol = 'sf19-n300k-v1' WHERE id = $1")
        .bind(g)
        .execute(&pool)
        .await
        .unwrap();
    put_eval(&pool, START, "sf19-n300k-v1", 30, &["e2e4", "e7e5"]).await;
    put_eval(&pool, START, engine::ANALYSIS_PROTOCOL, 25, &["d2d4", "d7d5"]).await;
    let src = ds(&pool, a, input);
    let p = src.get_position("li-a-old", 1).await.unwrap().unwrap();
    assert_eq!(p.pv, vec!["e2e4".to_owned(), "e7e5".to_owned()]);

    sqlx::query("UPDATE games SET analysis_protocol = $2 WHERE id = $1")
        .bind(g)
        .bind(engine::ANALYSIS_PROTOCOL)
        .execute(&pool)
        .await
        .unwrap();
    let p = src.get_position("li-a-old", 1).await.unwrap().unwrap();
    assert_eq!(p.pv, vec!["d2d4".to_owned(), "d7d5".to_owned()]);
}

/// ADR 0113: the chat's cached eval prefers the current protocol and falls back to the earlier
/// ones, so a line tried from a game analysed at 300,000 nodes still finds its stored search.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn cached_eval_falls_back_to_an_earlier_protocol(pool: PgPool) {
    let a = account(&pool, "alice").await;
    let input = store_input(&pool, a).await;
    put_eval(&pool, AFTER_E4, "sf19-n300k-v1", 20, &["e7e5"]).await;
    let src = ds(&pool, a, input);
    let e = src.get_cached_eval(AFTER_E4).await.unwrap().unwrap();
    assert_eq!(e.score, ai::input::types::Score::Cp(20));
    put_eval(&pool, AFTER_E4, engine::ANALYSIS_PROTOCOL, 15, &["c7c5"]).await;
    let e = src.get_cached_eval(AFTER_E4).await.unwrap().unwrap();
    assert_eq!(e.score, ai::input::types::Score::Cp(15));
    assert_eq!(e.best_uci.as_deref(), Some("c7c5"));
    // Another protocol (a lab search, say) is never read.
    put_eval(&pool, START, "lab-n30m", 99, &["e2e4"]).await;
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
    assert_eq!(got.input, stored);
    assert!(matches!(got.source, InputSource::ReportRun(_)), "{got:?}");
    assert_eq!(
        latest_coaching_input(&pool, a, TimeClass::Blitz)
            .await
            .unwrap(),
        None
    );
}

/// Finding Q5 (ADR 0086): a build-profile snapshot newer than the report's input is what the
/// coach reads, so a profile rebuild after the report reaches the chat at once.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_newer_profile_snapshot_supersedes_the_report_input(pool: PgPool) {
    let a = account(&pool, "alice").await;
    store_report_input(&pool, a, &v2_raw(a, "2026-11-20T09:00:00.000Z", 159)).await;
    let snap = store_snapshot(&pool, a, &v2_raw(a, "2026-11-22T23:00:00.000Z", 162)).await;
    let got = latest_coaching_input(&pool, a, TimeClass::Rapid)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(got.source, InputSource::Snapshot(snap));
    assert_eq!(got.input.basis.analysed_games, 162);
}

/// A report built after the newest snapshot is at least as current: the coach reads it.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_report_newer_than_the_snapshot_is_read(pool: PgPool) {
    let a = account(&pool, "alice").await;
    store_snapshot(&pool, a, &v2_raw(a, "2026-11-20T09:00:00.000Z", 159)).await;
    let run = store_report_input(&pool, a, &v2_raw(a, "2026-11-21T09:00:00.000Z", 162)).await;
    let got = latest_coaching_input(&pool, a, TimeClass::Rapid)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(got.source, InputSource::ReportRun(run));
}

/// An input of an older contract (v1, before the colour split) never wins over a v2 one, even
/// when it was generated later: the coach would say it has no White and Black results.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_v1_report_input_is_superseded_by_a_v2_snapshot(pool: PgPool) {
    let a = account(&pool, "alice").await;
    let snap = store_snapshot(&pool, a, &v2_raw(a, "2026-11-19T09:00:00.000Z", 159)).await;
    // The b2-rapid fixture is v1, generated 2026-11-20.
    store_input(&pool, a).await;
    let got = latest_coaching_input(&pool, a, TimeClass::Rapid)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(got.source, InputSource::Snapshot(snap));
    assert!(got.input.by_color.is_some());
}

/// Another account's snapshot, or one of another time class, is never read.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn snapshots_are_scoped_to_the_account_and_time_class(pool: PgPool) {
    let a = account(&pool, "alice").await;
    let b = account(&pool, "bob").await;
    store_snapshot(&pool, b, &v2_raw(b, "2026-11-22T09:00:00.000Z", 10)).await;
    // A snapshot stored under bob's row but carrying alice's id is still bob's row.
    store_snapshot(&pool, b, &v2_raw(a, "2026-11-22T09:00:00.000Z", 10)).await;
    assert_eq!(
        latest_coaching_input(&pool, a, TimeClass::Rapid)
            .await
            .unwrap(),
        None
    );
    let snap = store_snapshot(&pool, a, &v2_raw(a, "2026-11-22T09:00:00.000Z", 12)).await;
    let got = latest_coaching_input(&pool, a, TimeClass::Rapid)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(got.source, InputSource::Snapshot(snap));
    assert_eq!(
        latest_coaching_input(&pool, a, TimeClass::Blitz)
            .await
            .unwrap(),
        None
    );
}

/// R2's per-game metrics row of a game, with `exemplarPlies` for one weakness key.
async fn seed_metrics(pool: &PgPool, game: Uuid, key: &str, plies: &[u32]) {
    sqlx::query(
        "INSERT INTO game_metrics (game_id, account_id, time_class, played_at, protocol, metrics) \
         SELECT id, account_id, time_class, played_at, 'test-1', $2 FROM games WHERE id = $1",
    )
    .bind(game)
    .bind(json!({
        "version": 1,
        "weakness": { key: { "events": plies.len(), "opportunities": 4, "epl": 0.1,
                             "exemplarPlies": plies } },
    }))
    .execute(pool)
    .await
    .unwrap();
}

/// The input with nothing ranked: no band reference, so the report has no exemplar at all.
fn unranked(input: CoachingInput) -> CoachingInput {
    CoachingInput {
        weaknesses: Vec::new(),
        strengths: Vec::new(),
        openings: Vec::new(),
        changes: Vec::new(),
        ..input
    }
}

// Bug hunt I10: with no benchmark nothing is ranked, so the input-only search found nothing and
// the coach said it "found none" while R2 had stored the occurrences. find_examples reads R2's
// per-game exemplar plies, move features and openings of the account and time class.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn find_examples_reads_r2_occurrences_when_nothing_is_ranked(pool: PgPool) {
    let a = account(&pool, "alice").await;
    let input = unranked(store_input(&pool, a).await);
    let g = seed_game(&pool, a, "li-r2-w1").await;
    seed_move(&pool, g, 1, START, "e4", "e2e4", Some("d2d4")).await;
    seed_metrics(&pool, g, "W1", &[1]).await;
    // Another account's occurrence of the same weakness is never read.
    let b = account(&pool, "bob").await;
    let gb = seed_game(&pool, b, "li-r2-bob").await;
    seed_move(&pool, gb, 1, START, "e4", "e2e4", Some("d2d4")).await;
    seed_metrics(&pool, gb, "W1", &[1]).await;
    let src = ds(&pool, a, input);

    let w1 = src
        .find_examples(ExamplesQuery {
            weakness_id: Some("W1".into()),
            limit: 5,
            ..ExamplesQuery::default()
        })
        .await
        .unwrap();
    let ids: Vec<(&str, u32)> = w1.iter().map(|p| (p.game_id.as_str(), p.ply)).collect();
    assert_eq!(ids, vec![("li-r2-w1", 1)]);

    // A motif: the player's erring moves whose best or refuting line has it (R2 move features).
    sqlx::query(
        "INSERT INTO move_features (game_id, ply, enrichment_protocol, best_line) \
         VALUES ($1, 1, 'test-1', $2)",
    )
    .bind(g)
    .bind(json!({"gain": 0.3, "depth": 2, "mateIn": null, "tags": ["fork", "hangingPiece"]}))
    .execute(&pool)
    .await
    .unwrap();
    let forks = src
        .find_examples(ExamplesQuery {
            motif: Some("fork".into()),
            limit: 5,
            ..ExamplesQuery::default()
        })
        .await
        .unwrap();
    assert_eq!(forks.len(), 1);
    assert_eq!(forks[0].game_id, "li-r2-w1");
    assert!(
        forks[0].tags.contains(&"fork".to_owned()),
        "{:?}",
        forks[0].tags
    );
    // EXP-0007 hid `hangingPiece` (#122): the coach is never told it, nor can it search by it.
    assert!(
        !forks[0].tags.contains(&"hanging_piece".to_owned()),
        "{:?}",
        forks[0].tags
    );
    let hidden = src
        .find_examples(ExamplesQuery {
            motif: Some("hanging_piece".into()),
            limit: 5,
            ..ExamplesQuery::default()
        })
        .await
        .unwrap();
    assert!(hidden.is_empty(), "{hidden:?}");

    // An opening line: the player's worst move in the line's first 30 plies, per game.
    sqlx::query(
        "INSERT INTO game_openings (game_id, account_id, time_class, color, fen_keys, ucis, sans, \
         line_key) VALUES ($1, $2, 'rapid', 'white', '{}', '{}', '{}', 'w-kings-pawn')",
    )
    .bind(g)
    .bind(a)
    .execute(&pool)
    .await
    .unwrap();
    let line = src
        .find_examples(ExamplesQuery {
            line_key: Some("w-kings-pawn".into()),
            limit: 5,
            ..ExamplesQuery::default()
        })
        .await
        .unwrap();
    assert_eq!(line.len(), 1);
    assert_eq!((line[0].game_id.as_str(), line[0].ply), ("li-r2-w1", 1));
}

/// The chat never runs the engine for `find_examples`.
struct NoEngine;

impl ai::chat::data_source::InteractiveEngine for NoEngine {
    fn request<'a>(
        &'a self,
        _base_fen: &'a str,
        _moves: &'a [String],
        _wait: std::time::Duration,
    ) -> futures::future::BoxFuture<'a, anyhow::Result<ai::chat::data_source::EngineCheck>> {
        Box::pin(async { anyhow::bail!("no engine in this test") })
    }

    fn wait<'a>(
        &'a self,
        _eval_id: &'a str,
        _wait: std::time::Duration,
    ) -> futures::future::BoxFuture<'a, anyhow::Result<ai::chat::data_source::EngineCheck>> {
        Box::pin(async { anyhow::bail!("no engine in this test") })
    }

    fn abandon<'a>(
        &'a self,
        _eval_id: &'a str,
    ) -> futures::future::BoxFuture<'a, anyhow::Result<()>> {
        Box::pin(async { Ok(()) })
    }
}

// I10 through the coach's own tool: the model asks for hung-material examples on a profile with
// nothing ranked and gets the stored occurrence, citable, never an empty "found none".
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn the_find_examples_tool_cites_r2_occurrences(pool: PgPool) {
    use ai::chat::execute::{ToolDeps, ToolExec, ToolSession, execute_tool};
    let a = account(&pool, "alice").await;
    let input = unranked(store_input(&pool, a).await);
    let g = seed_game(&pool, a, "li-r2-tool").await;
    seed_move(&pool, g, 1, START, "e4", "e2e4", Some("d2d4")).await;
    seed_metrics(&pool, g, "W1", &[1]).await;
    let src = ds(&pool, a, input);
    let session = ToolSession::new(ai::validator::evidence::create_evidence());
    let deps = ToolDeps {
        ds: &src,
        engine: &NoEngine,
    };
    let out = execute_tool(
        &deps,
        &session,
        "find_examples",
        Some(&json!({"weaknessId": "W1", "limit": 3})),
    )
    .await;
    let ToolExec::Ready(o) = out else {
        panic!("expected a ready result, got {out:?}");
    };
    let v: serde_json::Value = serde_json::from_str(&o.content).unwrap();
    let examples = v["examples"].as_array().unwrap();
    assert_eq!(examples.len(), 1, "{v}");
    assert_eq!(examples[0]["position"]["gameId"], "li-r2-tool");
    assert!(session.evidence().game_ids.contains("li-r2-tool"));
}

// --- Fix E (N8): the move filter ---------------------------------------------------------------
//
// "Show me one rapid game where I blundered in a winning endgame" found nothing: find_examples
// only knew weaknesses, motifs and opening lines. The move filter reads the player's own moves of
// the scope (account and time class) by phase, judgement, win chance, drop, clock, colour and
// date, one move per game (its worst match), newest game first.

fn day(s: &str) -> time::Date {
    time::Date::parse(s, time::macros::format_description!("[year]-[month]-[day]")).unwrap()
}

/// A game of `account` with the player's colour, time class and day (2026-MM-DD).
async fn filter_game(
    pool: &PgPool,
    account: Uuid,
    id: &str,
    color: &str,
    time_class: &str,
    on: &str,
) -> Uuid {
    let g = seed_game(pool, account, id).await;
    sqlx::query(
        "UPDATE games SET player_color = $2::color, time_class = $3::time_class, \
         played_at = ($4::date)::timestamp AT TIME ZONE 'UTC' + interval '12 hours' WHERE id = $1",
    )
    .bind(g)
    .bind(color)
    .bind(time_class)
    .bind(day(on))
    .execute(pool)
    .await
    .unwrap();
    g
}

/// One move, as `(ply, by the player's side, phase, judgement, win before, drop, clock)`.
type Mv<'a> = (i32, bool, &'a str, Option<&'a str>, f64, f64, f64);

/// Moves of a game: White's `e4` from the start or Black's `e5` after it (both legal, so the
/// record has its position). `white` is the mover.
async fn filter_moves(pool: &PgPool, game: Uuid, moves: &[Mv<'_>]) {
    for &(ply, white, phase, judgement, win_before, delta_w, clock) in moves {
        let (color, fen, san, uci) = if white {
            ("white", START, "e4", "e2e4")
        } else {
            ("black", AFTER_E4, "e5", "e7e5")
        };
        sqlx::query(
            "INSERT INTO moves (game_id, ply, color, san, uci, fen_before, best_uci, win_before, \
             win_after, delta_w, accuracy, judgement, phase, clock_before, time_spent, decided) \
             VALUES ($1, $2, $3::color, $4, $5, $6, NULL, $7, greatest($7 - $8, 0), $8, 40.0, \
             $9::judgement, $10::phase, $11, 5.0, false)",
        )
        .bind(game)
        .bind(ply)
        .bind(color)
        .bind(san)
        .bind(uci)
        .bind(fen)
        .bind(win_before)
        .bind(delta_w)
        .bind(judgement)
        .bind(phase)
        .bind(clock)
        .execute(pool)
        .await
        .unwrap();
    }
}

/// Alice's rapid games (oldest first) plus moves that must never match: the opponent's, a blitz
/// game's, another account's. `g-none` has no analysed move and is never searched.
async fn filter_world(pool: &PgPool) -> DbCoachDataSource {
    let a = account(pool, "alice").await;
    let input = unranked(store_input(pool, a).await);
    let old = filter_game(pool, a, "g-old", "white", "rapid", "2026-02-01").await;
    filter_moves(
        pool,
        old,
        &[
            (21, true, "middlegame", Some("inaccuracy"), 60.0, 8.0, 300.0),
            (23, true, "middlegame", Some("mistake"), 55.0, 70.0, 200.0),
        ],
    )
    .await;
    let mid = filter_game(pool, a, "g-mid", "black", "rapid", "2026-03-01").await;
    filter_moves(
        pool,
        mid,
        &[
            (40, false, "endgame", Some("blunder"), 80.0, 50.0, 30.0),
            // The opponent's blunder in the same game: never the player's example.
            (41, true, "endgame", Some("blunder"), 90.0, 60.0, 20.0),
            // A good move (no judgement).
            (42, false, "endgame", None, 85.0, 1.0, 25.0),
        ],
    )
    .await;
    let new = filter_game(pool, a, "g-new", "white", "rapid", "2026-04-01").await;
    filter_moves(
        pool,
        new,
        &[
            (31, true, "opening", Some("mistake"), 50.0, 20.0, 500.0),
            (61, true, "endgame", Some("blunder"), 75.0, 40.0, 100.0),
            (63, true, "endgame", Some("blunder"), 72.0, 60.0, 50.0),
            (65, true, "endgame", Some("blunder"), 20.0, 15.0, 10.0),
        ],
    )
    .await;
    filter_game(pool, a, "g-none", "white", "rapid", "2026-05-01").await;
    let blitz = filter_game(pool, a, "g-blitz", "white", "blitz", "2026-04-02").await;
    let winning_blunder: Mv = (61, true, "endgame", Some("blunder"), 80.0, 50.0, 30.0);
    filter_moves(pool, blitz, &[winning_blunder]).await;
    let b = account(pool, "bob").await;
    let bob = filter_game(pool, b, "g-bob", "white", "rapid", "2026-04-03").await;
    filter_moves(pool, bob, &[winning_blunder]).await;
    ds(pool, a, input)
}

async fn filtered(src: &DbCoachDataSource, f: MoveFilter, limit: u32) -> Vec<(String, u32)> {
    src.find_examples(ExamplesQuery {
        filter: Some(f),
        limit,
        ..ExamplesQuery::default()
    })
    .await
    .unwrap()
    .into_iter()
    .map(|p| (p.game_id, p.ply))
    .collect()
}

fn ids(xs: &[(&str, u32)]) -> Vec<(String, u32)> {
    xs.iter().map(|(g, p)| ((*g).to_owned(), *p)).collect()
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn move_filter_finds_blunders_in_winning_endgames(pool: PgPool) {
    let src = filter_world(&pool).await;
    let f = MoveFilter {
        phase: Some(Phase::Endgame),
        judgement: Some(Judgement::Blunder),
        standing: Some(Standing::Winning),
        ..MoveFilter::default()
    };
    // g-new's worst winning endgame blunder (ply 63 dropped more than 61), then g-mid's; never
    // the opponent's, the blitz game's or Bob's.
    assert_eq!(
        filtered(&src, f.clone(), 5).await,
        ids(&[("g-new", 63), ("g-mid", 40)])
    );
    assert_eq!(filtered(&src, f.clone(), 1).await, ids(&[("g-new", 63)]));
    let recs = src
        .find_examples(ExamplesQuery {
            filter: Some(f.clone()),
            limit: 1,
            ..ExamplesQuery::default()
        })
        .await
        .unwrap();
    let r = &recs[0];
    assert_eq!(
        (r.phase.as_str(), r.judgement.as_deref()),
        ("endgame", Some("blunder"))
    );
    assert!(r.win_before >= 70.0);
    assert_eq!(r.played.uci, "e2e4");
    let counts = src.count_examples(&f).await.unwrap().unwrap();
    assert_eq!(counts.time_class, "rapid");
    assert_eq!(
        (
            counts.matching_moves,
            counts.matching_games,
            counts.games_searched
        ),
        (3, 2, 3),
        "g-none has no analysed move, so it is not searched"
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn move_filter_judgement_and_severity(pool: PgPool) {
    let src = filter_world(&pool).await;
    let exact = MoveFilter {
        judgement: Some(Judgement::Inaccuracy),
        ..MoveFilter::default()
    };
    assert_eq!(filtered(&src, exact, 5).await, ids(&[("g-old", 21)]));
    let at_least_mistake = MoveFilter {
        min_judgement: Some(Judgement::Mistake),
        ..MoveFilter::default()
    };
    assert_eq!(
        filtered(&src, at_least_mistake.clone(), 5).await,
        ids(&[("g-new", 63), ("g-mid", 40), ("g-old", 23)])
    );
    // Worst first: g-old's mistake dropped the most.
    let worst = MoveFilter {
        sort: ExampleSort::Worst,
        ..at_least_mistake
    };
    assert_eq!(
        filtered(&src, worst, 5).await,
        ids(&[("g-old", 23), ("g-new", 63), ("g-mid", 40)])
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn move_filter_win_chance_presets_and_bounds(pool: PgPool) {
    let src = filter_world(&pool).await;
    let standing = |s| MoveFilter {
        standing: Some(s),
        ..MoveFilter::default()
    };
    assert_eq!(
        filtered(&src, standing(Standing::Losing), 5).await,
        ids(&[("g-new", 65)])
    );
    assert_eq!(
        filtered(&src, standing(Standing::Equal), 5).await,
        ids(&[("g-new", 31), ("g-old", 23)])
    );
    let bounds = |lo, hi| MoveFilter {
        min_win_before: lo,
        max_win_before: hi,
        ..MoveFilter::default()
    };
    assert_eq!(
        filtered(&src, bounds(None, Some(25)), 5).await,
        ids(&[("g-new", 65)])
    );
    assert_eq!(
        filtered(&src, bounds(Some(55), Some(60)), 5).await,
        ids(&[("g-old", 23)])
    );
    // Without a judgement or drop filter only erring moves match, so g-mid's good move (85) is
    // left out; a drop filter includes it.
    assert!(filtered(&src, bounds(Some(84), None), 5).await.is_empty());
    let good = MoveFilter {
        min_win_before: Some(84),
        min_delta_w: Some(1),
        ..MoveFilter::default()
    };
    assert_eq!(filtered(&src, good, 5).await, ids(&[("g-mid", 42)]));
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn move_filter_drop_clock_colour_dates_and_last_games(pool: PgPool) {
    let src = filter_world(&pool).await;
    let drop = MoveFilter {
        min_delta_w: Some(45),
        ..MoveFilter::default()
    };
    assert_eq!(
        filtered(&src, drop, 5).await,
        ids(&[("g-new", 63), ("g-mid", 40), ("g-old", 23)])
    );
    let clock = MoveFilter {
        clock_below: Some(40),
        ..MoveFilter::default()
    };
    assert_eq!(
        filtered(&src, clock, 5).await,
        ids(&[("g-new", 65), ("g-mid", 40)])
    );
    let black = MoveFilter {
        color: Some(PlayerColor::Black),
        ..MoveFilter::default()
    };
    assert_eq!(filtered(&src, black, 5).await, ids(&[("g-mid", 40)]));
    let march = MoveFilter {
        since: Some(day("2026-03-01")),
        until: Some(day("2026-03-31")),
        ..MoveFilter::default()
    };
    assert_eq!(
        filtered(&src, march.clone(), 5).await,
        ids(&[("g-mid", 40)])
    );
    let c = src.count_examples(&march).await.unwrap().unwrap();
    assert_eq!((c.games_searched, c.matching_games), (1, 1));
    // The newest analysed game only (g-none has no moves).
    let last = MoveFilter {
        last_games: Some(1),
        ..MoveFilter::default()
    };
    assert_eq!(filtered(&src, last.clone(), 5).await, ids(&[("g-new", 63)]));
    let c = src.count_examples(&last).await.unwrap().unwrap();
    assert_eq!((c.games_searched, c.matching_moves), (1, 4));
}

// The N8 question through the coach's own tool: the examples carry their positions (so the
// answer's boards render) and `searched` says what was searched, with citable counts.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn the_find_examples_tool_filters_the_players_moves(pool: PgPool) {
    use ai::chat::execute::{ToolDeps, ToolExec, ToolSession, execute_tool};
    use ai::validator::format::resolve_token;
    use ai::validator::tokens::parse_tokens;
    let src = filter_world(&pool).await;
    let session = ToolSession::new(ai::validator::evidence::create_evidence());
    let deps = ToolDeps {
        ds: &src,
        engine: &NoEngine,
    };
    let input =
        json!({"phase": "endgame", "judgement": "blunder", "standing": "winning", "limit": 1});
    let out = execute_tool(&deps, &session, "find_examples", Some(&input)).await;
    let ToolExec::Ready(o) = out else {
        panic!("expected a ready result, got {out:?}");
    };
    assert!(!o.is_error, "{}", o.content);
    let v: serde_json::Value = serde_json::from_str(&o.content).unwrap();
    let ex = v["examples"].as_array().unwrap();
    assert_eq!(ex.len(), 1, "{v}");
    assert_eq!(ex[0]["position"]["gameId"], "g-new");
    assert_eq!(ex[0]["position"]["ply"], 63);
    assert!(session.evidence().positions.contains_key("g-new#63"));
    assert_eq!(v["searched"]["timeClass"], "rapid");
    assert_eq!(v["searched"]["matchingMoves"], 3);
    assert_eq!(v["searched"]["matchingGames"], 2);
    assert_eq!(v["searched"]["gamesSearched"], 3);
    // "winning" is an engine word: the bound it stands for is citable instead.
    assert_eq!(v["searched"]["minWinBefore"], 70);
    let bound = v["refs"]["num"]["searched.minWinBefore"].as_str().unwrap();
    let ev = session.evidence();
    let shown = resolve_token(&ev, &parse_tokens(bound).tokens[0].body).unwrap();
    assert!(shown.starts_with("70") && shown.ends_with('%'), "{shown}");
    let tok = v["refs"]["num"]["searched.matchingMoves"].as_str().unwrap();
    let parsed = parse_tokens(tok);
    let ev = session.evidence();
    assert_eq!(
        resolve_token(&ev, &parsed.tokens[0].body).as_deref(),
        Some("3")
    );
}

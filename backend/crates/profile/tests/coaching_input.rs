//! The CoachingInput builder (R2 Task 34, TS M2 Task 26): field rules, the M3 contract and its
//! limits, on the Task 33 seed with real moves.

mod common {
    pub mod moves;
    pub mod seed;
}

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::time::Duration;

use chess_core::board::Position;
use chess_core::pgn::parse_pgn;
use chess_core::{Color, TimeClass};
use coaching_input::limits::{WEAKNESS_ID_MAX_CHARS, WEAKNESS_ID_PATTERN};
use coaching_input::{
    AreaId, BandId, CoachingInput, ExampleSide, LIMITS, LineSide, Platform, Verdict, WeaknessState,
};
use common::moves::*;
use common::seed::{GameShape, PROTOCOL, SEED_NOW, SeedOptions, WHITE_GAME, seed_now, w1_stat};
use profile::build_profile::{BuildProfileDeps, handle_build_profile};
use profile::coaching::builder::{BuildError, build_coaching_input, build_coaching_input_report};
use profile::coaching::exemplars::{ExemplarRow, to_exemplar_position};
use profile::coaching::mapping::{contract_line_key, contract_platform, weakness_id_of_series};
use profile::coaching::strengths::{StrengthCandidate, StrengthSource, select_strengths};
use profile::timeline_job::{StoredHistory, state_changes};
use profile_core::openings::lines::{BandComparison, BandSource, LineSummary};
use profile_core::timeline::series::{Evidence, Onset, RowKind, SeriesOutcome, TimelineRow};
use profile_core::types::{
    AREAS, BandReferenceStat, MetricId, MotifTag, PlatformId, SeriesKey, WeaknessKey,
};
use queue::BuildProfilePayload;
use queue::queue::Queue;
use serde_json::{Value, json};
use sqlx::PgPool;
use testkit::SCHOLARS;
use time::macros::datetime;
use time::{Duration as TimeDuration, OffsetDateTime};
use uuid::Uuid;

const TEST_TIMEOUT: Duration = Duration::from_secs(120);

async fn within<T>(what: &str, fut: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(TEST_TIMEOUT, fut)
        .await
        .unwrap_or_else(|_| panic!("timed out after {TEST_TIMEOUT:?} waiting for {what}"))
}

async fn run_build(pool: &PgPool, account_id: Uuid) {
    let deps = BuildProfileDeps {
        pool: pool.clone(),
        queue: Queue::new(pool.clone()),
        now: seed_now,
        benchmark: profile::peer_benchmark::BenchmarkSource::LichessBands,
    };
    within(
        "build-profile",
        handle_build_profile(
            &deps,
            BuildProfilePayload {
                account_id,
                time_class: "blitz".to_owned(),
            },
        ),
    )
    .await
    .expect("build-profile");
}

async fn coaching(pool: &PgPool, account_id: Uuid) -> CoachingInput {
    within(
        "build_coaching_input",
        build_coaching_input(pool, account_id, TimeClass::Blitz, SEED_NOW),
    )
    .await
    .unwrap_or_else(|e| panic!("build_coaching_input: {e:?}"))
}

async fn built_fixture(pool: &PgPool) -> (Fixture, CoachingInput) {
    let f = seed_with_moves(pool, SeedOptions::default()).await;
    run_build(pool, f.account_id).await;
    let input = coaching(pool, f.account_id).await;
    (f, input)
}

fn all_exemplars(input: &CoachingInput) -> Vec<&coaching_input::ExemplarPosition> {
    let mut v: Vec<&coaching_input::ExemplarPosition> = Vec::new();
    for w in &input.weaknesses {
        v.extend(&w.exemplars);
    }
    for s in &input.strengths {
        v.extend(&s.exemplars);
    }
    for o in &input.openings {
        v.extend(&o.examples);
    }
    for c in &input.changes {
        v.extend(c.examples.iter().map(|e| &e.position));
    }
    v
}

// ------------------------------------------------------------------ TS M2 Task 26 cases

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn area_scores_follow_contract_order(pool: PgPool) {
    let (_, input) = built_fixture(&pool).await;
    let areas: Vec<AreaId> = input.area_scores.iter().map(|a| a.area).collect();
    assert_eq!(areas, AREAS.to_vec());
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn weaknesses_are_ranked_from_1_by_non_increasing_priority(pool: PgPool) {
    let (_, input) = built_fixture(&pool).await;
    assert!(!input.weaknesses.is_empty(), "W1 is ranked");
    for (i, w) in input.weaknesses.iter().enumerate() {
        assert_eq!(w.rank as usize, i + 1);
        assert!(w.priority > 0.0, "{} has priority {}", w.id, w.priority);
        assert!(!w.exemplars.is_empty(), "{} has no exemplar", w.id);
    }
    for pair in input.weaknesses.windows(2) {
        assert!(pair[0].priority >= pair[1].priority);
    }
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn w1_has_three_exemplars_by_delta_w_from_distinct_games(pool: PgPool) {
    let (_, input) = built_fixture(&pool).await;
    let w1 = input
        .weaknesses
        .iter()
        .find(|w| w.id == "W1" && w.variant.is_none())
        .expect("W1 ranked");
    assert_eq!(w1.exemplars.len(), 3);
    assert_eq!(w1.state, WeaknessState::Active);
    assert_eq!(w1.events, 80);
    assert_eq!(w1.opportunities, 2000);
    let wins: Vec<f64> = w1.exemplars.iter().map(|e| e.win_after).collect();
    assert_eq!(wins, vec![25.0, 35.0, 45.0], "ordered by ΔW descending");
    let games: BTreeSet<&str> = w1.exemplars.iter().map(|e| e.game_id.as_str()).collect();
    assert_eq!(games.len(), 3, "distinct games");
    let e = &w1.exemplars[0];
    assert_eq!(e.ply, EXEMPLAR_PLY);
    assert_eq!(e.played.san, "Bc4");
    assert_eq!(e.best.as_ref().map(|b| b.san.as_str()), Some("Nf3"));
    assert_eq!(e.refutation_pv, vec!["g8f6", "h5f7"], "illegal tail cut");
    assert_eq!(e.tags, vec!["fork", "mate_in_1", "mate"]);
    assert_eq!(e.eval_before, coaching_input::Score::Cp(SEED_CP));
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn exemplars_replay_legally_from_their_fen(pool: PgPool) {
    let (_, input) = built_fixture(&pool).await;
    let exemplars = all_exemplars(&input);
    assert!(exemplars.len() >= 3);
    for e in exemplars {
        let start = Position::from_fen(&e.fen).expect("exemplar FEN parses");
        if let Some(best) = &e.best {
            assert_eq!(
                e.best_pv.first(),
                Some(&best.uci),
                "best_pv starts with best"
            );
            let mut p = start.clone();
            assert_eq!(p.push_uci(&best.uci).unwrap().san, best.san);
        }
        let mut p = start.clone();
        for uci in &e.best_pv {
            p.push_uci(uci).expect("best_pv replays legally");
        }
        let mut p = start.clone();
        assert_eq!(p.push_uci(&e.played.uci).unwrap().san, e.played.san);
        for uci in &e.refutation_pv {
            p.push_uci(uci)
                .expect("refutation_pv replays legally after played");
        }
    }
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn basis_counts_the_seed_games(pool: PgPool) {
    let (_, input) = built_fixture(&pool).await;
    let b = &input.basis;
    assert_eq!((b.analysed_games, b.imported_games), (40, 40));
    assert_eq!(b.window_start, datetime!(2026-11-10 00:00 UTC).date());
    assert_eq!(b.window_end, datetime!(2026-11-18 00:00 UTC).date());
    assert!(b.backfill_complete, "every game is done");
    assert_eq!(input.band, BandId::B2);
    assert_eq!(input.rating, Some(1500));
    assert_eq!(input.platform, Platform::Lichess);
    assert_eq!(
        input.time_control,
        Some(coaching_input::TimeControl {
            base_seconds: 180,
            increment_seconds: 0
        })
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn backfill_is_incomplete_while_a_game_waits(pool: PgPool) {
    let f = seed_with_moves(&pool, SeedOptions::default()).await;
    sqlx::query("UPDATE games SET analysis_status = 'imported' WHERE id = $1")
        .bind(f.games[0])
        .execute(&pool)
        .await
        .unwrap();
    let input = coaching(&pool, f.account_id).await;
    assert!(!input.basis.backfill_complete);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn json_has_no_focus_and_round_trips_unchanged(pool: PgPool) {
    let (_, input) = built_fixture(&pool).await;
    let v = input.to_json();
    assert!(v.get("focus").is_none());
    let text = serde_json::to_string(&v).unwrap();
    let reparsed: Value = serde_json::from_str(&text).unwrap();
    // serde_json's default number parser may be 1 ulp off (no `float_roundtrip` feature);
    // the text itself is shortest-round-trip (ryu), so JSON.parse reads it exactly.
    assert_json_close(&reparsed, &v, "$");
    let back: CoachingInput = serde_json::from_value(reparsed).expect("strict types read it back");
    assert_json_close(&back.to_json(), &v, "$");
}

/// Equal JSON, numbers within a relative 1e-15 (see the round-trip test).
fn assert_json_close(a: &Value, b: &Value, path: &str) {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            let (x, y) = (x.as_f64().unwrap(), y.as_f64().unwrap());
            assert!(x.is_finite() && y.is_finite(), "{path}: non-finite");
            assert!(
                (x - y).abs() <= 1e-15 * x.abs().max(y.abs()),
                "{path}: {x} vs {y}"
            );
        }
        (Value::Array(x), Value::Array(y)) => {
            assert_eq!(x.len(), y.len(), "{path}: length");
            for (i, (x, y)) in x.iter().zip(y).enumerate() {
                assert_json_close(x, y, &format!("{path}[{i}]"));
            }
        }
        (Value::Object(x), Value::Object(y)) => {
            let kx: BTreeSet<&String> = x.keys().collect();
            let ky: BTreeSet<&String> = y.keys().collect();
            assert_eq!(kx, ky, "{path}: keys");
            for (k, xv) in x {
                assert_json_close(xv, &y[k], &format!("{path}.{k}"));
            }
        }
        _ => assert_eq!(a, b, "{path}"),
    }
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn builder_output_validates_against_ts_schema(pool: PgPool) {
    let (_, input) = built_fixture(&pool).await;
    assert_eq!(input.validate(), Ok(()));
    let v = ts_validator();
    let doc = input.to_json();
    assert_eq!(schema_errors(&v, &doc), Vec::<String>::new());

    // The validator is live: documents the contract forbids are rejected.
    let mut extra = doc.clone();
    extra["focus"] = json!([]);
    assert!(!schema_errors(&v, &extra).is_empty(), "unknown root key");
    let mut long_pv = doc.clone();
    long_pv["weaknesses"][0]["exemplars"][0]["bestPv"] = json!(vec!["g1f3"; 17]);
    assert!(!schema_errors(&v, &long_pv).is_empty(), "17-ply PV");
    let mut no_band = doc.clone();
    no_band["weaknesses"][0]["bandMedian"] = Value::Null;
    assert!(!schema_errors(&v, &no_band).is_empty(), "null bandMedian");
}

fn ts_validator() -> jsonschema::Validator {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/coaching-input/ts-schema.json"
    );
    let schema: Value =
        serde_json::from_str(&std::fs::read_to_string(path).expect("ts-schema.json")).unwrap();
    jsonschema::options()
        .should_validate_formats(true)
        .build(&schema)
        .expect("the TS schema compiles")
}

/// The document's errors: its v1 part (without v2's `byColor` and `benchmark`, ADR 0085, and
/// v3's `trajectory`, ADR 0089)
/// against the zod export, which is v1's, and the whole document against the contract's own
/// schema.
fn schema_errors(v: &jsonschema::Validator, doc: &Value) -> Vec<String> {
    let mut v1 = doc.clone();
    if let Some(o) = v1.as_object_mut() {
        o.remove("byColor");
        o.remove("benchmark");
        o.remove("trajectory");
        o.insert("inputVersion".into(), json!("coaching-input-v1"));
    }
    let contract = jsonschema::options()
        .should_validate_formats(true)
        .build(&coaching_input::json_schema())
        .expect("the contract schema compiles");
    v.iter_errors(&v1)
        .chain(contract.iter_errors(doc))
        .map(|e| format!("{e} at {}", e.instance_path()))
        .collect()
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn coaching_input_for_account_with_0_analysed_games_validates(pool: PgPool) {
    let account_id = testkit::insert_account(&pool, "lichess", "newcomer").await;
    let input = coaching(&pool, account_id).await;
    assert!(input.weaknesses.is_empty());
    assert!(input.changes.is_empty());
    assert!(input.strengths.is_empty());
    assert_eq!(input.area_scores.len(), 7);
    assert!(input.area_scores.iter().all(|a| !a.sufficient));
    assert_eq!(input.band, BandId::B1, "R-4 with ruling Q2");
    assert_eq!(input.rating, None);
    assert_eq!(input.time_control, None);
    assert_eq!(input.basis.window_start, SEED_NOW.date(), "R-5");
    assert_eq!(input.basis.window_end, SEED_NOW.date(), "R-5");
    assert_eq!(
        (input.basis.analysed_games, input.basis.imported_games),
        (0, 0)
    );
    assert_eq!(input.validate(), Ok(()));
    assert_eq!(
        schema_errors(&ts_validator(), &input.to_json()),
        Vec::<String>::new(),
        "valid against the TS schema"
    );
    let back: CoachingInput = serde_json::from_value(input.to_json()).unwrap();
    assert_eq!(back, input);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn unknown_account_is_a_db_error(pool: PgPool) {
    let r = build_coaching_input(&pool, Uuid::new_v4(), TimeClass::Blitz, SEED_NOW).await;
    assert!(matches!(r, Err(BuildError::Db(_))), "{r:?}");
}

// ------------------------------------------------------------------ limits and ranges

/// A legal 20-ply line from the exemplar position (knights out and back).
fn twenty_ply_pv() -> Vec<&'static str> {
    ["g1f3", "g8f6", "f3g1", "f6g8"]
        .iter()
        .copied()
        .cycle()
        .take(20)
        .collect()
}

fn line_summary(key: &str, name: &str, games: u32) -> LineSummary {
    LineSummary {
        line_key: key.to_owned(),
        name: name.to_owned(),
        eco: Some("C20".to_owned()),
        color: Color::White,
        kind: LineSide::Chosen,
        time_class: "blitz".to_owned(),
        moves_san: vec!["e4".into(), "e5".into(), "Qh5".into()],
        games,
        wins: games,
        draws: 0,
        losses: 0,
        score: 1.0,
        perf_vs_expected: Some(0.5),
        perf_games: games,
        perf_interval: None,
        analysed_games: games,
        avg_win_at_opening_end: Some(60.0),
        avg_opening_accuracy: Some(90.0),
        avg_time_in_opening: None,
        avg_eval_white_cp: Some(40),
        eval_games: games,
        deviation_ply: None,
        deviation_move_san: None,
        median_deviation_ply: None,
        deviation_evidence_games: 0,
        choice_fen_key: None,
        choice_uci: None,
        alternatives: Vec::new(),
        comparison: BandComparison {
            player_band: profile_core::types::Band::B2,
            band_source: BandSource::Rating,
            explorer_band: None,
            explorer_fallback: false,
        },
        share: 0.1,
        error_spot: None,
        struggles: Vec::new(),
        fix_spot: None,
        verdict: Some(Verdict::Keep),
        reason: "test".to_owned(),
    }
}

async fn insert_snapshot(
    pool: &PgPool,
    account_id: Uuid,
    metric: &str,
    value: f64,
    ci: Option<(f64, f64)>,
) {
    sqlx::query(
        "INSERT INTO kpi_snapshot (account_id, time_class, metric, window_kind, window_key, \
           value, value_adjusted, n, ci_low, ci_high, protocol) \
         VALUES ($1, 'blitz', $2, 'last_n', 'last50', $3, NULL, 40, $4, $5, $6)",
    )
    .bind(account_id)
    .bind(metric)
    .bind(value)
    .bind(ci.map(|c| c.0))
    .bind(ci.map(|c| c.1))
    .bind(PROTOCOL)
    .execute(pool)
    .await
    .expect("insert snapshot");
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn builder_respects_limits_and_unit_ranges(pool: PgPool) {
    let f = seed_with_moves(&pool, SeedOptions::default()).await;
    // A 20-ply best line on the first exemplar game.
    let pv = twenty_ply_pv();
    insert_features(
        &pool,
        f.games[EXEMPLAR_GAMES[0]],
        EXEMPLAR_PLY,
        &pv,
        &["g8f6", "h5f7"],
    )
    .await;
    run_build(&pool, f.account_id).await;

    // 31 opening lines with verdicts, one game each: one key of 70 characters and one name of
    // 90 characters.
    sqlx::query("DELETE FROM opening_lines WHERE account_id = $1")
        .bind(f.account_id)
        .execute(&pool)
        .await
        .unwrap();
    let long_key = format!("white:{}", "a-very-long-opening-family-name".repeat(3));
    assert!(long_key.chars().count() > 64);
    let long_name = "N".repeat(90);
    for i in 0..31usize {
        let key = if i == 0 {
            long_key.clone()
        } else {
            format!("white:line-{i}")
        };
        let name = if i == 1 {
            long_name.clone()
        } else {
            format!("Line {i}")
        };
        let games = u32::try_from(31 - i).unwrap();
        let summary = line_summary(&key, &name, games);
        sqlx::query(
            "INSERT INTO opening_lines (account_id, time_class, color, line_key, kind, games, \
               verdict, summary) VALUES ($1, 'blitz', 'white', $2, 'chosen', $3, 'keep', $4)",
        )
        .bind(f.account_id)
        .bind(&key)
        .bind(i32::try_from(games).unwrap())
        .bind(serde_json::to_value(&summary).unwrap())
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "UPDATE game_openings SET line_key = $2, line_kind = 'chosen', color = 'white' \
             WHERE game_id = $1",
        )
        .bind(f.games[i])
        .bind(&key)
        .execute(&pool)
        .await
        .unwrap();
    }
    // A signed metric and a rate interval above 1.
    insert_snapshot(
        &pool,
        f.account_id,
        "eval_trend_15_35",
        -8.0,
        Some((-9.0, -7.0)),
    )
    .await;
    insert_snapshot(
        &pool,
        f.account_id,
        "time_trouble_rate",
        0.9,
        Some((0.8, 1.04)),
    )
    .await;

    let input = coaching(&pool, f.account_id).await;
    assert_eq!(input.openings.len(), LIMITS.openings);
    let games: Vec<u32> = input.openings.iter().map(|o| o.games).collect();
    assert_eq!(
        games.first(),
        Some(&31),
        "ordered by games, the smallest cut"
    );
    assert!(!games.contains(&1));
    for o in &input.openings {
        assert!(
            o.line_key.chars().count() <= LIMITS.id_chars,
            "{}",
            o.line_key
        );
        assert!(!o.examples.is_empty(), "{} has an example", o.line_key);
        assert!(o.examples.len() <= LIMITS.examples_per_opening);
    }
    assert!(input.openings.iter().any(|o| o.name == long_name));
    assert_eq!(
        input.openings[0].line_key,
        contract_line_key(&long_key),
        "R-7 cut"
    );
    let max_pv = all_exemplars(&input)
        .iter()
        .map(|e| e.best_pv.len().max(e.refutation_pv.len()))
        .max()
        .unwrap();
    assert_eq!(max_pv, LIMITS.pv_plies, "the 20-ply PV is cut to 16");
    assert!(
        input.metrics.iter().all(|m| m.key != "eval_trend_15_35"),
        "signed metrics stay internal (R-2)"
    );
    let tt = input
        .metrics
        .iter()
        .find(|m| m.key == "time_trouble_rate")
        .expect("time_trouble_rate");
    assert_eq!(tt.high, Some(1.0));
    assert_eq!(tt.low, Some(0.8));
    assert_eq!(input.validate(), Ok(()));
    assert_eq!(
        schema_errors(&ts_validator(), &input.to_json()),
        Vec::<String>::new()
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn long_line_key_maps_within_64_chars(pool: PgPool) {
    let _ = pool;
    let long = format!("white:{}", "a-very-long-opening-family-name".repeat(3));
    let k = contract_line_key(&long);
    assert!(k.chars().count() <= 46, "{k}");
    assert!(k.starts_with("white:a-very-long-opening"));
    assert_eq!(
        contract_line_key("black:sicilian-defense"),
        "black:sicilian-defense"
    );
    assert_eq!(contract_line_key(&long), k, "deterministic");
}

// ------------------------------------------------------------------ strengths

fn w3_stat() -> BandReferenceStat {
    BandReferenceStat {
        metric: MetricId::Weakness(WeaknessKey::W3Shallow),
        median: 0.03,
        p25: 0.02,
        p40: 0.026,
        p75: 0.04,
        pooled_rate: Some(0.03),
        epl_per_event: Some(0.2),
        opps_per_game: Some(40.0),
        opp_slope_per_100: Some(0.0),
        n_players: 200,
        n_games: 2000,
        low_sample: false,
    }
}

/// W3.shallow at 4 / 1600 against a band median of 0.03, with found tactics (`4. Qxf7#`) in
/// two games.
async fn seed_with_strength(pool: &PgPool) -> Fixture {
    let f = seed_with_moves(
        pool,
        SeedOptions {
            games: 40,
            reference: false,
            ..SeedOptions::default()
        },
    )
    .await;
    common::seed::insert_reference(pool, &[w1_stat(), w3_stat()]).await;
    for (i, g) in f.games.iter().enumerate() {
        let played_at: OffsetDateTime =
            sqlx::query_scalar("SELECT played_at FROM games WHERE id = $1")
                .bind(g)
                .fetch_one(pool)
                .await
                .unwrap();
        sqlx::query(
            "INSERT INTO weakness_series_point (account_id, time_class, series_key, game_id, \
               played_at, events, opportunities, epl, protocol) \
             VALUES ($1, 'blitz', 'W3.shallow', $2, $3, $4, 40, 0, $5)",
        )
        .bind(f.account_id)
        .bind(g)
        .bind(played_at)
        .bind(i32::from(i % 10 == 0))
        .bind(PROTOCOL)
        .execute(pool)
        .await
        .unwrap();
        if i == 3 || i == 33 {
            set_metrics_path(
                pool,
                *g,
                &["strengths"],
                json!([{"kind": "tactic_found", "ply": 7}]),
            )
            .await;
        }
    }
    f
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn low_rate_weakness_with_found_tactics_is_a_strength(pool: PgPool) {
    let f = seed_with_strength(&pool).await;
    run_build(&pool, f.account_id).await;
    let input = coaching(&pool, f.account_id).await;
    let s = input
        .strengths
        .iter()
        .find(|s| s.area == AreaId::Tactics)
        .unwrap_or_else(|| panic!("a tactics strength in {:?}", input.strengths));
    assert_eq!(s.exemplars.len(), 2);
    for e in &s.exemplars {
        assert_eq!(e.played.san, "Qxf7#");
        assert_eq!(
            e.eval_after,
            coaching_input::Score::Cp(1000),
            "mate: terminal eval"
        );
    }
    // Never also a ranked weakness: its rate is below the band median (no excess).
    assert!(input.weaknesses.iter().all(|w| w.id != "W3"));
    assert_eq!(input.validate(), Ok(()));
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn every_strength_has_one_to_three_exemplars(pool: PgPool) {
    let f = seed_with_strength(&pool).await;
    run_build(&pool, f.account_id).await;
    let input = coaching(&pool, f.account_id).await;
    assert!(!input.strengths.is_empty());
    assert!(input.strengths.len() <= 4);
    for s in &input.strengths {
        assert!((1..=3).contains(&s.exemplars.len()), "{}", s.label);
    }
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn strength_has_exactly_contract_fields(pool: PgPool) {
    let f = seed_with_strength(&pool).await;
    run_build(&pool, f.account_id).await;
    let input = coaching(&pool, f.account_id).await;
    let v = input.to_json();
    let strengths = v["strengths"].as_array().unwrap();
    assert!(!strengths.is_empty());
    for s in strengths {
        let keys: BTreeSet<&str> = s.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(
            keys,
            BTreeSet::from(["area", "label", "metricKey", "exemplars"])
        );
    }
}

fn candidate(label: &str, percentile: Option<f64>, exemplars: usize) -> StrengthCandidate {
    let e = sample_exemplar();
    StrengthCandidate {
        area: AreaId::Tactics,
        label: label.to_owned(),
        kind: StrengthSource::Area,
        metric_key: Some(MetricId::Weakness(WeaknessKey::W3Shallow)),
        band_percentile: percentile,
        n: 100,
        exemplars: vec![e; exemplars],
    }
}

fn sample_exemplar() -> coaching_input::ExemplarPosition {
    let parsed = parse_pgn(SCHOLARS).unwrap();
    let p = &parsed.plies[6];
    let row = ExemplarRow {
        game_id: Uuid::nil(),
        platform_game_id: "abcd1234".into(),
        played_at: SEED_NOW,
        player_color: Color::White,
        ply: p.ply,
        color: p.color,
        san: p.san.clone(),
        uci: p.uci.clone(),
        fen_before: p.fen_before.clone(),
        best_uci: Some(p.uci.clone()),
        win_before: 90.0,
        win_after: 100.0,
        delta_w: 0.0,
        phase: "opening".into(),
        best_pv: vec![p.uci.clone()],
        refutation_pv: Vec::new(),
        best_tags: vec![MotifTag::Mate, MotifTag::MateIn1],
        refutation_tags: Vec::new(),
        eval_before: Some(chess_core::EngineScore::Mate(1)),
        eval_after: Some(chess_core::EngineScore::Cp(1000)),
    };
    to_exemplar_position(&row).expect("a valid row")
}

#[test]
fn played_best_move_has_no_best_and_white_pov_scores() {
    let e = sample_exemplar();
    assert_eq!(e.best, None, "played = best");
    assert_eq!(e.best_pv, vec!["h5f7"]);
    assert_eq!(e.eval_before, coaching_input::Score::Mate(1));
    assert_eq!(e.tags, vec!["mate", "mate_in_1"]);
    assert_eq!(e.game_id, "abcd1234");
}

/// EXP-0007 (#122): the coach never sees a hidden motif tag; the position keeps its move, its
/// eval swing and its best line.
#[test]
fn an_exemplar_carries_only_player_visible_tags() {
    let parsed = parse_pgn(SCHOLARS).unwrap();
    let p = &parsed.plies[6];
    let row = |best_tags: Vec<MotifTag>, refutation_tags: Vec<MotifTag>| ExemplarRow {
        game_id: Uuid::nil(),
        platform_game_id: "abcd1234".into(),
        played_at: SEED_NOW,
        player_color: Color::White,
        ply: p.ply,
        color: p.color,
        san: p.san.clone(),
        uci: p.uci.clone(),
        fen_before: p.fen_before.clone(),
        best_uci: Some(p.uci.clone()),
        win_before: 90.0,
        win_after: 100.0,
        delta_w: 0.0,
        phase: "opening".into(),
        best_pv: vec![p.uci.clone()],
        refutation_pv: Vec::new(),
        best_tags,
        refutation_tags,
        eval_before: Some(chess_core::EngineScore::Mate(1)),
        eval_after: Some(chess_core::EngineScore::Cp(1000)),
    };
    let mixed = to_exemplar_position(&row(
        vec![MotifTag::Sacrifice, MotifTag::Mate, MotifTag::BackRankMate],
        vec![MotifTag::HangingPiece, MotifTag::Fork],
    ))
    .expect("a valid row");
    assert_eq!(mixed.tags, vec!["mate", "fork"]);
    let hidden_only = to_exemplar_position(&row(
        vec![MotifTag::Pin, MotifTag::Skewer],
        vec![MotifTag::Deflection],
    ))
    .expect("a position whose tags are all hidden is still a position");
    assert!(hidden_only.tags.is_empty(), "{:?}", hidden_only.tags);
    assert_eq!(hidden_only.eval_before, coaching_input::Score::Mate(1));
    assert_eq!(hidden_only.best_pv, vec!["h5f7"]);
}

#[test]
fn select_strengths_orders_by_percentile_caps_at_4_and_drops_empty() {
    let picked = select_strengths(vec![
        candidate("a", Some(70.0), 1),
        candidate("b", Some(95.0), 5),
        candidate("none", Some(99.0), 0),
        candidate("c", Some(80.0), 2),
        candidate("d", None, 1),
        candidate("e", Some(75.0), 1),
    ]);
    let labels: Vec<&str> = picked.iter().map(|s| s.label.as_str()).collect();
    assert_eq!(labels, vec!["b", "c", "e", "a"]);
    assert_eq!(picked[0].exemplars.len(), 3, "at most 3 exemplars");
    assert_eq!(picked[0].metric_key.as_deref(), Some("weakness:W3.shallow"));
}

// ------------------------------------------------------------------ R3 T8 and T12 rulings

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn weakness_without_an_exemplar_is_left_out(pool: PgPool) {
    let f = seed_with_moves(&pool, SeedOptions::default()).await;
    sqlx::query(
        "UPDATE game_metrics SET metrics = jsonb_set(metrics, '{weakness,W1,exemplarPlies}', \
         '[]') WHERE account_id = $1",
    )
    .bind(f.account_id)
    .execute(&pool)
    .await
    .unwrap();
    run_build(&pool, f.account_id).await;
    let input = coaching(&pool, f.account_id).await;
    assert!(input.weaknesses.is_empty(), "{:?}", input.weaknesses);
    assert_eq!(input.validate(), Ok(()));
}

/// ADR 0101 (#12): W1 at the band's own rate (1 in 50 against a median of 0.02) is measured,
/// with events and a benchmark, but its interval holds the median: nothing stands out, so
/// nothing is ranked, and the input still validates.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_player_at_the_benchmark_has_no_ranked_weakness(pool: PgPool) {
    fn at_median(_: usize) -> GameShape {
        GameShape {
            w1_events: 1,
            ..WHITE_GAME
        }
    }
    let f = seed_with_moves(
        &pool,
        SeedOptions {
            shape: at_median,
            ..SeedOptions::default()
        },
    )
    .await;
    run_build(&pool, f.account_id).await;
    let input = coaching(&pool, f.account_id).await;
    assert!(input.weaknesses.is_empty(), "{:?}", input.weaknesses);
    assert_eq!(input.validate(), Ok(()));
    let (priority, band_median): (f64, Option<f64>) = sqlx::query_as(
        "SELECT priority, band_median FROM weakness_estimates          WHERE account_id = $1 AND side = 'both' AND series_key = 'W1'",
    )
    .bind(f.account_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(priority, 0.0, "not enough evidence yet: no priority");
    assert!(band_median.is_some(), "still compared");
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn zero_event_weakness_is_never_emitted(pool: PgPool) {
    let f = seed_with_moves(&pool, SeedOptions::default()).await;
    sqlx::query("UPDATE weakness_series_point SET events = 0 WHERE account_id = $1")
        .bind(f.account_id)
        .execute(&pool)
        .await
        .unwrap();
    let input = coaching(&pool, f.account_id).await;
    assert!(input.weaknesses.is_empty(), "{:?}", input.weaknesses);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn stale_metrics_are_no_evidence(pool: PgPool) {
    let f = seed_with_moves(&pool, SeedOptions::default()).await;
    sqlx::query(
        "UPDATE game_metrics SET metrics = jsonb_set(metrics, '{version}', '1') WHERE game_id = $1",
    )
    .bind(f.games[0])
    .execute(&pool)
    .await
    .unwrap();
    // The builder compares with the benchmark the last build stored (ADR 0080).
    run_build(&pool, f.account_id).await;
    let input = coaching(&pool, f.account_id).await;
    assert_eq!(input.basis.analysed_games, 39);
    assert_eq!(input.basis.imported_games, 40);
    let w1 = input.weaknesses.iter().find(|w| w.id == "W1").unwrap();
    assert_eq!(w1.opportunities, 39 * 50);
}

// ------------------------------------------------------------------ changes

async fn insert_onset_row(
    pool: &PgPool,
    account_id: Uuid,
    at_game: Uuid,
    onset_game: Uuid,
    onset_at: OffsetDateTime,
) {
    let causes = json!([{
        "kind": "rating_level",
        "description": "Opponents were rated higher after the change",
        "before": 1500.0,
        "after": 1650.0,
    }]);
    sqlx::query(
        "INSERT INTO weakness_state_history (account_id, time_class, series_key, kind, \
           from_state, to_state, at_game_id, at_played_at, onset_game_id, onset_at, onset_low, \
           onset_high, p_change, rate_before, rate_after, evidence, cause_hints, \
           detector_version, protocol) \
         VALUES ($1, 'blitz', 'W1', 'onset', 'not_an_issue', 'active', $2, $4, $3, $4, \
           $4 - interval '1 day', $4 + interval '1 day', 0.9, 0.02, 0.04, \
           '{\"k\": 1, \"n\": 10, \"confidence\": 0.9}', $5, \
           $6, $7)",
    )
    .bind(account_id)
    .bind(at_game)
    .bind(onset_game)
    .bind(onset_at)
    .bind(causes)
    .bind(profile_core::timeline::states::DETECTOR_VERSION)
    .bind(PROTOCOL)
    .execute(pool)
    .await
    .unwrap();
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn changes_come_from_recent_onset_rows_with_causes_and_examples(pool: PgPool) {
    let (f, _) = built_fixture(&pool).await;
    // Onset at game 20 (2026-11-17), so exemplar game 5 is before it and 30, 35 after it.
    let onset_at: OffsetDateTime = sqlx::query_scalar("SELECT played_at FROM games WHERE id = $1")
        .bind(f.games[20])
        .fetch_one(&pool)
        .await
        .unwrap();
    insert_onset_row(&pool, f.account_id, f.games[25], f.games[20], onset_at).await;
    // An onset older than 90 days is not a recent change.
    insert_onset_row(
        &pool,
        f.account_id,
        f.games[25],
        f.games[20],
        SEED_NOW - TimeDuration::days(120),
    )
    .await;
    let input = coaching(&pool, f.account_id).await;
    let recent: Vec<&coaching_input::TimelineChange> = input
        .changes
        .iter()
        .filter(|c| c.onset_date == onset_at.date())
        .collect();
    assert_eq!(recent.len(), 1, "{:?}", input.changes);
    assert!(
        input
            .changes
            .iter()
            .all(|c| c.onset_date >= (SEED_NOW - TimeDuration::days(90)).date())
    );
    let c = recent[0];
    assert_eq!(c.series_key, "W1");
    assert_eq!(c.weakness_id.as_deref(), Some("W1"));
    assert_eq!(
        (c.from_state, c.to_state),
        (WeaknessState::NotAnIssue, WeaknessState::Active)
    );
    assert_eq!(c.candidate_causes.len(), 1);
    assert_eq!(
        (c.rate_before, c.rate_after),
        (0.02f32 as f64, 0.04f32 as f64)
    );
    let platform_id: String =
        sqlx::query_scalar("SELECT platform_game_id FROM games WHERE id = $1")
            .bind(f.games[20])
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(c.onset_game_id, platform_id, "R-7: platform game ids");
    let sides: Vec<ExampleSide> = c.examples.iter().map(|e| e.side).collect();
    assert_eq!(
        sides.iter().filter(|s| **s == ExampleSide::Before).count(),
        1
    );
    assert_eq!(
        sides.iter().filter(|s| **s == ExampleSide::After).count(),
        2
    );
    assert_eq!(input.validate(), Ok(()));
}

// ------------------------------------------------------------------ T2 and T33 hand-offs

#[test]
fn every_weakness_contract_id_matches_the_pattern() {
    let re = regex_lite(WEAKNESS_ID_PATTERN);
    for k in WeaknessKey::ALL {
        let (id, variant) = k.contract_id();
        assert!(re(id), "{id}");
        assert!(id.chars().count() <= WEAKNESS_ID_MAX_CHARS);
        if let Some(v) = variant {
            assert!((1..=LIMITS.tag_chars).contains(&v.chars().count()), "{v}");
        }
    }
    for t in MotifTag::ALL {
        let tag = t.contract_tag();
        assert!(
            (1..=LIMITS.tag_chars).contains(&tag.chars().count()),
            "{tag}"
        );
    }
}

/// `^W(1[0-2]|[1-9])[a-d]?$` without a regex dependency.
fn regex_lite(pattern: &str) -> impl Fn(&str) -> bool {
    assert_eq!(
        pattern, "^W(1[0-2]|[1-9])[a-d]?$",
        "pattern changed: update the check"
    );
    |s: &str| {
        let Some(rest) = s.strip_prefix('W') else {
            return false;
        };
        let rest = rest.strip_suffix(['a', 'b', 'c', 'd']).unwrap_or(rest);
        matches!(
            rest,
            "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "10" | "11" | "12"
        )
    }
}

#[test]
fn series_key_maps_to_weakness_id() {
    let cases: [(&str, Option<&str>); 7] = [
        ("W1", Some("W1")),
        ("W3.shallow", Some("W3")),
        ("W9a", Some("W9a")),
        ("motif.missed:fork", Some("W3")),
        ("motif.allowed:fork", Some("W1")),
        ("opening:white:italian-game", None),
        ("endgame:rook", None),
    ];
    for (key, id) in cases {
        let k: SeriesKey = key.parse().unwrap();
        assert_eq!(weakness_id_of_series(&k).as_deref(), id, "{key}");
    }
}

#[test]
fn platform_ids_map_to_contract_platforms() {
    assert_eq!(contract_platform(PlatformId::Lichess), Platform::Lichess);
    assert_eq!(contract_platform(PlatformId::Chesscom), Platform::Chesscom);
}

fn row(kind: RowKind, from: Option<WeaknessState>, to: WeaknessState, day: u8) -> TimelineRow {
    let at = datetime!(2026-11-01 12:00 UTC) + TimeDuration::days(i64::from(day));
    TimelineRow {
        kind,
        from,
        to,
        at_game_id: Uuid::from_u128(u128::from(day)),
        at_played_at: at,
        onset: (kind == RowKind::Onset).then(|| Onset {
            game_id: Uuid::from_u128(1000 + u128::from(day)),
            played_at: at - TimeDuration::days(2),
            low: at - TimeDuration::days(3),
            high: at - TimeDuration::days(1),
        }),
        p_change: None,
        rate_before: None,
        rate_after: None,
        evidence: Evidence {
            k: 1,
            n: 10,
            confidence: 0.9,
        },
    }
}

/// T33 N3: an episode entered without its own onset row names no onset, not an earlier
/// episode's.
#[test]
fn onset_at_names_only_the_current_episode() {
    use WeaknessState::{Active, Improving, NotAnIssue};
    let key = SeriesKey::Weakness(WeaknessKey::W1);
    let mut outcomes = BTreeMap::new();
    outcomes.insert(
        key.clone(),
        SeriesOutcome {
            state: Active,
            rows: vec![
                row(RowKind::Transition, None, NotAnIssue, 1),
                row(RowKind::Transition, Some(NotAnIssue), Active, 5),
                row(RowKind::Onset, Some(NotAnIssue), Active, 5),
                row(RowKind::Transition, Some(Active), Improving, 9),
                row(RowKind::Transition, Some(Improving), Active, 12),
            ],
            p_imp: None,
            invalid_points: 0,
        },
    );
    let stored = HashMap::from([("W1".to_owned(), Improving)]);
    let changes = state_changes(&outcomes, &stored, &StoredHistory::default());
    let w1 = changes.iter().find(|c| c.key == key).expect("W1 changed");
    assert_eq!(w1.to, Active);
    assert_eq!(
        w1.onset_at, None,
        "the day-5 onset belongs to an earlier episode"
    );

    // The episode's own onset row (same game as its transition) is still named.
    outcomes.get_mut(&key).unwrap().rows.truncate(3);
    let stored = HashMap::from([("W1".to_owned(), NotAnIssue)]);
    let changes = state_changes(&outcomes, &stored, &StoredHistory::default());
    let w1 = changes.iter().find(|c| c.key == key).unwrap();
    assert_eq!(
        w1.onset_at,
        Some(datetime!(2026-11-04 12:00 UTC)),
        "onset of the current episode"
    );
}

// ------------------------------------------------------------------ coordinator rulings

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn report_carries_the_snapshot_time_and_now(pool: PgPool) {
    let f = seed_with_moves(&pool, SeedOptions::default()).await;
    run_build(&pool, f.account_id).await;
    let latest: OffsetDateTime = sqlx::query_scalar(
        "SELECT max(computed_at) FROM kpi_snapshot WHERE account_id = $1 AND window_key = 'last50'",
    )
    .bind(f.account_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let r = build_coaching_input_report(&pool, f.account_id, TimeClass::Blitz, SEED_NOW)
        .await
        .unwrap();
    assert_eq!(r.input.generated_at, SEED_NOW);
    assert_eq!(r.snapshots_computed_at, Some(latest));
    assert_eq!(r.skipped_exemplars, 0);

    let fresh = testkit::insert_account(&pool, "lichess", "nosnapshots").await;
    let r = build_coaching_input_report(&pool, fresh, TimeClass::Blitz, SEED_NOW)
        .await
        .unwrap();
    assert_eq!(r.snapshots_computed_at, None, "no snapshot used");
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn undecodable_exemplar_is_skipped_and_counted(pool: PgPool) {
    let f = seed_with_moves(&pool, SeedOptions::default()).await;
    // The best exemplar's stored move is not legal in its position: no position can be stated.
    sqlx::query("UPDATE moves SET uci = 'a1a8' WHERE game_id = $1 AND ply = $2")
        .bind(f.games[EXEMPLAR_GAMES[0]])
        .bind(i32::try_from(EXEMPLAR_PLY).unwrap())
        .execute(&pool)
        .await
        .unwrap();
    run_build(&pool, f.account_id).await;
    let r = build_coaching_input_report(&pool, f.account_id, TimeClass::Blitz, SEED_NOW)
        .await
        .unwrap();
    let w1 = r.input.weaknesses.iter().find(|w| w.id == "W1").unwrap();
    assert_eq!(w1.exemplars.len(), 2, "the other two, nothing invented");
    assert!(w1.exemplars.iter().all(|e| e.played.san == "Bc4"));
    assert!(r.skipped_exemplars >= 1, "counted: {}", r.skipped_exemplars);
}

// ------------------------------------------------------------------ fix round 1 (T34 review)

/// The seed's W1 (2 / 50, with exemplars) against a band whose W1 median is 0.08: the player is
/// better than most of the band.
fn w1_high_band() -> BandReferenceStat {
    BandReferenceStat {
        median: 0.08,
        p25: 0.06,
        p40: 0.072,
        p75: 0.1,
        pooled_rate: Some(0.08),
        ..w1_stat()
    }
}

async fn seed_below_median(pool: &PgPool) -> Fixture {
    let f = seed_with_moves(
        pool,
        SeedOptions {
            games: 40,
            reference: false,
            ..SeedOptions::default()
        },
    )
    .await;
    common::seed::insert_reference(pool, &[w1_high_band()]).await;
    f
}

/// Review I1 (coordinator ruling 1): a claimable estimate below the band median has priority 0
/// and is never a weakness, even with exemplars.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn below_median_weakness_is_never_ranked(pool: PgPool) {
    let f = seed_below_median(&pool).await;
    run_build(&pool, f.account_id).await;
    let input = coaching(&pool, f.account_id).await;
    assert!(
        input.weaknesses.iter().all(|w| w.id != "W1"),
        "{:?}",
        input.weaknesses
    );
    assert_eq!(input.validate(), Ok(()));
}

/// Review m2: a weakness strength cites only exemplars of its own claim. "Few errors: Hung
/// material" has no matching strength kind, so found only-moves (a calculation-area kind) never
/// stand in for it, and the strength is dropped.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn weakness_strength_needs_exemplars_of_its_own_claim(pool: PgPool) {
    let f = seed_below_median(&pool).await;
    for i in [3usize, 33] {
        set_metrics_path(
            &pool,
            f.games[i],
            &["strengths"],
            json!([{"kind": "only_move_found", "ply": 7}]),
        )
        .await;
    }
    run_build(&pool, f.account_id).await;
    let input = coaching(&pool, f.account_id).await;
    assert!(
        input
            .strengths
            .iter()
            .all(|s| !s.label.contains("Leaving pieces hanging")),
        "{:?}",
        input.strengths
    );
}

/// Review m1: when the latest transition does not enter the state asked about, no onset is
/// named (fail closed), even if an older onset row led into that state.
#[test]
fn onset_fails_closed_when_the_last_transition_enters_another_state() {
    use WeaknessState::{Active, Improving, NotAnIssue};
    let key = SeriesKey::Weakness(WeaknessKey::W1);
    let mut outcomes = BTreeMap::new();
    outcomes.insert(
        key.clone(),
        SeriesOutcome {
            // Inconsistent on purpose: the state says active, the last transition improving.
            state: Active,
            rows: vec![
                row(RowKind::Transition, None, NotAnIssue, 1),
                row(RowKind::Transition, Some(NotAnIssue), Active, 5),
                row(RowKind::Onset, Some(NotAnIssue), Active, 5),
                row(RowKind::Transition, Some(Active), Improving, 9),
            ],
            p_imp: None,
            invalid_points: 0,
        },
    );
    let stored = HashMap::from([("W1".to_owned(), Improving)]);
    let changes = state_changes(&outcomes, &stored, &StoredHistory::default());
    let w1 = changes.iter().find(|c| c.key == key).expect("W1 changed");
    assert_eq!(w1.onset_at, None);

    // No transition at all: nothing to date an episode by, so no onset either.
    outcomes.get_mut(&key).unwrap().rows = vec![row(RowKind::Onset, None, Active, 5)];
    let changes = state_changes(&outcomes, &stored, &StoredHistory::default());
    let w1 = changes.iter().find(|c| c.key == key).unwrap();
    assert_eq!(w1.onset_at, None);
}

/// Review m6: a row skipped by several lists counts once.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_skipped_row_counts_once(pool: PgPool) {
    let f = seed_with_moves(&pool, SeedOptions::default()).await;
    sqlx::query("UPDATE moves SET uci = 'a1a8' WHERE game_id = $1 AND ply = $2")
        .bind(f.games[EXEMPLAR_GAMES[0]])
        .bind(i32::try_from(EXEMPLAR_PLY).unwrap())
        .execute(&pool)
        .await
        .unwrap();
    run_build(&pool, f.account_id).await;
    // Tried by the W1 weakness and by the opening line's examples.
    let r = build_coaching_input_report(&pool, f.account_id, TimeClass::Blitz, SEED_NOW)
        .await
        .unwrap();
    assert_eq!(r.skipped_exemplars, 1);
}

/// Review m4: a change on an opening series whose key is over 64 characters is kept, with the
/// key mapped by R-7.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn long_opening_series_key_is_mapped_not_dropped(pool: PgPool) {
    let (f, _) = built_fixture(&pool).await;
    let onset_at: OffsetDateTime = sqlx::query_scalar("SELECT played_at FROM games WHERE id = $1")
        .bind(f.games[20])
        .fetch_one(&pool)
        .await
        .unwrap();
    insert_onset_row(&pool, f.account_id, f.games[25], f.games[20], onset_at).await;
    let line = format!("white:{}", "a-very-long-opening-family-name".repeat(3));
    sqlx::query("UPDATE weakness_state_history SET series_key = $2 WHERE account_id = $1 AND kind = 'onset'")
        .bind(f.account_id)
        .bind(format!("opening:{line}"))
        .execute(&pool)
        .await
        .unwrap();
    let input = coaching(&pool, f.account_id).await;
    let c = input
        .changes
        .iter()
        .find(|c| c.series_key.starts_with("opening:"))
        .unwrap_or_else(|| panic!("the change is kept: {:?}", input.changes));
    assert_eq!(
        c.series_key,
        format!("opening:{}", contract_line_key(&line))
    );
    assert!(c.series_key.chars().count() <= LIMITS.id_chars);
    assert_eq!(c.weakness_id, None);
}

/// Same database, same `now`: the same input (review, determinism).
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn same_data_and_now_give_the_same_input(pool: PgPool) {
    let f = seed_with_strength(&pool).await;
    run_build(&pool, f.account_id).await;
    let a = coaching(&pool, f.account_id).await;
    let b = coaching(&pool, f.account_id).await;
    assert_eq!(a.to_json(), b.to_json());
}

// ------------------------------------------------------------------ v3 trajectory (ADR 0089)

/// v3: the trajectory covers every rated game of the time class with its pre-game rating (on
/// Lichess the stored one), finds the peak, and compares the games around it with the latest.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn trajectory_finds_the_peak_and_compares_it_with_the_latest_games(pool: PgPool) {
    let f = seed_with_moves(&pool, SeedOptions::default()).await;
    // Up 10 a game to a peak at the middle game, then down 30 a game.
    sqlx::query(
        "WITH o AS (SELECT id, row_number() OVER (ORDER BY played_at, id) AS i,                 count(*) OVER () AS n FROM games WHERE account_id = $1 AND rated)          UPDATE games g SET white_rating = r.v, black_rating = r.v          FROM (SELECT id, (CASE WHEN i <= n / 2 THEN 1000 + i * 10                 ELSE 1000 + (n / 2) * 10 - (i - n / 2) * 30 END)::int AS v FROM o) r          WHERE r.id = g.id",
    )
    .bind(f.account_id)
    .execute(&pool)
    .await
    .unwrap();
    let rated: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM games WHERE account_id = $1 AND time_class = 'blitz' AND rated",
    )
    .bind(f.account_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    run_build(&pool, f.account_id).await;
    let input = coaching(&pool, f.account_id).await;
    assert_eq!(input.input_version, coaching_input::InputVersion::V3);
    let t = input.trajectory.as_ref().expect("v3 carries a trajectory");
    assert_eq!(i64::from(t.rated_games), rated);
    if rated < 20 {
        assert!(!t.sufficient && t.then_vs_now.is_none());
        return;
    }
    let peak = i32::try_from(1000 + (rated / 2) * 10).unwrap();
    assert_eq!(t.peak.as_ref().map(|p| i32::from(p.rating)), Some(peak));
    let v = t.then_vs_now.as_ref().unwrap();
    // Since algo-6.0.0 (EXP-0030, ADR 0117): the first games against the last, under the rule
    // that cleared EXP-0030, whose verdict the coach is told.
    assert_eq!(v.then.basis, coaching_input::WindowBasis::Start);
    assert_eq!(v.now.basis, coaching_input::WindowBasis::Last);
    const { assert!(coaching_input::trajectory::RATING_VERDICT_SHOWN) };
    assert_eq!(
        t.rating_rule.as_deref(),
        Some(coaching_input::trajectory::CLEARED_RULE)
    );
    assert!(t.kind.is_some());
    assert_ne!(v.rating.verdict, coaching_input::ChangeVerdict::Withheld);
    assert!(v.rating.diff_low.is_some() && v.rating.diff_high.is_some());
    assert_eq!(v.rating.then.unit, coaching_input::MetricUnit::Elo);
    // The change is the difference the verdict tests: the windows' averages.
    let (a, b) = (v.rating.then.value.unwrap(), v.rating.now.value.unwrap());
    assert_eq!(t.change, Some((b - a + 0.5).floor() as i32));
    assert_eq!(
        t.months.iter().map(|m| i64::from(m.games)).sum::<i64>(),
        rated
    );
    assert_eq!(input.validate(), Ok(()));
}

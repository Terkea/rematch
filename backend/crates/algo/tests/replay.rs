//! The replay suite (ADR 0104 section 2, issue #54): the same games and the same algorithm give
//! byte-identical facts, profile rows and recommendations.
//!
//! The fixture games (`backend/fixtures/pgn`, the Lichess sample, the Lichess API and chess.com
//! month fixtures) go through the production import mapping, `analyse-game` with the profile
//! hook (enrichment, move features, metrics, series), `build-profile` and the CoachingInput
//! builder, twice, in two throwaway databases. Each run is dumped in canonical form: rows sorted,
//! wall-clock columns, row ids and the algorithm id removed, UUIDs replaced by natural keys,
//! floats as shortest round-trip decimals. The two dumps must be equal, and equal to the goldens
//! in `backend/fixtures/replay/`.
//!
//! `REPLAY_UPDATE=1` rewrites the goldens, but only when they are missing, unchanged, or written
//! by an older major version: facts, profile or recommendations may change only with a major
//! bump of `algo::VERSION` (`algorithm/CHANGELOG.md`). `scripts/algorithm/check.mjs` holds pull
//! requests to the same rule.
//!
//! The engine is the real Stockfish at [`REPLAY_NODES`] nodes per search through the benchmark's
//! memo (one thread, `ucinewgame` before every search), so the suite runs in CI in about a
//! minute. Tablebases and Maia are off: the suite must give the same bytes on any machine.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use analysis::eval_cache::DbEvalCache;
use analysis::jobs::analyse::{AnalyseGameDeps, handle_analyse_game};
use bench::memo::MemoAnalyser;
use chess_core::TimeClass;
use db::games::{AnalysisStatus, NewGame, upsert_game};
use engine::Analyser;
use engine::pool::{EnginePool, PoolOptions};
use enrich::enrich_game::EnrichDeps;
use enrich::store::DbEnrichmentStore;
use platforms::types::NormalizedGame;
use profile::build_profile::{BuildProfileDeps, handle_build_profile};
use profile::coaching::build_coaching_input_report;
use profile::hook_impl::{ProfileHook, into_hook, warm_openings};
use profile::peer_benchmark::BenchmarkSource;
use queue::queue::Queue;
use queue::{AnalyseGamePayload, BuildProfilePayload, Job, KIND_ANALYSE};
use serde_json::Value;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use time::OffsetDateTime;
use time::macros::datetime;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

/// Nodes per search: small enough for CI, recorded in the golden header.
pub const REPLAY_NODES: u64 = 3_000;
/// The clock of every build: after the newest fixture game.
const NOW: OffsetDateTime = datetime!(2026-11-21 00:00:00 UTC);
/// The anchor of a PGN without a date: game `i` is played `i` minutes after it.
const PGN_ANCHOR: OffsetDateTime = datetime!(2026-09-01 12:00:00 UTC);

fn replay_now() -> OffsetDateTime {
    NOW
}

fn fixtures() -> PathBuf {
    algo::repo_root().join("backend/fixtures")
}

fn golden_dir() -> PathBuf {
    fixtures().join("replay")
}

// ---- Import --------------------------------------------------------------------------------

/// The PGN header `name`'s value.
fn header<'a>(pgn: &'a str, name: &str) -> Option<&'a str> {
    let prefix = format!("[{name} \"");
    pgn.lines()
        .find_map(|l| l.trim().strip_prefix(prefix.as_str()))
        .and_then(|r| r.strip_suffix("\"]"))
}

fn pgn_played_at(pgn: &str, index: usize) -> OffsetDateTime {
    let date = header(pgn, "UTCDate").or_else(|| header(pgn, "Date"));
    let time = header(pgn, "UTCTime").unwrap_or("00:00:00");
    let parsed = date.and_then(|d| {
        let f = time::macros::format_description!(
            "[year].[month].[day] [hour]:[minute]:[second]"
        );
        time::PrimitiveDateTime::parse(&format!("{d} {time}"), f).ok()
    });
    match parsed {
        Some(p) => p.assume_utc(),
        None => PGN_ANCHOR + time::Duration::minutes(index as i64),
    }
}

fn pgn_time_control(pgn: &str) -> (Option<i32>, Option<i32>, TimeClass) {
    let tc = header(pgn, "TimeControl").unwrap_or("");
    let mut parts = tc.split('+');
    let base = parts.next().and_then(|b| b.parse::<u32>().ok());
    let inc = parts.next().and_then(|i| i.parse::<u32>().ok()).unwrap_or(0);
    match base {
        Some(b) => (
            Some(b as i32),
            Some(inc as i32),
            chess_core::time_control::classify_time_control(b, inc),
        ),
        None => (None, None, TimeClass::Rapid),
    }
}

/// The games of a PGN file (one or more, separated by a blank line before `[Event`).
fn split_pgn(text: &str) -> Vec<String> {
    let text = text.replace("\r\n", "\n");
    let mut out = Vec::new();
    let mut cur = String::new();
    for line in text.lines() {
        if line.starts_with("[Event ") && cur.contains("\n\n") {
            out.push(cur.trim().to_owned());
            cur.clear();
        }
        cur.push_str(line);
        cur.push('\n');
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_owned());
    }
    out
}

fn pgn_game(account_id: Uuid, id: String, pgn: String, index: usize) -> NewGame {
    let (base, inc, tc) = pgn_time_control(&pgn);
    let rating = |h| header(&pgn, h).and_then(|r| r.parse::<i32>().ok());
    NewGame {
        account_id,
        platform: "chesscom".to_owned(),
        platform_game_id: id,
        played_at: pgn_played_at(&pgn, index),
        time_class: tc.as_str().to_owned(),
        base_seconds: base,
        increment_seconds: inc,
        rated: true,
        player_color: "white".to_owned(),
        white_name: header(&pgn, "White").unwrap_or("white").to_owned(),
        black_name: header(&pgn, "Black").unwrap_or("black").to_owned(),
        white_rating: rating("WhiteElo"),
        black_rating: rating("BlackElo"),
        result: header(&pgn, "Result").unwrap_or("*").to_owned(),
        termination: header(&pgn, "Termination").map(str::to_owned),
        eco: header(&pgn, "ECO").map(str::to_owned),
        opening_name: None,
        lichess_evals: None,
        status: AnalysisStatus::Pending,
        ply_count: Some(analysis::jobs::import::count_plies(&pgn)),
        pgn,
    }
}

/// The importer's mapping (`analysis::jobs::import`), with the player always White.
fn normalized_game(account_id: Uuid, g: &NormalizedGame) -> NewGame {
    NewGame {
        account_id,
        platform: g.platform.as_str().to_owned(),
        platform_game_id: g.platform_game_id.clone(),
        pgn: g.pgn.clone(),
        played_at: g.played_at,
        time_class: g.time_class.as_str().to_owned(),
        base_seconds: g.base_seconds,
        increment_seconds: g.increment_seconds,
        rated: g.rated,
        player_color: "white".to_owned(),
        white_name: g.white.name.clone(),
        black_name: g.black.name.clone(),
        white_rating: g.white.rating,
        black_rating: g.black.rating,
        result: g.result.clone(),
        termination: g.termination.clone(),
        eco: g.eco.clone(),
        opening_name: g.opening_name.clone(),
        lichess_evals: g
            .lichess_evals
            .as_ref()
            .and_then(|e| serde_json::to_value(e).ok()),
        status: AnalysisStatus::Pending,
        ply_count: Some(analysis::jobs::import::count_plies(&g.pgn)),
    }
}

/// The fixture games, in a fixed order, as (account username, platform, game).
fn fixture_games() -> Vec<(&'static str, &'static str, Box<dyn Fn(Uuid) -> NewGame>)> {
    let mut out: Vec<(&'static str, &'static str, Box<dyn Fn(Uuid) -> NewGame>)> = Vec::new();
    let root = fixtures();
    let mut pgn_files = Vec::new();
    for dir in ["pgn", "pgn/terminal", "lichess-db"] {
        let mut names: Vec<String> = std::fs::read_dir(root.join(dir))
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".pgn"))
            .collect();
        names.sort();
        pgn_files.extend(names.into_iter().map(|n| format!("{dir}/{n}")));
    }
    let mut index = 0;
    for f in pgn_files {
        let text = std::fs::read_to_string(root.join(&f)).unwrap();
        for (i, pgn) in split_pgn(&text).into_iter().enumerate() {
            let id = format!("{f}#{i}");
            let at = index;
            index += 1;
            out.push((
                "replay-pgn",
                "chesscom",
                Box::new(move |acct| pgn_game(acct, id.clone(), pgn.clone(), at)),
            ));
        }
    }
    let month: Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("chesscom-month.json")).unwrap())
            .unwrap();
    for g in month["games"].as_array().unwrap() {
        if let Some(n) = platforms::chesscom::normalize_chesscom(g) {
            out.push((
                "replay-chesscom",
                "chesscom",
                Box::new(move |acct| normalized_game(acct, &n)),
            ));
        }
    }
    let nd = std::fs::read_to_string(root.join("lichess-games.ndjson")).unwrap();
    for line in nd.lines().filter(|l| !l.trim().is_empty()) {
        let v: Value = serde_json::from_str(line).unwrap();
        if let Some(n) = platforms::lichess::normalize_lichess(&v) {
            out.push((
                "replay-lichess",
                "lichess",
                Box::new(move |acct| normalized_game(acct, &n)),
            ));
        }
    }
    out
}

// ---- The pipeline --------------------------------------------------------------------------

fn job_for(game_id: Uuid) -> Job {
    Job {
        id: 1,
        kind: KIND_ANALYSE.to_owned(),
        payload: serde_json::json!({ "gameId": game_id }),
        dedupe_key: None,
        user_key: None,
        priority: 100,
        attempts: 1,
        max_attempts: 3,
        lease_token: Uuid::nil(),
        run_at: NOW,
        created_at: NOW,
        started_at: NOW,
    }
}

fn parse_tc(s: &str) -> TimeClass {
    match s {
        "bullet" => TimeClass::Bullet,
        "blitz" => TimeClass::Blitz,
        "classical" => TimeClass::Classical,
        "daily" => TimeClass::Daily,
        _ => TimeClass::Rapid,
    }
}

/// One full run in `pool`; returns the analysis outcome of every game, by natural key.
async fn run_pipeline(pool: &PgPool, engines: Arc<EnginePool>) -> (BTreeMap<String, String>, Value) {
    warm_openings().await;
    let inner: Arc<dyn Analyser> = engines;
    let memo: Arc<dyn Analyser> = Arc::new(MemoAnalyser::new(inner, REPLAY_NODES));
    let mut accounts: BTreeMap<&str, Uuid> = BTreeMap::new();
    let mut games = Vec::new();
    for (user, platform, make) in fixture_games() {
        let acct = match accounts.get(user) {
            Some(a) => *a,
            None => {
                let a = testkit::insert_account(pool, platform, user).await;
                accounts.insert(user, a);
                a
            }
        };
        let g = make(acct);
        let key = g.platform_game_id.clone();
        let r = upsert_game(pool, &g).await.expect("insert game");
        games.push((key, r.id));
    }
    let mut deps = AnalyseGameDeps::new(
        pool.clone(),
        memo.clone(),
        Arc::new(DbEvalCache::new(pool.clone())),
    );
    deps.publish_events = false;
    deps.concurrency = 1;
    deps.hook = Some(into_hook(ProfileHook {
        pool: pool.clone(),
        enrich: EnrichDeps {
            analyser: memo,
            store: Arc::new(DbEnrichmentStore::new(pool.clone())),
            tb: None,
            difficulty: None,
            cancel: CancellationToken::new(),
        },
        queue: Queue::new(pool.clone()),
    }));
    let mut outcomes = BTreeMap::new();
    for (key, id) in &games {
        let r = handle_analyse_game(
            &deps,
            &job_for(*id),
            AnalyseGamePayload { game_id: *id },
            CancellationToken::new(),
        )
        .await;
        outcomes.insert(
            key.clone(),
            match r {
                Ok(_) => "ok".to_owned(),
                Err(e) => format!("error: {e}"),
            },
        );
    }
    let pairs: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT DISTINCT g.account_id, g.time_class::text FROM games g \
         JOIN linked_accounts a ON a.id = g.account_id ORDER BY 2, 1",
    )
    .fetch_all(pool)
    .await
    .unwrap();
    let build = BuildProfileDeps {
        pool: pool.clone(),
        queue: Queue::new(pool.clone()),
        now: replay_now,
        benchmark: BenchmarkSource::Peers,
    };
    let mut recs = BTreeMap::new();
    for (account, _, tc) in &pairs {
        handle_build_profile(
            &build,
            BuildProfilePayload {
                account_id: *account,
                time_class: tc.clone(),
            },
        )
        .await
        .expect("build-profile");
    }
    for (account, user, tc) in &pairs {
        let report = build_coaching_input_report(pool, *account, parse_tc(tc), NOW)
            .await
            .expect("coaching input");
        let drills: Vec<Value> = report
            .input
            .weaknesses
            .iter()
            .map(|w| {
                serde_json::json!({
                    "weakness": w.id,
                    "variant": w.variant,
                    "drills": ai::output::drills::drills_for(&w.id, w.variant.as_deref())
                        .into_iter()
                        .map(|d| d.as_str().to_owned())
                        .collect::<Vec<_>>(),
                })
            })
            .collect();
        recs.insert(
            format!("{user}/{tc}"),
            serde_json::json!({ "coachingInput": report.input.to_json(), "drills": drills }),
        );
    }
    (outcomes, serde_json::to_value(recs).unwrap())
}

// ---- The canonical dump --------------------------------------------------------------------

/// Columns that are not facts: wall-clock times, row ids, and the algorithm id (asserted apart).
const EXCLUDED: [&str; 16] = [
    "id",
    "created_at",
    "updated_at",
    "computed_at",
    "validated_at",
    "generated_at",
    "generatedAt",
    "analysed_at",
    "enriched_at",
    "imported_at",
    "started_at",
    "finished_at",
    "heartbeat_at",
    "run_at",
    "algorithm_id",
    "pgn",
];

fn is_uuid(s: &str) -> bool {
    Uuid::parse_str(s).is_ok() && s.len() == 36
}

/// Removes the excluded keys, replaces UUIDs by their natural key, and re-serialises numbers
/// in their shortest round-trip form.
fn normalise(v: &Value, names: &BTreeMap<String, String>) -> Value {
    match v {
        Value::Object(m) => Value::Object(
            m.iter()
                .filter(|(k, _)| !EXCLUDED.contains(&k.as_str()) && !k.ends_with("_ms"))
                .map(|(k, v)| (k.clone(), normalise(v, names)))
                .collect(),
        ),
        Value::Array(a) => Value::Array(a.iter().map(|x| normalise(x, names)).collect()),
        Value::String(s) if is_uuid(s) => Value::String(
            names
                .get(&s.to_lowercase())
                .cloned()
                .unwrap_or_else(|| "<uuid>".to_owned()),
        ),
        Value::String(s) => {
            // Keys like "<account uuid>/<time class>".
            let mut out = s.clone();
            for (id, name) in names {
                if out.contains(id.as_str()) {
                    out = out.replace(id.as_str(), name);
                }
            }
            Value::String(out)
        }
        Value::Number(n) => match n.as_f64() {
            Some(f) if !n.is_i64() && !n.is_u64() => serde_json::Number::from_f64(f)
                .map(Value::Number)
                .unwrap_or(Value::Null),
            _ => v.clone(),
        },
        _ => v.clone(),
    }
}

pub const FACTS: [&str; 9] = [
    "games",
    "position_evals",
    "position_multipv",
    "position_threats",
    "position_tb",
    "move_features",
    "game_metrics",
    "opponent_game_metrics",
    "weakness_series_point",
];
pub const PROFILE: [&str; 4] = [
    "weakness_estimates",
    "weakness_state_history",
    "kpi_snapshot",
    "account_benchmark_stat",
];
/// Tables whose every row must carry the build's algorithm id.
const STAMPED: [&str; 5] = [
    "game_metrics",
    "weakness_estimates",
    "weakness_state_history",
    "kpi_snapshot",
    "coaching_inputs",
];

async fn natural_names(pool: &PgPool) -> BTreeMap<String, String> {
    let mut names = BTreeMap::new();
    let games: Vec<(Uuid, String)> =
        sqlx::query_as("SELECT id, platform_game_id FROM games").fetch_all(pool).await.unwrap();
    for (id, key) in games {
        names.insert(id.to_string(), format!("game:{key}"));
    }
    let accts: Vec<(Uuid, String)> = sqlx::query_as("SELECT id, username FROM linked_accounts")
        .fetch_all(pool)
        .await
        .unwrap();
    for (id, name) in accts {
        names.insert(id.to_string(), format!("account:{name}"));
    }
    names
}

async fn dump_tables(pool: &PgPool, tables: &[&str], names: &BTreeMap<String, String>) -> String {
    let mut out = String::new();
    for t in tables {
        let rows: Vec<Value> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "SELECT to_jsonb(t) FROM {t} t"
        )))
        .fetch_all(pool)
        .await
        .unwrap();
        let mut lines: Vec<String> = rows.iter().map(|r| normalise(r, names).to_string()).collect();
        lines.sort();
        out.push_str(&format!("## {t} ({} rows)\n", lines.len()));
        for l in lines {
            out.push_str(&l);
            out.push('\n');
        }
    }
    out
}

struct Dump {
    facts: String,
    profile: String,
    recommendation: String,
}

async fn dump(pool: &PgPool, outcomes: &BTreeMap<String, String>, recs: &Value) -> Dump {
    let names = natural_names(pool).await;
    let mut facts = String::from("## analysis outcomes\n");
    for (k, v) in outcomes {
        facts.push_str(&format!("{k}: {v}\n"));
    }
    facts.push_str(&dump_tables(pool, &FACTS, &names).await);
    let profile = dump_tables(pool, &PROFILE, &names).await;
    let recommendation = {
        let n = normalise(recs, &names);
        let mut s = serde_json::to_string_pretty(&n).unwrap();
        s.push('\n');
        s
    };
    for t in STAMPED {
        let ids: Vec<Option<String>> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "SELECT DISTINCT algorithm_id FROM {t}"
        )))
        .fetch_all(pool)
        .await
        .unwrap();
        assert!(
            ids.iter().all(|i| i.as_deref() == Some(common::algorithm::id())),
            "{t} rows not stamped with {}: {ids:?}",
            common::algorithm::id()
        );
    }
    Dump {
        facts,
        profile,
        recommendation,
    }
}

// ---- Goldens -------------------------------------------------------------------------------

fn major(version: &str) -> u64 {
    version.split('.').next().and_then(|m| m.parse().ok()).unwrap_or(0)
}

fn header_line() -> String {
    format!(
        "# replay golden: algorithm version {}, {REPLAY_NODES} nodes per search, engine protocol {}, \
         regenerate with REPLAY_UPDATE=1 (major bumps only)\n",
        algo::VERSION,
        engine::ANALYSIS_PROTOCOL
    )
}

fn golden_version(text: &str) -> Option<String> {
    text.lines()
        .next()?
        .strip_prefix("# replay golden: algorithm version ")?
        .split(',')
        .next()
        .map(str::to_owned)
}

/// The first line where `a` and `b` differ, with the table it is in.
fn first_difference(a: &str, b: &str) -> String {
    let mut table = "";
    for (i, (x, y)) in a.lines().zip(b.lines()).enumerate() {
        if x.starts_with("## ") {
            table = x;
        }
        if x != y {
            return format!("line {} in `{table}`:\n  - {x}\n  + {y}", i + 1);
        }
    }
    format!(
        "one is a prefix of the other ({} vs {} lines)",
        a.lines().count(),
        b.lines().count()
    )
}

fn check_golden(name: &str, body: &str) {
    let path = golden_dir().join(name);
    let text = format!("{}{body}", header_line());
    let old = std::fs::read_to_string(&path).ok().map(|s| s.replace("\r\n", "\n"));
    let old_body = old.as_deref().map(|o| o.split_once('\n').map_or("", |x| x.1).to_owned());
    if std::env::var("REPLAY_UPDATE").as_deref() == Ok("1") {
        if let (Some(o), Some(ob)) = (old.as_deref(), old_body.as_deref())
            && ob != body
        {
            let v = golden_version(o).unwrap_or_default();
            assert!(
                major(&v) < major(algo::VERSION),
                "{name}: the replay output changed, but algo::VERSION {} has the same major as the \
                 golden ({v}). Facts, profile and recommendations change only with a major bump \
                 (algorithm/CHANGELOG.md).\n{}",
                algo::VERSION,
                first_difference(ob, body)
            );
        }
        std::fs::create_dir_all(golden_dir()).unwrap();
        std::fs::write(&path, &text).unwrap();
        return;
    }
    let ob = old_body.unwrap_or_else(|| {
        panic!("{} is missing: run the suite with REPLAY_UPDATE=1", path.display())
    });
    assert!(
        ob == body,
        "{name} differs from the golden: the algorithm's output changed. Bump the major version \
         (algo::VERSION, algorithm/CHANGELOG.md) and regenerate with REPLAY_UPDATE=1.\n{}",
        first_difference(&ob, body)
    );
}

// ---- The test ------------------------------------------------------------------------------

async fn second_database(a: &PgPool) -> (PgPool, String) {
    let name = format!("replay_b_{}", Uuid::new_v4().simple());
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE DATABASE {name}")))
        .execute(a)
        .await
        .unwrap();
    let opts = a.connect_options().as_ref().clone().database(&name);
    let b = PgPoolOptions::new()
        .max_connections(5)
        .connect_with(opts)
        .await
        .unwrap();
    db::MIGRATOR.run(&b).await.unwrap();
    (b, name)
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn the_same_games_give_the_same_bytes_twice(pool: PgPool) {
    let Some(path) = testkit::stockfish() else {
        return;
    };
    let engines = EnginePool::create(PoolOptions {
        size: 1,
        path,
        hash_mb: None,
    })
    .await
    .expect("engine pool");

    let (oa, ra) = run_pipeline(&pool, engines.clone()).await;
    let a = dump(&pool, &oa, &ra).await;

    let (b_pool, b_name) = second_database(&pool).await;
    let (ob, rb) = run_pipeline(&b_pool, engines.clone()).await;
    let b = dump(&b_pool, &ob, &rb).await;
    b_pool.close().await;
    let _ = sqlx::query(sqlx::AssertSqlSafe(format!(
        "DROP DATABASE IF EXISTS {b_name} WITH (FORCE)"
    )))
    .execute(&pool)
    .await;
    engines.close().await;

    assert!(oa.values().filter(|v| *v == "ok").count() >= 10, "{oa:?}");
    for (name, x, y) in [
        ("facts", &a.facts, &b.facts),
        ("profile", &a.profile, &b.profile),
        ("recommendation", &a.recommendation, &b.recommendation),
    ] {
        assert!(x == y, "run 1 and run 2 differ in {name}: {}", first_difference(x, y));
    }
    check_golden("facts.golden.txt", &a.facts);
    check_golden("profile.golden.txt", &a.profile);
    check_golden("recommendation.golden.json", &a.recommendation);
}

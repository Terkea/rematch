//! R2 Task 35: `cli profile`, `cli eta` and `cli queue-lag` (ADR-R2-25).
//!
//! The profile fixture runs the real pipeline on synthetic games: `handle_analyse_game` with the
//! profile hook (fake engine, Maia not configured), then one `build-profile`.

use std::sync::{Arc, Mutex};

use analysis::eval_cache::DbEvalCache;
use analysis::jobs::analyse::{AnalyseGameDeps, handle_analyse_game};
use cli::profile_cmd::{run_eta, run_profile};
use cli::queue_cmd::run_queue_lag;
use cli::recompute_cmd::run_recompute_metrics;
use enrich::enrich_game::EnrichDeps;
use enrich::store::MemoryEnrichmentStore;
use profile::build_profile::{BuildProfileDeps, handle_build_profile};
use profile::coaching_input::CoachingInput;
use profile::hook_impl::ProfileHook;
use queue::queue::Queue;
use queue::{AnalyseGamePayload, BuildProfilePayload, Job, KIND_ANALYSE};
use serde_json::{Value, json};
use sqlx::PgPool;
use testkit::fake_engine::FakeAnalyser;
use testkit::{GameSeed, insert_account, insert_game};
use time::OffsetDateTime;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

/// 16 plies, a blitz game (180+0).
const RUY_LOPEZ: &str =
    "1. e4 e5 2. Nf3 Nc6 3. Bb5 a6 4. Ba4 Nf6 5. O-O Be7 6. Re1 b5 7. Bb3 d6 8. c3 O-O 1/2-1/2";
const HERO: &str = "clihero";

#[derive(Default, Clone)]
struct Out {
    out: Arc<Mutex<Vec<String>>>,
    err: Arc<Mutex<Vec<String>>>,
}

impl Out {
    fn print(&self) -> impl Fn(&str) + '_ {
        |l: &str| self.out.lock().unwrap().push(l.to_owned())
    }
    fn print_error(&self) -> impl Fn(&str) + '_ {
        |l: &str| self.err.lock().unwrap().push(l.to_owned())
    }
    fn out(&self) -> String {
        self.out.lock().unwrap().join("\n")
    }
    fn err(&self) -> String {
        self.err.lock().unwrap().join("\n")
    }
}

fn argv(args: &[&str]) -> Vec<String> {
    args.iter().map(|s| (*s).to_owned()).collect()
}

fn job(game_id: Uuid) -> Job {
    let now = OffsetDateTime::now_utc();
    Job {
        id: 1,
        kind: KIND_ANALYSE.to_owned(),
        payload: json!({ "gameId": game_id }),
        dedupe_key: None,
        user_key: None,
        priority: 100,
        attempts: 1,
        max_attempts: 3,
        lease_token: Uuid::new_v4(),
        run_at: now,
        created_at: now,
        started_at: now,
    }
}

/// `games` analysed blitz games of a Lichess account through the profile hook, then its profile
/// build. Returns the account id.
async fn seed_profile(pool: &PgPool, games: usize) -> Uuid {
    let account = insert_account(pool, "lichess", HERO).await;
    let analyser = Arc::new(FakeAnalyser::new());
    let queue = Queue::new(pool.clone());
    let mut deps = AnalyseGameDeps::new(
        pool.clone(),
        analyser.clone(),
        Arc::new(DbEvalCache::new(pool.clone())),
    );
    deps.publish_events = false;
    deps.hook = Some(Arc::new(ProfileHook {
        pool: pool.clone(),
        enrich: EnrichDeps {
            analyser: analyser.clone(),
            store: Arc::new(MemoryEnrichmentStore::default()),
            tb: None,
            difficulty: None,
            cancel: CancellationToken::new(),
        },
        queue: queue.clone(),
    }));
    for _ in 0..games {
        let id = insert_game(
            pool,
            account,
            GameSeed {
                pgn: RUY_LOPEZ.to_owned(),
                white: HERO.to_owned(),
                ..GameSeed::default()
            },
        )
        .await;
        handle_analyse_game(
            &deps,
            &job(id),
            AnalyseGamePayload { game_id: id },
            CancellationToken::new(),
        )
        .await
        .expect("analysis");
    }
    let enriched: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM games WHERE account_id = $1 AND enrichment_protocol = 'enr-v1'",
    )
    .bind(account)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(enriched as usize, games, "the hook enriched every game");
    handle_build_profile(
        &BuildProfileDeps {
            pool: pool.clone(),
            queue,
            now: OffsetDateTime::now_utc,
            benchmark: profile::peer_benchmark::BenchmarkSource::Peers,
        },
        BuildProfilePayload {
            account_id: account,
            time_class: "blitz".to_owned(),
        },
    )
    .await
    .expect("build-profile");
    account
}

const PROFILE_JSON: [&str; 9] = [
    "profile",
    "--platform",
    "lichess",
    "--username",
    "CliHero",
    "--time-class",
    "blitz",
    "--json",
    "",
];

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn profile_json_prints_a_valid_coaching_input(pool: PgPool) {
    let account = seed_profile(&pool, 3).await;
    let o = Out::default();
    let code = run_profile(
        &argv(&PROFILE_JSON[..8]),
        &pool,
        OffsetDateTime::now_utc(),
        &o.print(),
        &o.print_error(),
    )
    .await;
    assert_eq!(code, 0, "{}", o.err());
    let doc: Value = serde_json::from_str(&o.out()).expect("one JSON document");
    assert_eq!(doc["accountId"], json!(account.to_string()));
    assert_eq!(doc["basis"]["analysedGames"], json!(3));
    assert_eq!(doc["areaScores"].as_array().map(Vec::len), Some(7));
    let errors: Vec<String> = ts_validator()
        .iter_errors(&doc)
        .map(|e| format!("{e} at {}", e.instance_path()))
        .collect();
    assert!(
        errors.is_empty(),
        "the TS schema rejects the output: {errors:?}"
    );
    let input: CoachingInput = serde_json::from_value(doc).expect("the contract type");
    input.validate().expect("the M3 contract's rules");
}

/// The zod export R3 parses with (`fixtures/coaching-input/ts-schema.json`).
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

/// The validator is not vacuous: a document the TS schema must refuse is refused.
#[test]
fn the_ts_schema_refuses_a_broken_document() {
    let v = ts_validator();
    assert!(!v.is_valid(&json!({})));
    assert!(!v.is_valid(&json!({ "inputVersion": 1, "band": "B9" })));
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn profile_text_is_one_summary_line_and_says_when_there_is_no_benchmark(pool: PgPool) {
    seed_profile(&pool, 2).await;
    let o = Out::default();
    let code = run_profile(
        &argv(&PROFILE_JSON[..7]),
        &pool,
        OffsetDateTime::now_utc(),
        &o.print(),
        &o.print_error(),
    )
    .await;
    assert_eq!(code, 0, "{}", o.err());
    let out = o.out();
    let first = out.lines().next().unwrap();
    assert!(
        first.starts_with("band B") && first.contains("  analysed 2/2  focus-ready weaknesses "),
        "{first}"
    );
    assert!(first.contains("  areas: openings "), "{first}");
    for area in [
        "middlegame",
        "endgame",
        "tactics",
        "calculation",
        "time",
        "mindset",
    ] {
        assert!(first.contains(&format!(" {area} ")), "{area} in {first}");
    }
    assert!(
        out.contains("no benchmark for this account and time class yet"),
        "the missing benchmark is stated: {out}"
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn profile_of_an_unknown_account_or_bad_arguments_exits_1(pool: PgPool) {
    let o = Out::default();
    let code = run_profile(
        &argv(&PROFILE_JSON[..7]),
        &pool,
        OffsetDateTime::now_utc(),
        &o.print(),
        &o.print_error(),
    )
    .await;
    assert_eq!(code, 1);
    assert_eq!(o.err(), "no account \"CliHero\" on lichess");
    // The lookup never creates the account.
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM linked_accounts")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 0);
    for bad in [
        &["profile", "--platform", "lichess", "--username", "x"][..],
        &[
            "profile",
            "--platform",
            "icc",
            "--username",
            "x",
            "--time-class",
            "blitz",
        ],
        &[
            "profile",
            "--platform",
            "lichess",
            "--username",
            "x",
            "--time-class",
            "fast",
        ],
    ] {
        let o = Out::default();
        let code = run_profile(
            &argv(bad),
            &pool,
            OffsetDateTime::now_utc(),
            &o.print(),
            &o.print_error(),
        )
        .await;
        assert_eq!(code, 1, "{bad:?}");
        assert!(
            o.err().contains("usage: cli profile"),
            "{bad:?}: {}",
            o.err()
        );
    }
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn queue_lag_json_has_the_documented_keys(pool: PgPool) {
    let o = Out::default();
    let code = run_queue_lag(
        &argv(&["queue-lag", "--json"]),
        &pool,
        &o.print(),
        &o.print_error(),
    )
    .await;
    assert_eq!(code, 0, "{}", o.err());
    let doc: Value = serde_json::from_str(&o.out()).unwrap();
    let mut keys: Vec<&str> = doc
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "advice",
            "coreSecondsQueued",
            "oldestP0AgeSeconds",
            "queuedGames"
        ]
    );
    assert_eq!(doc["oldestP0AgeSeconds"], Value::Null);
    assert_eq!(doc["queuedGames"], json!(0));
    assert_eq!(doc["coreSecondsQueued"], json!(0.0));
    assert_eq!(doc["advice"], json!({ "action": "hold", "servers": 0 }));

    let o = Out::default();
    assert_eq!(
        run_queue_lag(&argv(&["queue-lag"]), &pool, &o.print(), &o.print_error()).await,
        0
    );
    assert_eq!(
        o.out(),
        "oldest P0 age: none  queued games: 0  core-seconds queued: 0  advice: hold"
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn eta_prints_the_sentence_for_the_account(pool: PgPool) {
    let account = seed_profile(&pool, 2).await;
    // The ETA counts the games the scheduler selected (ADR-R2-17).
    sqlx::query("UPDATE games SET selected_at = now() WHERE account_id = $1")
        .bind(account)
        .execute(&pool)
        .await
        .unwrap();
    let o = Out::default();
    let code = run_eta(
        &argv(&["eta", "--platform", "lichess", "--username", HERO]),
        &pool,
        &o.print(),
        &o.print_error(),
    )
    .await;
    assert_eq!(code, 0, "{}", o.err());
    // Nothing is outstanding: a known zero (ADR-R2-17).
    assert_eq!(o.out(), "under 1 min, analysing 2 / 2");
    let o = Out::default();
    let code = run_eta(
        &argv(&["eta", "--platform", "lichess", "--username", "nobody"]),
        &pool,
        &o.print(),
        &o.print_error(),
    )
    .await;
    assert_eq!(code, 1);
    assert_eq!(o.err(), "no account \"nobody\" on lichess");
}

// ---------------------------------------------------------------------------------------------
// `cli recompute-metrics` (ADR-FIX-C-1)

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn recompute_metrics_rewrites_stale_rows_from_the_cache(pool: PgPool) {
    seed_profile(&pool, 2).await;
    sqlx::query("UPDATE game_metrics SET metrics = jsonb_set(metrics, '{version}', '2')")
        .execute(&pool)
        .await
        .unwrap();
    let o = Out::default();
    let code = run_recompute_metrics(
        &argv(&["recompute-metrics"]),
        &pool,
        &o.print(),
        &o.print_error(),
    )
    .await;
    assert_eq!(code, 0, "{}", o.err());
    assert_eq!(
        o.out(),
        "marked 2 games with stale metrics
recomputed 2, skipped 0, failed 0"
    );
    let stale: i64 =
        sqlx::query_scalar("SELECT count(*) FROM game_metrics WHERE metrics_version <> $1")
            .bind(profile_core::types::PLAYER_GAME_METRICS_VERSION as i32)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(stale, 0);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn recompute_metrics_refuses_arguments(pool: PgPool) {
    let o = Out::default();
    let code = run_recompute_metrics(
        &argv(&["recompute-metrics", "--now"]),
        &pool,
        &o.print(),
        &o.print_error(),
    )
    .await;
    assert_eq!(code, 1);
    assert!(
        o.err().contains("usage: cli recompute-metrics"),
        "{}",
        o.err()
    );
}

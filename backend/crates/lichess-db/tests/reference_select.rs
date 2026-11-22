//! Reference-player selection (R2 Task 21, R-10, ADR-R2-11 Part 1): bottom-k players by a salted
//! name hash per (band, time class) cell, eviction, the result-independent activity rule, the
//! player-level scan rate, name redaction, enqueueing, and the build's verification order.
//! All dumps are synthetic; nothing is downloaded.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::Duration;

use async_compression::tokio::bufread::ZstdEncoder;
use chess_core::TimeClass;
use futures::stream;
use lichess_db::config::{DumpSource, http_client};
use lichess_db::download::{DumpError, RetryPolicy};
use lichess_db::explorer_build::{BuildOptions, Verify, build};
use lichess_db::pipeline::{PipelineConfig, run_pipeline};
use lichess_db::reference_select::{
    ReferenceSink, SelectOptions, SelectedGame, enqueue_reference_games, hash_fraction,
    memory_bound_bytes, parse_salt, player_hash, player_rate, redact_pgn, reference_filter,
};
use profile_core::types::Band;
use queue::queue::Queue;
use queue::{KIND_REFERENCE_GAME, REFERENCE_PRIORITY};
use refdata::builds::{BuildKind, BuildStatus, NewBuild, create_build, get_build, set_source_sha};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use tokio::io::AsyncReadExt;
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const SALT_A: [u8; 32] = [7u8; 32];
const SALT_B: [u8; 32] = [42u8; 32];

/// A rated blitz game with clocks on `2026.10.<day>`.
fn game(id: &str, white: &str, wr: i32, black: &str, br: i32, result: &str, day: u32) -> String {
    format!(
        "[Event \"Rated Blitz game\"]\n[Site \"https://lichess.org/{id}\"]\n\
         [White \"{white}\"]\n[Black \"{black}\"]\n[Result \"{result}\"]\n\
         [UTCDate \"2026.10.{day:02}\"]\n[UTCTime \"10:00:00\"]\n[WhiteElo \"{wr}\"]\n\
         [BlackElo \"{br}\"]\n[WhiteRatingDiff \"+5\"]\n[BlackRatingDiff \"-5\"]\n\
         [TimeControl \"180+0\"]\n[Termination \"Normal\"]\n\n\
         1. e4 {{ [%clk 0:03:00] }} 1... e5 {{ [%clk 0:03:00] }} {result}\n\n"
    )
}

fn opts(players_per_cell: usize, games_per_player: usize) -> SelectOptions {
    SelectOptions {
        players_per_cell,
        games_per_player,
        min_games_in_time_class: games_per_player,
        ..SelectOptions::default()
    }
}

fn cfg(threads: usize) -> PipelineConfig {
    PipelineConfig {
        threads,
        batch_games: 3,
        channel_batches: 2,
    }
}

async fn select(
    games: &[String],
    salt: [u8; 32],
    o: SelectOptions,
    threads: usize,
) -> Vec<SelectedGame> {
    let sink = Arc::new(ReferenceSink::new(salt, o));
    let src = stream::iter(games.iter().cloned().map(Ok::<_, DumpError>));
    run_pipeline(src, &reference_filter(), &sink, &cfg(threads))
        .await
        .unwrap();
    Arc::try_unwrap(sink).ok().expect("sole owner").finish()
}

fn hex_of(salt: &[u8; 32], name: &str) -> String {
    hex::encode(player_hash(salt, name))
}

/// 3 players (`p1`..`p3`, B2) with 10 blitz games each against one-off opponents: 30 PGNs.
fn three_players() -> Vec<String> {
    let mut out = Vec::new();
    for g in 0..10 {
        for p in 1..=3 {
            let day = 1 + (g * 3 + p) as u32 % 28;
            out.push(game(
                &format!("g{p}x{g:02}"),
                &format!("p{p}"),
                1500,
                &format!("opp{p}x{g}"),
                1520,
                if g % 2 == 0 { "1-0" } else { "0-1" },
                day,
            ));
        }
    }
    out
}

fn players_of(sel: &[SelectedGame]) -> BTreeSet<String> {
    sel.iter().map(|g| g.player_hash.clone()).collect()
}

/// The `k` regular players with the smallest hashes under `salt`.
fn smallest(salt: &[u8; 32], names: &[&str], k: usize) -> BTreeSet<String> {
    let mut h: Vec<String> = names.iter().map(|n| hex_of(salt, n)).collect();
    h.sort();
    h.into_iter().take(k).collect()
}

#[test]
fn defaults_match_the_brief() {
    let o = SelectOptions::default();
    assert_eq!(o.players_per_cell, 20);
    assert_eq!(o.games_per_player, 10);
    assert_eq!(o.oversample, 20);
    assert_eq!(o.max_scan_games, 3_000_000);
    assert_eq!(o.min_games_in_time_class, 20);
    assert_eq!(o.player_rate, 1.0);
}

#[test]
fn player_hash_is_sha256_of_salt_and_lowercased_name() {
    let mut h = Sha256::new();
    h.update(SALT_A);
    h.update(b"alice");
    let want: [u8; 32] = h.finalize().into();
    assert_eq!(player_hash(&SALT_A, "Alice"), want);
    assert_eq!(player_hash(&SALT_A, "ALICE"), want);
    assert_ne!(player_hash(&SALT_B, "alice"), want);
}

#[test]
fn player_rate_scans_about_max_scan_games() {
    // Each admitted player brings all their games; a game passes when either side does.
    assert_eq!(player_rate(3_000_000, 100_000_000), 0.015);
    assert_eq!(player_rate(3_000_000, 1_000_000), 1.0);
    assert_eq!(player_rate(10, 0), 1.0);
}

#[test]
fn memory_bound_is_cells_times_players_times_games_times_pgn() {
    let o = SelectOptions::default();
    // 6 bands x 4 time classes x (20 x 20) players x 10 games x 2 000 bytes.
    assert_eq!(memory_bound_bytes(&o, 2_000), 24 * 400 * 10 * 2_000);
}

#[tokio::test]
async fn thirty_games_of_three_players_select_ten_games_of_two() {
    let games = three_players();
    let sel = select(&games, SALT_A, opts(2, 5), 2).await;
    assert_eq!(sel.len(), 10, "2 players x 5 games");
    let players = players_of(&sel);
    assert_eq!(players.len(), 2);
    assert_eq!(players, smallest(&SALT_A, &["p1", "p2", "p3"], 2));
    let mut per: BTreeMap<String, usize> = BTreeMap::new();
    for g in &sel {
        *per.entry(g.player_hash.clone()).or_default() += 1;
        assert_eq!((g.band, g.tc), (Band::B2, TimeClass::Blitz));
        assert!(g.white_hash == g.player_hash || g.black_hash == g.player_hash);
    }
    assert!(per.values().all(|&n| n == 5), "{per:?}");
    let ids: BTreeSet<&str> = sel
        .iter()
        .map(|g| g.header.lichess_game_id.as_str())
        .collect();
    assert_eq!(ids.len(), 10);
}

#[tokio::test]
async fn same_salt_same_selection_at_any_thread_count_and_another_salt_is_still_bottom_k() {
    let games = three_players();
    let a1 = select(&games, SALT_A, opts(2, 5), 1).await;
    let a4 = select(&games, SALT_A, opts(2, 5), 4).await;
    assert_eq!(a1, a4);
    let b = select(&games, SALT_B, opts(2, 5), 2).await;
    assert_eq!(b.len(), 10);
    assert_eq!(players_of(&b), smallest(&SALT_B, &["p1", "p2", "p3"], 2));
}

#[tokio::test]
async fn the_games_kept_per_player_spread_over_the_whole_month() {
    // One player, 28 games on days 1..28: the 5 kept are the 5 smallest game hashes, not the
    // first 5 of the month.
    let games: Vec<String> = (1..=28)
        .map(|d| {
            game(
                &format!("d{d:02}"),
                "solo",
                1500,
                &format!("o{d}"),
                1500,
                "1-0",
                d,
            )
        })
        .collect();
    let sel = select(&games, SALT_A, opts(1, 5), 2).await;
    assert_eq!(sel.len(), 5);
    let days: Vec<u8> = sel.iter().map(|g| g.header.utc.day()).collect();
    assert!(
        days.iter().any(|&d| d > 5),
        "not just the first days: {days:?}"
    );
}

#[tokio::test]
async fn eviction_is_final() {
    // Cap 1 player per cell (players_per_cell 1, oversample 1). The opponents are B5, in
    // another cell. `big` enters first; `small` (smaller hash) evicts it; `big` then reaches 12
    // games but never re-enters, and `small` alone is selected.
    let mut names = ["n1", "n2", "n3", "n4", "n5", "n6"].to_vec();
    names.sort_by_key(|n| hex_of(&SALT_A, n));
    let (small, big) = (names[0], names[5]);
    let o = SelectOptions {
        oversample: 1,
        ..opts(1, 5)
    };
    let mut games: Vec<String> = (0..2)
        .map(|i| {
            game(
                &format!("b{i}"),
                big,
                1500,
                &format!("x{i}"),
                2400,
                "1-0",
                1,
            )
        })
        .collect();
    games.extend((0..5).map(|i| {
        game(
            &format!("s{i}"),
            small,
            1500,
            &format!("y{i}"),
            2400,
            "1-0",
            2,
        )
    }));
    games.extend((2..12).map(|i| {
        game(
            &format!("b{i}"),
            big,
            1500,
            &format!("z{i}"),
            2400,
            "0-1",
            3,
        )
    }));
    let sel = select(&games, SALT_A, o.clone(), 2).await;
    let b2: BTreeSet<String> = sel
        .iter()
        .filter(|g| g.band == Band::B2)
        .map(|g| g.player_hash.clone())
        .collect();
    assert_eq!(b2, BTreeSet::from([hex_of(&SALT_A, small)]));

    // Without `small`, `big` is selected: the absence above is the eviction.
    let alone: Vec<String> = games
        .iter()
        .filter(|g| !g.contains(small))
        .cloned()
        .collect();
    let sel = select(&alone, SALT_A, o, 2).await;
    assert!(players_of(&sel).contains(&hex_of(&SALT_A, big)));
}

#[tokio::test]
async fn players_need_enough_games_in_the_time_class_within_the_month() {
    // `few` has 5 B2 blitz games; `many` has 5 B2 + 4 B3 blitz games. With 5 per player and at
    // least 8 games in the time class, only `many` qualifies, in B2.
    let mut games: Vec<String> = (0..5)
        .map(|i| {
            game(
                &format!("f{i}"),
                "few",
                1500,
                &format!("a{i}"),
                2400,
                "1-0",
                4,
            )
        })
        .collect();
    games.extend((0..5).map(|i| {
        game(
            &format!("m{i}"),
            "many",
            1500,
            &format!("b{i}"),
            2400,
            "1-0",
            5,
        )
    }));
    games.extend((5..9).map(|i| {
        game(
            &format!("m{i}"),
            "many",
            1800,
            &format!("c{i}"),
            2400,
            "1-0",
            6,
        )
    }));
    let o = SelectOptions {
        min_games_in_time_class: 8,
        ..opts(5, 5)
    };
    let sel = select(&games, SALT_A, o, 2).await;
    assert_eq!(players_of(&sel), BTreeSet::from([hex_of(&SALT_A, "many")]));
    assert!(sel.iter().all(|g| g.band == Band::B2));
}

#[tokio::test]
async fn selection_ignores_results_and_rating_changes() {
    let games = three_players();
    let flipped: Vec<String> = games
        .iter()
        .map(|g| {
            g.replace("1-0", "@@")
                .replace("0-1", "1-0")
                .replace("@@", "0-1")
                .replace("\"+5\"", "\"+60\"")
                .replace("\"-5\"", "\"-60\"")
        })
        .collect();
    let a = players_of(&select(&games, SALT_A, opts(2, 5), 2).await);
    let b = players_of(&select(&flipped, SALT_A, opts(2, 5), 2).await);
    assert_eq!(a, b);
}

#[tokio::test]
async fn bot_games_are_not_used() {
    let games: Vec<String> = three_players()
        .into_iter()
        .map(|g| g.replace("[White \"p1\"]", "[White \"p1\"]\n[WhiteTitle \"BOT\"]"))
        .collect();
    let sel = select(&games, SALT_A, opts(3, 5), 2).await;
    assert!(!players_of(&sel).contains(&hex_of(&SALT_A, "p1")));
    assert_eq!(players_of(&sel).len(), 2);
}

#[tokio::test]
async fn a_player_rate_keeps_the_same_bottom_k_with_fewer_games_admitted() {
    let games = three_players();
    let full = select(&games, SALT_A, opts(2, 5), 2).await;
    let tau = full
        .iter()
        .map(|g| hash_fraction(&hex::decode(&g.player_hash).unwrap().try_into().unwrap()))
        .fold(0.0_f64, f64::max)
        + 1e-9;
    let frac = |n: &str| hash_fraction(&player_hash(&SALT_A, n));
    // Two players above the threshold play 20 games against each other: none is admitted.
    let loud: Vec<String> = (0..200)
        .map(|i| format!("loud{i}"))
        .filter(|n| frac(n) >= tau)
        .take(2)
        .collect();
    assert_eq!(loud.len(), 2, "tau {tau}");
    let mut games = games;
    games.extend((0..20).map(|i| game(&format!("l{i}"), &loud[0], 1500, &loud[1], 1500, "1-0", 9)));
    let expected = games
        .iter()
        .filter(|g| {
            let h = lichess_db::headers::parse_dump_headers(g).unwrap();
            frac(&h.white) < tau || frac(&h.black) < tau
        })
        .count() as u64;
    assert!(expected <= 30);
    let o = SelectOptions {
        player_rate: tau,
        ..opts(2, 5)
    };
    let sink = Arc::new(ReferenceSink::new(SALT_A, o));
    let src = stream::iter(games.iter().cloned().map(Ok::<_, DumpError>));
    run_pipeline(src, &reference_filter(), &sink, &cfg(2))
        .await
        .unwrap();
    let stats = sink.stats();
    assert_eq!(stats.games_admitted, expected, "{stats:?}");
    assert!(!stats.capped);
    let part = Arc::try_unwrap(sink).ok().unwrap().finish();
    assert_eq!(players_of(&part), players_of(&full));
}

#[tokio::test]
async fn max_scan_games_caps_admission() {
    let games = three_players();
    let o = SelectOptions {
        max_scan_games: 12,
        ..opts(2, 5)
    };
    let sink = Arc::new(ReferenceSink::new(SALT_A, o));
    let src = stream::iter(games.iter().cloned().map(Ok::<_, DumpError>));
    run_pipeline(src, &reference_filter(), &sink, &cfg(2))
        .await
        .unwrap();
    let stats = sink.stats();
    assert_eq!(stats.games_admitted, 12);
    assert!(stats.capped);
}

#[test]
fn redaction_removes_both_names() {
    let pgn = game("r1", "alice", 1500, "Bob", 1500, "1-0", 1);
    let red = redact_pgn(&pgn);
    assert!(!red.contains("alice"));
    assert!(!red.contains("Bob"));
    assert!(red.contains("[White \"?\"]"));
    assert!(red.contains("[Black \"?\"]"));
    assert!(red.contains("[Site \"https://lichess.org/r1\"]"));
    assert!(red.contains("1. e4"));
}

async fn verified_build(pool: &PgPool) -> Uuid {
    let id = create_build(
        pool,
        &NewBuild {
            kind: BuildKind::BandReference,
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
    set_source_sha(pool, id, &"ab".repeat(32), 30)
        .await
        .unwrap();
    id
}

async fn jobs(pool: &PgPool) -> Vec<(String, i16, String, Option<String>, serde_json::Value)> {
    sqlx::query_as("SELECT kind, priority, state::text, dedupe_key, payload FROM jobs ORDER BY id")
        .fetch_all(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn enqueue_reference_games_inserts_ten_pending_rows_and_ten_jobs(pool: PgPool) {
    let sel = select(&three_players(), SALT_A, opts(2, 5), 2).await;
    let build = verified_build(&pool).await;
    let q = Queue::new(pool.clone());
    let n = enqueue_reference_games(&pool, &q, build, &sel)
        .await
        .unwrap();
    assert_eq!(n, 10);
    let rows: Vec<(Uuid, String)> =
        sqlx::query_as("SELECT id, status::text FROM reference_games WHERE build_id = $1")
            .bind(build)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(rows.len(), 10);
    assert!(rows.iter().all(|r| r.1 == "pending"));
    let js = jobs(&pool).await;
    assert_eq!(js.len(), 10);
    let ids: BTreeSet<Uuid> = rows.iter().map(|r| r.0).collect();
    for (kind, prio, state, key, payload) in &js {
        assert_eq!(kind, KIND_REFERENCE_GAME);
        assert_eq!(*prio, REFERENCE_PRIORITY);
        assert_eq!(*prio, 200);
        assert_eq!(state, "queued");
        let id: Uuid = payload["referenceGameId"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap();
        assert!(ids.contains(&id));
        assert_eq!(key.as_deref(), Some(format!("refgame:{id}").as_str()));
    }
    // A retried enqueue adds nothing.
    assert_eq!(
        enqueue_reference_games(&pool, &q, build, &sel)
            .await
            .unwrap(),
        10
    );
    assert_eq!(jobs(&pool).await.len(), 10);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_game_selected_for_both_colours_is_inserted_once(pool: PgPool) {
    // `a` and `b` play each other 5 times (B2 both): both are selected with the same 5 games.
    let games: Vec<String> = (0..5)
        .map(|i| game(&format!("ab{i}"), "a", 1500, "b", 1510, "1/2-1/2", 7))
        .collect();
    let sel = select(&games, SALT_A, opts(2, 5), 2).await;
    assert_eq!(sel.len(), 10);
    assert_eq!(players_of(&sel).len(), 2);
    let build = verified_build(&pool).await;
    let n = enqueue_reference_games(&pool, &Queue::new(pool.clone()), build, &sel)
        .await
        .unwrap();
    assert_eq!(n, 5);
    let hashes: Vec<(String, String)> =
        sqlx::query_as("SELECT white_hash, black_hash FROM reference_games")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(hashes.len(), 5);
    assert!(
        hashes
            .iter()
            .all(|(w, b)| *w == hex_of(&SALT_A, "a") && *b == hex_of(&SALT_A, "b"))
    );
    assert_eq!(jobs(&pool).await.len(), 5);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn usernames_are_never_stored(pool: PgPool) {
    let games: Vec<String> = (0..5)
        .map(|i| {
            game(
                &format!("al{i}"),
                "alice",
                1500,
                &format!("Opp{i}"),
                1500,
                "1-0",
                8,
            )
        })
        .collect();
    let sel = select(&games, SALT_A, opts(2, 5), 2).await;
    assert_eq!(sel.len(), 5);
    let build = verified_build(&pool).await;
    enqueue_reference_games(&pool, &Queue::new(pool.clone()), build, &sel)
        .await
        .unwrap();
    let rows: Vec<String> =
        sqlx::query_scalar("SELECT row_to_json(r)::text FROM reference_games r")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(rows.len(), 5);
    for r in &rows {
        assert!(!r.to_lowercase().contains("alice"), "{r}");
        assert!(!r.to_lowercase().contains("opp"), "{r}");
    }
    let js: Vec<String> = sqlx::query_scalar("SELECT row_to_json(j)::text FROM jobs j")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert!(js.iter().all(|j| !j.to_lowercase().contains("alice")));
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn enqueue_refuses_an_unverified_build(pool: PgPool) {
    let sel = select(&three_players(), SALT_A, opts(2, 5), 2).await;
    let build = create_build(
        &pool,
        &NewBuild {
            kind: BuildKind::BandReference,
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
    let err = enqueue_reference_games(&pool, &Queue::new(pool.clone()), build, &sel)
        .await
        .unwrap_err();
    assert!(matches!(err, DumpError::Store(_)), "{err:?}");
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM reference_games")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 0);
    assert!(jobs(&pool).await.is_empty());
}

async fn zstd(data: &[u8]) -> Vec<u8> {
    let mut enc = ZstdEncoder::new(data);
    let mut out = Vec::new();
    enc.read_to_end(&mut out).await.unwrap();
    out
}

async fn server(body: Vec<u8>, sum: String) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/standard/sha256sums.txt"))
        .respond_with(ResponseTemplate::new(200).set_body_string(format!(
            "{sum}  lichess_db_standard_rated_2026-10.pgn.zst\n"
        )))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/standard/lichess_db_standard_rated_2026-10.pgn.zst"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(body))
        .mount(&server)
        .await;
    server
}

fn ref_opts() -> BuildOptions {
    BuildOptions {
        explorer: false,
        band_reference: true,
        reference: opts(2, 5),
        expected_games: Some(30),
        retry: RetryPolicy {
            connect_tries: 3,
            base_delay: Duration::from_millis(1),
            max_resumes: 3,
        },
        pipeline: cfg(2),
        ..BuildOptions::new("2026-10")
    }
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn band_reference_build_verifies_then_enqueues(pool: PgPool) {
    let body = zstd(three_players().concat().as_bytes()).await;
    let sum = hex::encode(Sha256::digest(&body));
    let srv = server(body, sum.clone()).await;
    let out = build(
        &pool,
        &http_client().unwrap(),
        &DumpSource::new(srv.uri(), "2026-10"),
        &ref_opts(),
    )
    .await
    .unwrap();
    assert_eq!(out.explorer_build, None);
    let id = out.reference_build.unwrap();
    let row = get_build(&pool, id).await.unwrap().unwrap();
    assert_eq!(row.kind, BuildKind::BandReference);
    assert_eq!(
        row.status,
        BuildStatus::Building,
        "T32 activates after analysis"
    );
    assert_eq!(row.source_sha256, sum);
    let salt = row.params["salt"]
        .as_str()
        .expect("the salt is stored for audit");
    assert_eq!(salt.len(), 64, "32 bytes, hex");
    assert_eq!(row.params["players_per_cell"], 2);
    let n: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM reference_games WHERE build_id = $1 AND status = 'pending'",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(n, 10);
    assert_eq!(jobs(&pool).await.len(), 10);
}

async fn selected_players(pool: &PgPool, build: Uuid) -> BTreeSet<String> {
    let rows: Vec<(String, String)> =
        sqlx::query_as("SELECT white_hash, black_hash FROM reference_games WHERE build_id = $1")
            .bind(build)
            .fetch_all(pool)
            .await
            .unwrap();
    rows.into_iter().flat_map(|(w, b)| [w, b]).collect()
}

async fn games_of(pool: &PgPool, build: Uuid) -> BTreeSet<String> {
    sqlx::query_scalar("SELECT lichess_game_id FROM reference_games WHERE build_id = $1")
        .bind(build)
        .fetch_all(pool)
        .await
        .unwrap()
        .into_iter()
        .collect()
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_stored_salt_reproduces_the_selection(pool: PgPool) {
    let body = zstd(three_players().concat().as_bytes()).await;
    let sum = hex::encode(Sha256::digest(&body));
    let srv = server(body, sum).await;
    let client = http_client().unwrap();
    let src = DumpSource::new(srv.uri(), "2026-10");
    let first = build(&pool, &client, &src, &ref_opts())
        .await
        .unwrap()
        .reference_build
        .unwrap();
    let row = get_build(&pool, first).await.unwrap().unwrap();
    let salt: [u8; 32] = hex::decode(row.params["salt"].as_str().unwrap())
        .unwrap()
        .try_into()
        .unwrap();
    let again = build(
        &pool,
        &client,
        &src,
        &BuildOptions {
            reference_salt: Some(salt),
            ..ref_opts()
        },
    )
    .await
    .unwrap()
    .reference_build
    .unwrap();
    assert_ne!(first, again);
    let row2 = get_build(&pool, again).await.unwrap().unwrap();
    assert_eq!(row2.params["salt"], row.params["salt"]);
    assert_eq!(games_of(&pool, first).await, games_of(&pool, again).await);
    assert_eq!(
        selected_players(&pool, first).await,
        selected_players(&pool, again).await
    );
    // The stored hashes are those of the stored salt.
    assert!(
        selected_players(&pool, first)
            .await
            .contains(&hex_of(&salt, "p1"))
            || selected_players(&pool, first)
                .await
                .contains(&hex_of(&salt, "p2"))
    );

    // Another salt is honoured too (and recorded).
    let other = build(
        &pool,
        &client,
        &src,
        &BuildOptions {
            reference_salt: Some(SALT_B),
            ..ref_opts()
        },
    )
    .await
    .unwrap()
    .reference_build
    .unwrap();
    let row3 = get_build(&pool, other).await.unwrap().unwrap();
    assert_eq!(row3.params["salt"], hex::encode(SALT_B));
    let want = smallest(&SALT_B, &["p1", "p2", "p3"], 2);
    let got = selected_players(&pool, other).await;
    assert!(want.is_subset(&got), "{want:?} in {got:?}");
}

#[test]
fn parse_salt_takes_64_hex_digits() {
    assert_eq!(parse_salt(&hex::encode(SALT_A)).unwrap(), SALT_A);
    assert!(parse_salt("abcd").is_err());
    assert!(parse_salt(&"zz".repeat(32)).is_err());
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn selection_waits_for_verification(pool: PgPool) {
    let body = zstd(three_players().concat().as_bytes()).await;
    let srv = server(body, hex::encode(Sha256::digest(b"not the dump"))).await;
    let err = build(
        &pool,
        &http_client().unwrap(),
        &DumpSource::new(srv.uri(), "2026-10"),
        &ref_opts(),
    )
    .await
    .unwrap_err();
    assert!(matches!(err, DumpError::ChecksumMismatch { .. }), "{err:?}");
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM reference_games")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 0);
    assert!(jobs(&pool).await.is_empty());
    let st: String = sqlx::query_scalar("SELECT status::text FROM reference_builds")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(st, "failed");
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_band_reference_build_without_enough_players_fails(pool: PgPool) {
    let body = zstd(game("one", "solo", 1500, "o", 1500, "1-0", 1).as_bytes()).await;
    let sum = hex::encode(Sha256::digest(&body));
    let srv = server(body, sum).await;
    let err = build(
        &pool,
        &http_client().unwrap(),
        &DumpSource::new(srv.uri(), "2026-10"),
        &ref_opts(),
    )
    .await
    .unwrap_err();
    assert!(matches!(err, DumpError::Config(_)), "{err:?}");
    let st: String = sqlx::query_scalar("SELECT status::text FROM reference_builds")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(st, "failed");
    let _ = Verify::Full;
}

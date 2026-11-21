//! Port of apps/worker/test/importers/lichess.test.ts, against the recorded fixture.

use std::sync::Arc;

use chess_core::pgn::parse_pgn;
use chess_core::{EngineScore, TimeClass};
use futures::TryStreamExt;
use platforms::lichess::{LichessSource, lichess_games_url, normalize_lichess};
use platforms::{FetchOptions, GameSource, NormalizedGame, Platform, PlatformClient};
use serde_json::{Value, json};
use testkit::fake_platform::FakeClient;
use time::OffsetDateTime;
use time::macros::datetime;

fn fixture() -> Vec<Value> {
    testkit::fixture("lichess-games.ndjson")
        .trim()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

async fn run(
    lines: Vec<Value>,
    user: &str,
    opts: FetchOptions,
    token: Option<&str>,
) -> (Vec<NormalizedGame>, Arc<FakeClient>) {
    let fake = Arc::new(FakeClient::with_ndjson(lines));
    let client: Arc<dyn PlatformClient> = fake.clone();
    let src = LichessSource {
        token: token.map(str::to_owned),
    };
    let games = src
        .fetch(client, user, opts)
        .try_collect::<Vec<_>>()
        .await
        .unwrap();
    (games, fake)
}

fn ms(t: OffsetDateTime) -> i128 {
    t.unix_timestamp_nanos() / 1_000_000
}

fn with(base: &Value, f: impl FnOnce(&mut serde_json::Map<String, Value>)) -> Value {
    let mut v = base.clone();
    f(v.as_object_mut().unwrap());
    v
}

#[tokio::test]
async fn yields_standard_games_and_skips_variants() {
    let (games, _) = run(
        fixture(),
        "DrNykterstein",
        FetchOptions {
            max: Some(3),
            ..Default::default()
        },
        None,
    )
    .await;
    assert_eq!(games.len(), 3);
    assert!(games.iter().all(|g| g.platform_game_id != "chess960x"));
    assert_eq!(games[0].platform, Platform::Lichess);
}

#[tokio::test]
async fn builds_the_request_url_with_lower_cased_username_and_flags() {
    let opts = FetchOptions {
        max: Some(3),
        ..Default::default()
    };
    let (_, fake) = run(fixture(), "DrNykterstein", opts.clone(), None).await;
    let calls = fake.calls();
    let (url, headers) = &calls[0];
    assert_eq!(
        url,
        "https://lichess.org/api/games/user/drnykterstein?max=3&sort=dateDesc&pgnInJson=true&clocks=true&evals=true&opening=true"
    );
    assert_eq!(url, &lichess_games_url("DrNykterstein", &opts));
    assert!(headers.contains(&("Accept".into(), "application/x-ndjson".into())));
    assert!(!headers.iter().any(|(k, _)| k == "Authorization"));
}

#[test]
fn url_param_order_and_username_encoding() {
    let opts = FetchOptions {
        since: Some(datetime!(2026-01-01 0:00 UTC)),
        until: Some(datetime!(2026-02-01 0:00 UTC)),
        max: Some(5),
    };
    assert_eq!(
        lichess_games_url("A b/é", &opts),
        "https://lichess.org/api/games/user/a%20b%2F%C3%A9?since=1767225600000&until=1769904000000&max=5&sort=dateDesc&pgnInJson=true&clocks=true&evals=true&opening=true"
    );
    // encodeURIComponent leaves these unreserved marks alone.
    assert!(
        lichess_games_url("a-b_c.d!~*'()", &FetchOptions::default()).contains("/a-b_c.d!~*'()?")
    );
}

#[tokio::test]
async fn passes_until_as_epoch_ms_for_resuming_from_a_cursor() {
    let until = datetime!(2026-02-01 0:00 UTC);
    let opts = FetchOptions {
        until: Some(until),
        ..Default::default()
    };
    let (_, fake) = run(fixture(), "x", opts, None).await;
    assert!(fake.calls()[0].0.contains(&format!("until={}", ms(until))));
}

#[tokio::test]
async fn passes_since_as_epoch_ms_and_token_as_bearer() {
    let since = datetime!(2026-01-01 0:00 UTC);
    let opts = FetchOptions {
        since: Some(since),
        ..Default::default()
    };
    let (_, fake) = run(fixture(), "x", opts, Some("tok")).await;
    let calls = fake.calls();
    assert!(calls[0].0.contains(&format!("since={}", ms(since))));
    assert!(
        calls[0]
            .1
            .contains(&("Authorization".into(), "Bearer tok".into()))
    );
}

#[tokio::test]
async fn maps_fields_of_a_recorded_game() {
    let fx = fixture();
    let (games, _) = run(fx.clone(), "DrNykterstein", FetchOptions::default(), None).await;
    let g = &games[0];
    let raw = &fx[0];
    assert_eq!(g.platform_game_id, raw["id"].as_str().unwrap());
    assert_eq!(g.time_class, TimeClass::Blitz);
    assert_eq!(g.base_seconds, Some(180));
    assert_eq!(g.increment_seconds, Some(0));
    assert_eq!(ms(g.played_at), raw["createdAt"].as_i64().unwrap() as i128);
    assert_eq!(g.white.name, "DrNykterstein");
    assert_eq!(
        g.white.rating,
        Some(raw["players"]["white"]["rating"].as_i64().unwrap() as i32)
    );
    assert_eq!(g.result, "1-0");
    assert_eq!(g.termination.as_deref(), Some("resign"));
    assert_eq!(g.eco.as_deref(), Some("A50"));
    assert_eq!(
        g.opening_name.as_deref(),
        Some("Queen's Indian Accelerated")
    );
    assert_eq!(g.rated, raw["rated"].as_bool().unwrap());
}

#[tokio::test]
async fn derives_result_from_winner_status() {
    let base = fixture()[0].clone();
    let cases: [(&str, Option<&str>, &str); 12] = [
        ("mate", Some("white"), "1-0"),
        ("mate", Some("black"), "0-1"),
        ("resign", Some("white"), "1-0"),
        ("resign", Some("black"), "0-1"),
        ("outoftime", Some("white"), "1-0"),
        ("outoftime", Some("black"), "0-1"),
        ("timeout", Some("white"), "1-0"),
        ("timeout", Some("black"), "0-1"),
        ("draw", None, "1/2-1/2"),
        ("stalemate", None, "1/2-1/2"),
        ("outoftime", None, "1/2-1/2"),
        ("started", None, "*"),
    ];
    let lines = cases
        .iter()
        .enumerate()
        .map(|(i, (status, winner, _))| {
            with(&base, |m| {
                m.remove("winner");
                m.insert("id".into(), json!(format!("r{i}")));
                m.insert("status".into(), json!(status));
                if let Some(w) = winner {
                    m.insert("winner".into(), json!(w));
                }
            })
        })
        .collect();
    let (games, _) = run(lines, "x", FetchOptions::default(), None).await;
    let got: Vec<&str> = games.iter().map(|g| g.result.as_str()).collect();
    let want: Vec<&str> = cases.iter().map(|c| c.2).collect();
    assert_eq!(got, want);
}

#[tokio::test]
async fn skips_aborted_nostart_and_pgn_less_games() {
    let base = fixture()[0].clone();
    let lines = vec![
        with(&base, |m| {
            m.insert("id".into(), json!("ab"));
            m.insert("status".into(), json!("aborted"));
        }),
        with(&base, |m| {
            m.insert("id".into(), json!("ns"));
            m.insert("status".into(), json!("noStart"));
        }),
        with(&base, |m| {
            m.remove("pgn");
            m.insert("id".into(), json!("np"));
        }),
        with(&base, |m| {
            m.insert("id".into(), json!("ep"));
            m.insert("pgn".into(), json!(""));
        }),
        with(&base, |m| {
            m.insert("id".into(), json!("ok"));
        }),
    ];
    let (games, _) = run(lines, "x", FetchOptions::default(), None).await;
    let ids: Vec<&str> = games.iter().map(|g| g.platform_game_id.as_str()).collect();
    assert_eq!(ids, ["ok"]);
}

#[tokio::test]
async fn maps_correspondence_games_to_daily_with_null_clocks() {
    let lines = vec![with(&fixture()[0], |m| {
        m.remove("clock");
        m.insert("speed".into(), json!("correspondence"));
    })];
    let (games, _) = run(lines, "x", FetchOptions::default(), None).await;
    assert_eq!(games[0].time_class, TimeClass::Daily);
    assert_eq!(games[0].base_seconds, None);
    assert_eq!(games[0].increment_seconds, None);
}

#[tokio::test]
async fn maps_analysis_to_lichess_evals_one_per_ply_padding_with_null() {
    let fx = fixture();
    let (games, _) = run(fx.clone(), "x", FetchOptions::default(), None).await;
    for (i, g) in games.iter().enumerate() {
        let plies = parse_pgn(&g.pgn).unwrap().plies.len();
        let evals = g.lichess_evals.as_ref().unwrap();
        assert_eq!(evals.len(), plies);
        let first = fx[i]["analysis"][0]["eval"].as_i64().unwrap() as i32;
        assert_eq!(evals[0], Some(EngineScore::Cp(first)));
    }
    let raw = fx[0].clone();
    let with_mate = with(&raw, |m| {
        m.insert(
            "analysis".into(),
            json!([{"eval": 10}, {"mate": -3, "judgment": {"name": "Blunder"}}]),
        );
    });
    let (games, _) = run(vec![with_mate], "x", FetchOptions::default(), None).await;
    let evals = games[0].lichess_evals.as_ref().unwrap();
    assert_eq!(evals[0], Some(EngineScore::Cp(10)));
    assert_eq!(evals[1], Some(EngineScore::Mate(-3)));
    assert_eq!(evals[2], None);
    assert_eq!(
        evals.len(),
        raw["moves"].as_str().unwrap().split(' ').count()
    );
}

#[tokio::test]
async fn truncates_analysis_longer_than_the_ply_count_and_prefers_mate() {
    let lines = vec![json!({
        "id": "t", "createdAt": 1, "moves": "e4 e5", "pgn": "1. e4 e5 *",
        "players": {"white": {}, "black": {"aiLevel": 3}},
        "analysis": [{"eval": 1, "mate": 2}, {"eval": 2}, {"eval": 3}]
    })];
    let (games, _) = run(lines, "x", FetchOptions::default(), None).await;
    let g = &games[0];
    assert_eq!(
        g.lichess_evals,
        Some(vec![Some(EngineScore::Mate(2)), Some(EngineScore::Cp(2))])
    );
    assert_eq!(g.white.name, "Anonymous");
    assert_eq!(g.black.name, "Stockfish level 3");
    assert_eq!(g.time_class, TimeClass::Daily);
}

#[tokio::test]
async fn has_null_lichess_evals_without_analysis() {
    let lines = vec![with(&fixture()[0], |m| {
        m.remove("analysis");
    })];
    let (games, _) = run(lines, "x", FetchOptions::default(), None).await;
    assert_eq!(games[0].lichess_evals, None);
}

#[tokio::test]
async fn recorded_standard_games_parse_with_a_clock_on_every_ply() {
    let (games, _) = run(fixture(), "x", FetchOptions::default(), None).await;
    assert!(!games.is_empty());
    for g in &games {
        let plies = parse_pgn(&g.pgn).unwrap().plies;
        assert!(!plies.is_empty());
        if g.time_class != TimeClass::Daily {
            assert!(plies.iter().all(|p| p.clock_seconds.is_some()));
        }
    }
}

#[test]
fn normalize_returns_none_for_non_standard_variants() {
    let raw = with(&fixture()[0], |m| {
        m.insert("variant".into(), json!("chess960"));
    });
    assert!(normalize_lichess(&raw).is_none());
    assert!(normalize_lichess(&fixture()[0]).is_some());
}

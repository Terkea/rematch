//! Port of apps/worker/test/importers/chesscom.test.ts (fixtures, no live network).

use std::collections::HashMap;
use std::sync::Arc;

use chess_core::TimeClass;
use chess_core::pgn::parse_pgn;
use futures::StreamExt;
use platforms::chesscom::{ChesscomSource, normalize_chesscom};
use platforms::client::{ClientOptions, HttpPlatformClient};
use platforms::{
    FetchOptions, GameSource, NormalizedGame, Platform, PlatformClient, PlatformError,
};
use serde_json::{Value, json};
use testkit::fake_platform::FakeClient;
use time::OffsetDateTime;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const ARCHIVES_URL: &str = "https://api.chess.com/pub/player/hikaru/games/archives";

struct Fixtures {
    archives: Vec<String>,
    month: Vec<Value>,
}

fn fixtures() -> Fixtures {
    let a: Value = serde_json::from_str(&testkit::fixture("chesscom-archives.json")).unwrap();
    let m: Value = serde_json::from_str(&testkit::fixture("chesscom-month.json")).unwrap();
    Fixtures {
        archives: a["archives"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect(),
        month: m["games"].as_array().unwrap().clone(),
    }
}

fn fake(months: Vec<(String, Vec<Value>)>) -> (Arc<FakeClient>, Fixtures) {
    fake_with(months, None)
}

/// `archives_override` replaces the API's archive list (for ordering tests).
fn fake_with(
    months: Vec<(String, Vec<Value>)>,
    archives_override: Option<Vec<String>>,
) -> (Arc<FakeClient>, Fixtures) {
    let f = fixtures();
    let list = archives_override.unwrap_or_else(|| f.archives.clone());
    // Every archive month exists (empty unless given); any other URL is NotFound.
    let mut json: HashMap<String, Value> = f
        .archives
        .iter()
        .map(|a| (a.clone(), json!({"games": []})))
        .collect();
    json.insert(ARCHIVES_URL.to_string(), json!({"archives": list}));
    json.insert(f.archives[2].clone(), json!({"games": f.month}));
    for (k, v) in months {
        json.insert(k, json!({"games": v}));
    }
    (
        Arc::new(FakeClient {
            json,
            ..FakeClient::default()
        }),
        f,
    )
}

async fn collect(
    c: &Arc<FakeClient>,
    user: &str,
    opts: FetchOptions,
) -> Vec<Result<NormalizedGame, PlatformError>> {
    let src = ChesscomSource::new();
    let client: Arc<dyn PlatformClient> = c.clone();
    src.fetch(client, user, opts).collect().await
}

async fn games(c: &Arc<FakeClient>, user: &str, opts: FetchOptions) -> Vec<NormalizedGame> {
    collect(c, user, opts)
        .await
        .into_iter()
        .map(|r| r.unwrap())
        .collect()
}

fn urls(c: &FakeClient) -> Vec<String> {
    c.calls().into_iter().map(|(url, _)| url).collect()
}

fn with(g: &Value, patch: Value) -> Value {
    let mut g = g.clone();
    for (k, v) in patch.as_object().unwrap() {
        g[k] = v.clone();
    }
    g
}

fn ids(gs: &[NormalizedGame]) -> Vec<&str> {
    gs.iter().map(|g| g.platform_game_id.as_str()).collect()
}

fn at(s: i64) -> OffsetDateTime {
    OffsetDateTime::from_unix_timestamp(s).unwrap()
}

fn oct_games(f: &Fixtures) -> Vec<Value> {
    vec![
        with(
            &f.month[0],
            json!({"url": "https://www.chess.com/game/live/1", "end_time": 1000}),
        ),
        with(
            &f.month[0],
            json!({"url": "https://www.chess.com/game/live/2", "end_time": 2000}),
        ),
    ]
}

#[tokio::test]
async fn walks_archives_newest_first_with_a_lower_cased_username() {
    let f = fixtures();
    let (c, f) = fake(vec![(f.archives[1].clone(), oct_games(&f))]);
    let gs = games(&c, "Hikaru", FetchOptions::default()).await;
    assert_eq!(
        urls(&c)[..3],
        [
            ARCHIVES_URL.to_string(),
            f.archives[2].clone(),
            f.archives[1].clone()
        ]
    );
    assert_eq!(gs.len(), 7);
    assert_eq!(ids(&gs[5..]), ["2", "1"]);
}

#[tokio::test]
async fn sorts_archives_by_url_not_api_order() {
    let f = fixtures();
    let reversed: Vec<String> = f.archives.iter().rev().cloned().collect();
    let (c, f) = fake_with(vec![(f.archives[1].clone(), oct_games(&f))], Some(reversed));
    let _ = games(&c, "hikaru", FetchOptions::default()).await;
    assert_eq!(
        urls(&c)[..3],
        [
            ARCHIVES_URL.to_string(),
            f.archives[2].clone(),
            f.archives[1].clone()
        ]
    );
}

#[tokio::test]
async fn yields_games_within_a_month_newest_first_and_skips_chess960() {
    let (c, _) = fake(vec![]);
    let gs = games(&c, "hikaru", FetchOptions::default()).await;
    let ends: Vec<_> = gs.iter().map(|g| g.played_at).collect();
    let mut sorted = ends.clone();
    sorted.sort_by(|a, b| b.cmp(a));
    assert_eq!(ends, sorted);
    assert_eq!(gs.len(), 5); // 4 real + daily; chess960 skipped
    assert!(!ids(&gs).contains(&"900000000001"));
}

#[tokio::test]
async fn stops_at_max_without_fetching_more_months() {
    let f = fixtures();
    let (c, f) = fake(vec![(f.archives[1].clone(), oct_games(&f))]);
    let gs = games(
        &c,
        "hikaru",
        FetchOptions {
            max: Some(2),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(gs.len(), 2);
    assert_eq!(urls(&c), [ARCHIVES_URL.to_string(), f.archives[2].clone()]);
}

#[tokio::test]
async fn max_zero_yields_nothing_and_fetches_nothing() {
    let (c, _) = fake(vec![]);
    let gs = games(
        &c,
        "hikaru",
        FetchOptions {
            max: Some(0),
            ..Default::default()
        },
    )
    .await;
    assert!(gs.is_empty());
    assert!(urls(&c).is_empty());
}

#[tokio::test]
async fn max_counts_across_months() {
    let f = fixtures();
    let (c, _) = fake(vec![(f.archives[1].clone(), oct_games(&f))]);
    let gs = games(
        &c,
        "hikaru",
        FetchOptions {
            max: Some(6),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(ids(&gs[5..]), ["2"]);
    assert_eq!(gs.len(), 6);
}

#[tokio::test]
async fn does_not_fetch_months_entirely_before_since() {
    let (c, f) = fake(vec![]);
    let gs = games(
        &c,
        "hikaru",
        FetchOptions {
            since: Some(at(1_793_491_200)), // 2026-11-01T00:00:00Z
            ..Default::default()
        },
    )
    .await;
    assert_eq!(urls(&c), [ARCHIVES_URL.to_string(), f.archives[2].clone()]);
    assert_eq!(gs.len(), 5);
}

#[tokio::test]
async fn skips_months_entirely_after_until_and_filters_newer_games_until_inclusive() {
    let f = fixtures();
    let (c, f) = fake(vec![(f.archives[1].clone(), oct_games(&f))]);
    let gs = games(
        &c,
        "hikaru",
        FetchOptions {
            until: Some(at(1_792_022_400)), // 2026-10-15T00:00:00Z
            ..Default::default()
        },
    )
    .await;
    // November starts after `until`, so only October and older are fetched.
    assert_eq!(
        urls(&c),
        [
            ARCHIVES_URL.to_string(),
            f.archives[1].clone(),
            f.archives[0].clone()
        ]
    );
    assert_eq!(ids(&gs), ["2", "1"]);
}

#[tokio::test]
async fn filters_games_older_than_since_within_a_month() {
    let (c, f) = fake(vec![]);
    let cut = f.month[2]["end_time"].as_i64().unwrap();
    let gs = games(
        &c,
        "hikaru",
        FetchOptions {
            since: Some(at(cut)),
            ..Default::default()
        },
    )
    .await;
    assert!(!gs.is_empty());
    assert!(gs.len() < 5);
    assert!(gs.iter().all(|g| g.played_at >= at(cut)));
}

#[tokio::test]
async fn maps_increments_daily_and_ids() {
    let f = fixtures();
    let inc = with(&f.month[0], json!({"time_control": "180+2"}));
    let (c, f) = fake(vec![(f.archives[2].clone(), vec![inc, f.month[5].clone()])]);
    let gs = games(&c, "hikaru", FetchOptions::default()).await;
    let live = gs
        .iter()
        .find(|g| g.time_class != TimeClass::Daily)
        .unwrap();
    assert_eq!(live.time_class, TimeClass::Blitz);
    assert_eq!(live.base_seconds, Some(180));
    assert_eq!(live.increment_seconds, Some(2));
    assert_eq!(live.platform, Platform::Chesscom);
    assert_eq!(live.platform_game_id, "187517419600");
    assert!(live.lichess_evals.is_none());
    let daily = gs
        .iter()
        .find(|g| g.time_class == TimeClass::Daily)
        .unwrap();
    assert_eq!(daily.base_seconds, Some(86400));
    assert_eq!(daily.increment_seconds, Some(0));
    assert_eq!(daily.platform_game_id, "900000000002");
    let _ = f;
}

#[tokio::test]
async fn maps_1_259200_to_daily_with_base_259200() {
    let f = fixtures();
    let g = with(&f.month[5], json!({"time_control": "1/259200"}));
    let (c, _) = fake(vec![(f.archives[2].clone(), vec![g])]);
    let gs = games(
        &c,
        "hikaru",
        FetchOptions {
            max: Some(1),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(gs[0].time_class, TimeClass::Daily);
    assert_eq!(gs[0].base_seconds, Some(259200));
    assert_eq!(gs[0].increment_seconds, Some(0));
}

#[tokio::test]
async fn skips_games_with_an_unparseable_time_control() {
    let f = fixtures();
    let bad = with(
        &f.month[0],
        json!({"time_control": "-", "url": "https://www.chess.com/game/live/777"}),
    );
    let (c, _) = fake(vec![(f.archives[2].clone(), vec![bad, f.month[1].clone()])]);
    let gs = games(
        &c,
        "hikaru",
        FetchOptions {
            max: Some(5),
            ..Default::default()
        },
    )
    .await;
    assert!(!ids(&gs).contains(&"777"));
    assert!(gs.iter().all(|g| g.time_class != TimeClass::Daily));
    assert_eq!(gs.len(), 1);
}

#[tokio::test]
async fn takes_result_and_metadata_from_the_pgn() {
    let (c, _) = fake(vec![]);
    let gs = games(&c, "hikaru", FetchOptions::default()).await;
    let g = gs
        .iter()
        .find(|x| x.platform_game_id == "187517419600")
        .unwrap();
    assert_eq!(g.result, "1-0");
    assert_eq!(g.white.name, "Hikaru");
    assert_eq!(g.white.rating, Some(3459));
    assert_eq!(g.eco.as_deref(), Some("A01"));
    assert_eq!(
        g.opening_name.as_deref(),
        Some("Nimzowitsch Larsen Attack Classical Variation")
    );
    assert_eq!(g.termination.as_deref(), Some("Hikaru won by resignation"));
    assert_eq!(g.played_at, at(1795051401));
}

#[tokio::test]
async fn recorded_standard_games_parse_with_a_clock_on_every_ply() {
    let (c, _) = fake(vec![]);
    let gs = games(&c, "hikaru", FetchOptions::default()).await;
    assert!(!gs.is_empty());
    for g in gs {
        let parsed = parse_pgn(&g.pgn).unwrap();
        assert!(!parsed.plies.is_empty());
        if g.time_class != TimeClass::Daily {
            assert!(parsed.plies.iter().all(|p| p.clock_seconds.is_some()));
        }
    }
}

#[tokio::test]
async fn empty_archives_list_yields_nothing() {
    let (c, _) = fake_with(vec![], Some(vec![]));
    let rs = collect(&c, "hikaru", FetchOptions::default()).await;
    assert!(rs.is_empty());
    assert_eq!(urls(&c), [ARCHIVES_URL.to_string()]);
}

#[tokio::test]
async fn unknown_user_yields_not_found() {
    let c = Arc::new(FakeClient::default());
    let rs = collect(&c, "ghost", FetchOptions::default()).await;
    assert_eq!(rs.len(), 1);
    assert!(matches!(rs[0], Err(PlatformError::NotFound { .. })));
}

#[tokio::test]
async fn http_404_maps_to_not_found() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/ghost/games/archives"))
        .respond_with(ResponseTemplate::new(404))
        .expect(1)
        .mount(&server)
        .await;
    let client: Arc<dyn PlatformClient> = Arc::new(HttpPlatformClient::new(ClientOptions {
        name: Platform::Chesscom,
        retry_after: std::time::Duration::ZERO,
        ..ClientOptions::default()
    }));
    let src = ChesscomSource::with_base_url(&server.uri());
    let rs: Vec<_> = src
        .fetch(client, "Ghost", FetchOptions::default())
        .collect()
        .await;
    assert!(matches!(rs[..], [Err(PlatformError::NotFound { .. })]));
}

#[test]
fn normalize_skips_variants_and_defaults_rated_false() {
    let f = fixtures();
    assert!(normalize_chesscom(&f.month[4]).is_none()); // chess960
    let mut g = f.month[0].clone();
    g.as_object_mut().unwrap().remove("rated");
    assert!(!normalize_chesscom(&g).unwrap().rated);
}

#[tokio::test]
async fn december_archive_with_since_in_january_is_skipped() {
    let f = fixtures();
    let dec = "https://api.chess.com/pub/player/hikaru/games/2026/12".to_string();
    let mut list = f.archives.clone();
    list.push(dec.clone());
    let (c, _) = fake_with(vec![(dec.clone(), oct_games(&f))], Some(list));
    // December 2026 ends 2027-01-01T00:00:00Z; since after that skips it and stops.
    let gs = games(
        &c,
        "hikaru",
        FetchOptions {
            since: Some(at(1_798_761_600 + 86_400)), // 2027-01-02
            ..Default::default()
        },
    )
    .await;
    assert!(gs.is_empty());
    assert_eq!(urls(&c), [ARCHIVES_URL.to_string()]);
    // since inside December still fetches it
    let (c, _) = fake_with(
        vec![(dec.clone(), oct_games(&f))],
        Some(vec![f.archives[2].clone(), dec.clone()]),
    );
    let gs = games(
        &c,
        "hikaru",
        FetchOptions {
            since: Some(at(0)),
            max: Some(1),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(urls(&c)[1], dec);
    assert_eq!(gs.len(), 1);
}

#[tokio::test]
async fn since_equal_to_a_month_end_skips_that_month() {
    let (c, f) = fake(vec![]);
    // November ends 2026-12-01T00:00:00Z = 1796083200. Since == end: month skipped (<=).
    let gs = games(
        &c,
        "hikaru",
        FetchOptions {
            since: Some(at(1_796_083_200)),
            ..Default::default()
        },
    )
    .await;
    assert!(gs.is_empty());
    assert_eq!(urls(&c), [ARCHIVES_URL.to_string()]);
    let _ = f;
    // One second earlier the month straddles and is fetched.
    let (c, f) = fake(vec![]);
    let _ = games(
        &c,
        "hikaru",
        FetchOptions {
            since: Some(at(1_796_083_199)),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(urls(&c)[1], f.archives[2]);
}

#[test]
fn rated_is_taken_from_the_data() {
    let f = fixtures();
    let t = normalize_chesscom(&with(&f.month[0], json!({"rated": true}))).unwrap();
    let u = normalize_chesscom(&with(&f.month[0], json!({"rated": false}))).unwrap();
    assert!(t.rated);
    assert!(!u.rated);
}

#[test]
fn one_over_86400_is_daily_with_base_86400() {
    let f = fixtures();
    let g = normalize_chesscom(&with(&f.month[0], json!({"time_control": "1/86400"}))).unwrap();
    assert_eq!(g.time_class, TimeClass::Daily);
    assert_eq!(g.base_seconds, Some(86400));
    assert_eq!(g.increment_seconds, Some(0));
}

#[test]
fn opening_name_comes_from_the_slug_with_hyphens_as_spaces() {
    let f = fixtures();
    let pgn = f.month[0]["pgn"].as_str().unwrap();
    let with_header = format!(
        "[Opening \"Ignored Name\"]
{pgn}"
    );
    let g = normalize_chesscom(&with(&f.month[0], json!({"pgn": with_header}))).unwrap();
    assert_eq!(
        g.opening_name.as_deref(),
        Some("Nimzowitsch Larsen Attack Classical Variation")
    );
    let slug = pgn.replace(
        "Nimzowitsch-Larsen-Attack-Classical-Variation-2.Bb2-Bg4-3.h3-Bh5",
        "Caro-Kann-Defense-Advance-Variation-3.e5-c5",
    );
    let g = normalize_chesscom(&with(&f.month[0], json!({"pgn": slug}))).unwrap();
    assert_eq!(
        g.opening_name.as_deref(),
        Some("Caro Kann Defense Advance Variation")
    );
}

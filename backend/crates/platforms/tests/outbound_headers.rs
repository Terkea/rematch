//! ADR 0043 on the wire: every request a chess.com import sends carries the fixed project
//! User-Agent and no email address, in any header or in the URL.

use std::sync::Arc;

use futures::StreamExt;
use platforms::chesscom::ChesscomSource;
use platforms::client::{ClientOptions, HttpPlatformClient};
use platforms::{FetchOptions, GameSource, Platform, PlatformClient};
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const FIXED_UA: &str = "chess-improver/0.1 (+https://github.com/chess-improver)";

#[tokio::test]
async fn a_chesscom_import_sends_only_the_fixed_user_agent_and_no_email() {
    let server = MockServer::start().await;
    let month = format!("{}/pub/player/someone/games/2026/03", server.uri());
    Mock::given(method("GET"))
        .and(path("/pub/player/someone/games/archives"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "archives": [month] })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/pub/player/someone/games/2026/03"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "games": [] })))
        .mount(&server)
        .await;

    let client: Arc<dyn PlatformClient> = Arc::new(HttpPlatformClient::new(ClientOptions {
        name: Platform::Chesscom,
        ..ClientOptions::default()
    }));
    let source = ChesscomSource::with_base_url(&format!("{}/pub/player", server.uri()));
    let games: Vec<_> = source
        .fetch(client, "someone", FetchOptions::default())
        .collect()
        .await;
    assert!(games.iter().all(Result::is_ok), "{games:?}");

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 2, "the archive list and one month");
    for req in &requests {
        let agents: Vec<_> = req.headers.get_all("user-agent").iter().collect();
        assert_eq!(agents.len(), 1, "{:?}", req.headers);
        assert_eq!(agents[0].to_str().unwrap(), FIXED_UA);
        assert!(req.headers.get("from").is_none(), "{:?}", req.headers);
        for (name, value) in &req.headers {
            let value = value.to_str().unwrap_or_default();
            assert!(!value.contains('@'), "{name}: {value}");
        }
        assert!(!req.url.as_str().contains('@'), "{}", req.url);
    }
}

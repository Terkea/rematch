use std::time::Duration;

use sqlx::PgPool;

#[test]
fn fixture_reads_backend_fixtures() {
    let pgn = testkit::fixture("pgn/scholars-mate.pgn");
    assert!(pgn.contains("Qxf7#"), "{pgn}");
}

#[test]
fn stockfish_is_configured_in_docker() {
    if testkit::in_docker() {
        let path = testkit::stockfish().expect("STOCKFISH_PATH");
        assert!(path.exists(), "{} does not exist", path.display());
    }
}

#[tokio::test]
async fn wait_for_returns_the_first_some() {
    let mut calls = 0;
    let got = testkit::wait_for(
        "third call",
        Duration::from_secs(5),
        Duration::from_millis(1),
        || {
            calls += 1;
            let n = calls;
            async move { (n >= 3).then_some(n) }
        },
    )
    .await;
    assert_eq!(got, 3);
}

#[tokio::test]
#[should_panic(expected = "timed out waiting for never")]
async fn wait_for_panics_on_timeout() {
    testkit::wait_for::<(), _, _>(
        "never",
        Duration::from_millis(20),
        Duration::from_millis(5),
        || async { None },
    )
    .await;
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn seeds_an_account_and_a_game(pool: PgPool) {
    let account = testkit::insert_account(&pool, "lichess", "Hero").await;
    let (username, display): (String, String) =
        sqlx::query_as("SELECT username, display_name FROM linked_accounts WHERE id = $1")
            .bind(account)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!((username.as_str(), display.as_str()), ("hero", "Hero"));

    let game = testkit::insert_game(
        &pool,
        account,
        testkit::GameSeed {
            pending_since_ago: Some(Duration::from_secs(3600)),
            ..Default::default()
        },
    )
    .await;
    let (platform, time_class, status, old): (String, String, String, bool) = sqlx::query_as(
        "SELECT platform::text, time_class::text, analysis_status::text, \
           pending_since < now() - interval '59 minutes' FROM games WHERE id = $1",
    )
    .bind(game)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        (platform.as_str(), time_class.as_str(), status.as_str(), old),
        ("lichess", "blitz", "pending", true)
    );
}

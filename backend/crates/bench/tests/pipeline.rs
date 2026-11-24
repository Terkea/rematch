//! The harness end to end on a tiny suite: generated games go through the real analysis, hook,
//! build-profile and CoachingInput, and the same seed gives the same games.

use std::sync::Arc;

use bench::flaws::MissMate;
use bench::generate::generate_game;
use bench::harness::Harness;
use bench::memo::MemoAnalyser;
use bench::results::score;
use bench::suite::{PlayerDef, Suite, game_specs};
use engine::Analyser;
use engine::pool::{EnginePool, PoolOptions};
use profile_core::stats::variant::PriorRule;
use sqlx::PgPool;

fn tiny() -> Suite {
    Suite {
        name: "test".to_owned(),
        players: vec![
            PlayerDef {
                name: "bench-t-missmate".to_owned(),
                flaw: Some(("miss_mate".to_owned(), 1.0)),
                games: 4,
                seed: 3,
            },
            PlayerDef {
                name: "bench-t-clean".to_owned(),
                flaw: None,
                games: 4,
                seed: 4,
            },
        ],
        checkpoints: vec![2, 4],
        nodes: 1_500,
        opponents: 3,
        perturb: true,
    }
}

async fn engines() -> Option<Arc<EnginePool>> {
    let path = testkit::stockfish()?;
    Some(
        EnginePool::create(PoolOptions {
            size: 2,
            path,
            hash_mb: Some(16),
        })
        .await
        .expect("engine pool"),
    )
}

#[tokio::test]
async fn the_same_seed_gives_the_same_game() {
    let Some(pool) = engines().await else { return };
    let inner: Arc<dyn Analyser> = pool.clone();
    let p = &tiny().players[0];
    let spec = &game_specs(p, 3)[0];
    let flaw: Arc<dyn bench::flaws::Flaw> = Arc::new(MissMate(1.0));
    let (a, ta) = generate_game(&MemoAnalyser::new(inner.clone(), 1_500), spec, Some(&flaw))
        .await
        .unwrap();
    let (b, tb) = generate_game(&MemoAnalyser::new(inner, 1_500), spec, Some(&flaw))
        .await
        .unwrap();
    assert_eq!(a, b);
    assert_eq!(ta, tb);
    let parsed = chess_core::pgn::parse_pgn(&a.pgn).expect("generated PGN parses");
    assert_eq!(parsed.plies.len() as u32, a.plies);
    assert!(parsed.plies.iter().all(|p| p.clock_seconds.is_some()));
    EnginePool::close(&pool).await;
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_tiny_suite_runs_through_the_real_pipeline(pool: PgPool) {
    let Some(engines) = engines().await else {
        return;
    };
    let inner: Arc<dyn Analyser> = engines.clone();
    let suite = tiny();
    let harness = Harness {
        pool: pool.clone(),
        memo: Arc::new(MemoAnalyser::new(inner, suite.nodes)),
        engines: 2,
        log: Box::new(|l| eprintln!("{l}")),
        variant: None,
    };
    let players = harness.run(&suite).await.expect("suite runs");
    EnginePool::close(&engines).await;
    assert_eq!(players.len(), 2);
    for p in &players {
        assert_eq!(p.checkpoints.len(), 2, "{}", p.name);
        assert!(p.deterministic, "{}", p.name);
        assert!(p.perturbed.is_some());
        // build-profile wrote an estimate row for every binomial key with evidence.
        let last = p.last().unwrap();
        assert!(
            last.estimates.iter().any(|e| e.key == "W7.traps"),
            "{:?}",
            last.estimates
        );
    }
    let done: i64 = sqlx::query_scalar("SELECT count(*) FROM games WHERE analysis_status = 'done'")
        .fetch_one(&pool)
        .await
        .unwrap();
    let total: i64 = sqlx::query_scalar("SELECT count(*) FROM games")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(done, total, "every game analysed");
    let enriched: i64 =
        sqlx::query_scalar("SELECT count(*) FROM games WHERE enrichment_protocol IS NOT NULL")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(enriched, total, "every game enriched by the hook");
    // Under 30 games nothing is ranked (MIN_GAMES), and the scorecard still renders.
    let s = score("test", suite.nodes, &players);
    assert_eq!(s.metrics.clean_players, 1);
    assert!(!bench::results::markdown(&s).is_empty());
}

/// `ALGO_VARIANT` side by side: the champion run as the variant reproduces the main checkpoints
/// exactly (the second account sees the same games), and a different rule moves the estimates.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_side_by_side_variant_sees_the_same_games(pool: PgPool) {
    let Some(engines) = engines().await else {
        return;
    };
    let inner: Arc<dyn Analyser> = engines.clone();
    let mut suite = tiny();
    suite.perturb = false;
    let memo = Arc::new(MemoAnalyser::new(inner, suite.nodes));
    let champion = Harness {
        pool: pool.clone(),
        memo: memo.clone(),
        engines: 2,
        log: Box::new(|l| eprintln!("{l}")),
        variant: Some(("champion".to_owned(), PriorRule::CHAMPION)),
    };
    let players = champion.run(&suite).await.expect("suite runs");
    for p in &players {
        let v = p.variant.as_ref().expect("variant result");
        assert!(v.deterministic, "{}", p.name);
        assert_eq!(v.checkpoints, p.checkpoints, "{}", p.name);
    }
    let lighter = PriorRule {
        prior_games: Some(1.0),
        effective_counts: true,
        dispersion: false,
    };
    let other = Harness {
        pool: pool.clone(),
        memo,
        engines: 2,
        log: Box::new(|l| eprintln!("{l}")),
        variant: Some(("dev".to_owned(), lighter)),
    };
    let mut renamed = suite.clone();
    for p in &mut renamed.players {
        p.name = format!("{}-b", p.name);
    }
    let again = other.run(&renamed).await.expect("suite runs");
    EnginePool::close(&engines).await;
    assert_eq!(profile_core::stats::variant::current(), PriorRule::CHAMPION);
    let moved = again.iter().any(|p| {
        let v = p.variant.as_ref().unwrap();
        v.checkpoints
            .iter()
            .zip(&p.checkpoints)
            .any(|(a, b)| a.estimates != b.estimates)
    });
    assert!(moved, "a lighter prior moves at least one estimate");
}

//! `ALGO_VARIANT` side by side (ADR 0110): the variant's prior rule is process-wide, so these
//! tests live in their own binary (their own process) and run one after another in one test, so no
//! other build can see the rule while it is set.

use std::sync::Arc;

use bench::harness::Harness;
use bench::memo::MemoAnalyser;
use bench::suite::{PlayerDef, Suite};
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

/// `ALGO_VARIANT` side by side: `rule` runs as the variant on the suite's own games.
async fn side_by_side(pool: PgPool, rule: PriorRule) -> Option<Vec<bench::results::PlayerResult>> {
    let engines = engines().await?;
    let inner: Arc<dyn Analyser> = engines.clone();
    let suite = tiny();
    let harness = Harness {
        pool,
        memo: Arc::new(MemoAnalyser::new(inner, suite.nodes)),
        engines: 2,
        log: Box::new(|l| eprintln!("{l}")),
        variant: Some(("test".to_owned(), rule)),
    };
    let players = harness.run(&suite).await.expect("suite runs");
    EnginePool::close(&engines).await;
    // The process's rule is restored after every variant build.
    assert_eq!(profile_core::stats::variant::current(), PriorRule::CHAMPION);
    Some(players)
}

/// The champion as the variant reproduces the main checkpoints exactly, perturbed one included:
/// the second account sees the same games and nothing of the first. Then a lighter prior run side
/// by side moves the estimates and leaves the main run alone.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_side_by_side_variant_sees_the_same_games(pool: PgPool) {
    let Some(players) = side_by_side(pool.clone(), PriorRule::CHAMPION).await else {
        return;
    };
    for p in &players {
        let v = p.variant.as_ref().expect("variant result");
        assert!(v.deterministic, "{}", p.name);
        assert_eq!(v.checkpoints, p.checkpoints, "{}", p.name);
        assert_eq!(v.perturbed, p.perturbed, "{}", p.name);
    }
    sqlx::query("TRUNCATE linked_accounts CASCADE")
        .execute(&pool)
        .await
        .expect("a fresh start");
    let rule = PriorRule {
        prior_games: Some(1.0),
        effective_counts: true,
        dispersion: false,
    };
    let players = side_by_side(pool, rule).await.expect("engines, as above");
    let moved = players.iter().any(|p| {
        let v = p.variant.as_ref().unwrap();
        v.checkpoints
            .iter()
            .zip(&p.checkpoints)
            .any(|(a, b)| a.estimates != b.estimates)
    });
    assert!(moved, "a lighter prior moves at least one estimate");
}

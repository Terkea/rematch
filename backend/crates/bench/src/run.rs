//! One benchmark run against a throwaway database: create it next to `DATABASE_URL`'s, migrate,
//! run the suite, score it, drop it.

use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;

use engine::Analyser;
use engine::pool::{EnginePool, PoolOptions};
use serde::{Deserialize, Serialize};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{ConnectOptions, Connection};

use crate::harness::Harness;
use crate::memo::MemoAnalyser;
use crate::results::{PlayerResult, Scorecard, markdown, score};
use crate::suite::Suite;

pub const RESULTS_FILE: &str = "bench-results.json";
pub const SCORECARD_FILE: &str = "bench-scorecard.md";

pub struct RunOptions {
    pub database_url: String,
    pub stockfish: PathBuf,
    pub engines: usize,
    pub out_dir: PathBuf,
    /// Keep the benchmark database after the run (for inspection).
    pub keep_db: bool,
}

/// `bench-results.json`: the scorecard and every player's raw results.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RunOutput {
    pub scorecard: Scorecard,
    pub suite: Suite,
    pub engine_searches: u64,
    pub memo_hits: u64,
    pub seconds: f64,
    pub players: Vec<PlayerResult>,
}

async fn admin(url: &str) -> anyhow::Result<sqlx::PgConnection> {
    Ok(PgConnectOptions::from_str(url)?.connect().await?)
}

pub async fn run(
    suite: &Suite,
    o: &RunOptions,
    log: Box<dyn Fn(&str) + Send + Sync>,
) -> anyhow::Result<RunOutput> {
    enrich::install_crypto_provider();
    let started = std::time::Instant::now();
    let name = format!("bench_{}", uuid::Uuid::new_v4().simple());
    let mut conn = admin(&o.database_url).await?;
    // `name` is our own `bench_<uuid hex>`, never input.
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE DATABASE {name}")))
        .execute(&mut conn)
        .await?;
    log(&format!("database {name}"));
    let opts = PgConnectOptions::from_str(&o.database_url)?.database(&name);
    let pool = PgPoolOptions::new()
        .max_connections(u32::try_from(o.engines * 2 + 4).unwrap_or(16))
        .connect_with(opts)
        .await?;
    let outcome = async {
        db::MIGRATOR.run(&pool).await?;
        let engines = EnginePool::create(PoolOptions {
            size: o.engines.max(1),
            path: o.stockfish.clone(),
            hash_mb: Some(16),
        })
        .await?;
        let inner: Arc<dyn Analyser> = engines.clone();
        let memo = Arc::new(MemoAnalyser::new(inner, suite.nodes));
        let harness = Harness {
            pool: pool.clone(),
            memo: memo.clone(),
            engines: o.engines.max(1),
            log,
        };
        let players = harness.run(suite).await;
        EnginePool::close(&engines).await;
        let players = players?;
        let (engine_searches, memo_hits) = memo.counts();
        anyhow::Ok(RunOutput {
            scorecard: score(&suite.name, suite.nodes, &players),
            suite: suite.clone(),
            engine_searches,
            memo_hits,
            seconds: started.elapsed().as_secs_f64(),
            players,
        })
    }
    .await;
    pool.close().await;
    if !o.keep_db {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "DROP DATABASE IF EXISTS {name} WITH (FORCE)"
        )))
        .execute(&mut conn)
        .await?;
    }
    conn.close().await?;
    let out = outcome?;
    write_outputs(&out, &o.out_dir)?;
    Ok(out)
}

/// Writes [`RESULTS_FILE`] and [`SCORECARD_FILE`] into `dir`.
pub fn write_outputs(out: &RunOutput, dir: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(dir)?;
    std::fs::write(dir.join(RESULTS_FILE), serde_json::to_string_pretty(out)?)?;
    std::fs::write(dir.join(SCORECARD_FILE), markdown(&out.scorecard))?;
    Ok(())
}

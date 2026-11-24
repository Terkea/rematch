//! One benchmark run against a throwaway database: create it next to `DATABASE_URL`'s, migrate,
//! run the suite, score it, drop it.

use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;

use engine::Analyser;
use engine::pool::{EnginePool, PoolOptions};
use serde::{Deserialize, Serialize};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{ConnectOptions, Connection, PgPool};

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
    /// A variant run side by side (`ALGO_VARIANT`): its name and prior rule.
    pub variant: Option<(String, profile_core::stats::variant::PriorRule)>,
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
    /// The side-by-side variant's scorecard, from its own checkpoints.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant: Option<VariantRun>,
}

/// The side-by-side variant of a run (`ALGO_VARIANT`).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VariantRun {
    pub name: String,
    pub scorecard: Scorecard,
}

async fn admin(url: &str) -> anyhow::Result<sqlx::PgConnection> {
    Ok(PgConnectOptions::from_str(url)?.connect().await?)
}

/// A migrated database `bench_<uuid>` next to `DATABASE_URL`'s, for one benchmark or lab run.
pub struct Throwaway {
    pub name: String,
    pub pool: PgPool,
    admin: sqlx::PgConnection,
}

impl Throwaway {
    /// Creates and migrates the database; a failed migration drops it again.
    pub async fn create(
        database_url: &str,
        max_connections: u32,
        log: &(dyn Fn(&str) + Send + Sync),
    ) -> anyhow::Result<Self> {
        let name = format!("bench_{}", uuid::Uuid::new_v4().simple());
        let mut admin = admin(database_url).await?;
        // `name` is our own `bench_<uuid hex>`, never input.
        sqlx::query(sqlx::AssertSqlSafe(format!("CREATE DATABASE {name}")))
            .execute(&mut admin)
            .await?;
        log(&format!("database {name}"));
        let opts = PgConnectOptions::from_str(database_url)?.database(&name);
        let pool = PgPoolOptions::new()
            .max_connections(max_connections)
            .connect_with(opts)
            .await?;
        let me = Self { name, pool, admin };
        if let Err(e) = db::MIGRATOR.run(&me.pool).await {
            me.finish(false).await?;
            return Err(e.into());
        }
        Ok(me)
    }

    /// Closes the pool and drops the database, unless `keep`.
    pub async fn finish(mut self, keep: bool) -> anyhow::Result<()> {
        self.pool.close().await;
        if !keep {
            sqlx::query(sqlx::AssertSqlSafe(format!(
                "DROP DATABASE IF EXISTS {} WITH (FORCE)",
                self.name
            )))
            .execute(&mut self.admin)
            .await?;
        }
        self.admin.close().await?;
        Ok(())
    }
}

/// `engines` Stockfish processes behind one in-memory cache at `nodes` per search (ADR 0100).
pub async fn memo_engines(
    stockfish: &Path,
    engines: usize,
    nodes: u64,
) -> anyhow::Result<(Arc<EnginePool>, Arc<MemoAnalyser>)> {
    let pool = EnginePool::create(PoolOptions {
        size: engines.max(1),
        path: stockfish.to_path_buf(),
        hash_mb: Some(16),
    })
    .await?;
    let inner: Arc<dyn Analyser> = pool.clone();
    Ok((pool, Arc::new(MemoAnalyser::new(inner, nodes))))
}

pub async fn run(
    suite: &Suite,
    o: &RunOptions,
    log: Box<dyn Fn(&str) + Send + Sync>,
) -> anyhow::Result<RunOutput> {
    enrich::install_crypto_provider();
    let started = std::time::Instant::now();
    let db = Throwaway::create(
        &o.database_url,
        u32::try_from(o.engines * 2 + 4).unwrap_or(16),
        &*log,
    )
    .await?;
    let outcome = async {
        let (engines, memo) = memo_engines(&o.stockfish, o.engines, suite.nodes).await?;
        let harness = Harness {
            pool: db.pool.clone(),
            memo: memo.clone(),
            engines: o.engines.max(1),
            log,
            variant: o.variant.clone(),
        };
        let players = harness.run(suite).await;
        EnginePool::close(&engines).await;
        let players = players?;
        let (engine_searches, memo_hits) = memo.counts();
        let variant = o.variant.as_ref().map(|(name, _)| {
            let side: Vec<PlayerResult> = players
                .iter()
                .filter_map(PlayerResult::as_variant)
                .collect();
            VariantRun {
                name: name.clone(),
                scorecard: score(&suite.name, suite.nodes, &side),
            }
        });
        anyhow::Ok(RunOutput {
            scorecard: score(&suite.name, suite.nodes, &players),
            suite: suite.clone(),
            engine_searches,
            memo_hits,
            seconds: started.elapsed().as_secs_f64(),
            players,
            variant,
        })
    }
    .await;
    db.finish(o.keep_db).await?;
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

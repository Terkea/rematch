//! The harness: generated games through the real pipeline, in-process, against one database.
//!
//! For each player:
//! 1. Generate its games ([`generate_game`], `engines` games at a time).
//! 2. Insert them as the chess.com importer does (`db::games::upsert_game`, status `pending`).
//! 3. Per checkpoint, in chronological order: analyse the new games with the production
//!    `analyse-game` handler and the production profile hook (enrichment, move features,
//!    metrics, series), then run the production `build-profile` handler with the peer
//!    benchmark, then read the `weakness_estimates` rows and build the CoachingInput.
//! 4. Build the CoachingInput a second time from the same rows (determinism).
//! 5. With `perturb`, a second account with 90% of the games (every game whose seed-drawn coin
//!    falls under 0.1 is left out) runs the same pipeline once (stability).
//!
//! Every engine request goes through the [`MemoAnalyser`], so the pipeline evaluates the games
//! with the generator's own searches. The clock of every build is the latest game's time plus
//! one hour ([`set_now`]).

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use analysis::eval_cache::DbEvalCache;
use analysis::jobs::analyse::{AnalyseGameDeps, handle_analyse_game};
use chess_core::TimeClass;
use db::games::{AnalysisStatus, NewGame, upsert_game};
use engine::Analyser;
use enrich::enrich_game::EnrichDeps;
use enrich::store::DbEnrichmentStore;
use futures::StreamExt;
use profile::build_profile::{BuildProfileDeps, handle_build_profile};
use profile::coaching::build_coaching_input_report;
use profile::hook_impl::{ProfileHook, into_hook, warm_openings};
use profile::peer_benchmark::BenchmarkSource;
use profile_core::stats::rng::Rng;
use profile_core::stats::variant::{self, PriorRule};
use queue::queue::Queue;
use queue::{AnalyseGamePayload, BuildProfilePayload, Job, KIND_ANALYSE};
use sqlx::PgPool;
use time::OffsetDateTime;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::flaws::Tally;
use crate::generate::{GeneratedGame, generate_game};
use crate::memo::MemoAnalyser;
use crate::results::{
    Checkpoint, EstimateRow, FlawInfo, GenStats, PlayerResult, RankedItem, VariantResult,
};
use crate::suite::{PlayerDef, Suite, game_specs};

pub const PLATFORM: &str = "chesscom";
pub const TIME_CLASS: TimeClass = TimeClass::Rapid;
/// Share of games left out of the perturbed account.
pub const PERTURB_DROP: f64 = 0.1;

static NOW: AtomicI64 = AtomicI64::new(0);

/// The build clock (`BuildProfileDeps::now` takes a plain `fn`).
fn bench_now() -> OffsetDateTime {
    OffsetDateTime::from_unix_timestamp(NOW.load(Ordering::SeqCst)).unwrap_or(crate::suite::ANCHOR)
}

/// Sets the clock every later build reads (the lab sets it to its request's `now`).
pub fn set_now(t: OffsetDateTime) {
    NOW.store(t.unix_timestamp(), Ordering::SeqCst);
}

pub struct Harness {
    pub pool: PgPool,
    pub memo: Arc<MemoAnalyser>,
    /// Games generated or analysed at once.
    pub engines: usize,
    pub log: Box<dyn Fn(&str) + Send + Sync>,
    /// A variant run side by side (`ALGO_VARIANT`, the evaluation spec section 5): every player's
    /// games also go into a second account, whose profiles are built under the variant's prior
    /// rule, so the two versions see the same games. `None`: the process's rule only.
    pub variant: Option<(String, PriorRule)>,
}

/// A chess.com account named `username` (the importer's shape).
pub async fn insert_account(pool: &PgPool, username: &str) -> anyhow::Result<Uuid> {
    Ok(sqlx::query_scalar(
        "INSERT INTO linked_accounts (platform, username, display_name) \
         VALUES ($1::platform, $2, $3) RETURNING id",
    )
    .bind(PLATFORM)
    .bind(username.to_lowercase())
    .bind(username)
    .fetch_one(pool)
    .await?)
}

/// Inserts `g` as the chess.com importer does (status `pending`), on the generator's clock.
async fn insert_game(
    pool: &PgPool,
    account: Uuid,
    g: &GeneratedGame,
    suffix: &str,
) -> anyhow::Result<Uuid> {
    let base = crate::generate::BASE_SECONDS as i32;
    insert_game_with_clock(pool, account, g, suffix, base, 0).await
}

/// [`insert_game`] with the game's own base time and increment (the lab's games, ADR 0106).
pub async fn insert_game_with_clock(
    pool: &PgPool,
    account: Uuid,
    g: &GeneratedGame,
    suffix: &str,
    base_seconds: i32,
    increment_seconds: i32,
) -> anyhow::Result<Uuid> {
    let r = upsert_game(
        pool,
        &NewGame {
            account_id: account,
            platform: PLATFORM.to_owned(),
            platform_game_id: format!("{}{suffix}", g.platform_game_id),
            pgn: g.pgn.clone(),
            played_at: g.played_at,
            time_class: TIME_CLASS.as_str().to_owned(),
            base_seconds: Some(base_seconds),
            increment_seconds: Some(increment_seconds),
            rated: true,
            player_color: g.player_color.clone(),
            white_name: g.white_name.clone(),
            black_name: g.black_name.clone(),
            white_rating: Some(g.white_rating),
            black_rating: Some(g.black_rating),
            result: g.result.clone(),
            termination: Some(g.termination.clone()),
            eco: None,
            opening_name: None,
            lichess_evals: None,
            status: AnalysisStatus::Pending,
            ply_count: Some(g.plies as i32),
        },
    )
    .await?;
    anyhow::ensure!(r.inserted, "game {} already exists", g.platform_game_id);
    Ok(r.id)
}

fn job_for(game_id: Uuid) -> Job {
    let now = OffsetDateTime::now_utc();
    Job {
        id: 1,
        kind: KIND_ANALYSE.to_owned(),
        payload: serde_json::json!({ "gameId": game_id }),
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

impl Harness {
    /// The production `analyse-game` dependencies with the profile hook, served by the memo.
    pub fn analyse_deps(&self) -> AnalyseGameDeps {
        let analyser: Arc<dyn Analyser> = self.memo.clone();
        let queue = Queue::new(self.pool.clone());
        let mut deps = AnalyseGameDeps::new(
            self.pool.clone(),
            analyser.clone(),
            Arc::new(DbEvalCache::new(self.pool.clone())),
        );
        deps.publish_events = false;
        deps.hook = Some(into_hook(ProfileHook {
            pool: self.pool.clone(),
            enrich: EnrichDeps {
                analyser,
                store: Arc::new(DbEnrichmentStore::new(self.pool.clone())),
                tb: None,
                difficulty: None,
                cancel: CancellationToken::new(),
            },
            queue,
        }));
        deps
    }

    /// Analyses `ids` with the production handler, `engines` at a time.
    pub async fn analyse(&self, deps: &AnalyseGameDeps, ids: &[Uuid]) -> anyhow::Result<()> {
        let results: Vec<_> = futures::stream::iter(ids.iter().copied())
            .map(|id| async move {
                handle_analyse_game(
                    deps,
                    &job_for(id),
                    AnalyseGamePayload { game_id: id },
                    CancellationToken::new(),
                )
                .await
                .map_err(|e| anyhow::anyhow!("analyse {id}: {e}"))
            })
            .buffer_unordered(self.engines.max(1))
            .collect()
            .await;
        for r in results {
            r?;
        }
        Ok(())
    }

    /// The production `build-profile` of `account` (rapid) against the peer benchmark.
    pub async fn build(&self, account: Uuid) -> anyhow::Result<()> {
        let deps = BuildProfileDeps {
            pool: self.pool.clone(),
            queue: Queue::new(self.pool.clone()),
            now: bench_now,
            benchmark: BenchmarkSource::Peers,
        };
        handle_build_profile(
            &deps,
            BuildProfilePayload {
                account_id: account,
                time_class: TIME_CLASS.as_str().to_owned(),
            },
        )
        .await
        .map_err(|e| anyhow::anyhow!("build-profile: {e}"))?;
        Ok(())
    }

    async fn estimates(&self, account: Uuid) -> anyhow::Result<Vec<EstimateRow>> {
        type Row = (String, f64, i32, f64, f64, f64, f64, Option<f64>, f64);
        let rows: Vec<Row> = sqlx::query_as(
            "SELECT series_key, k_unweighted, n_unweighted, mean, ci_low, ci_high, priority, \
               band_median, confidence \
             FROM weakness_estimates WHERE account_id = $1 AND time_class = $2::time_class \
               AND side = 'both' ORDER BY series_key",
        )
        .bind(account)
        .bind(TIME_CLASS.as_str())
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| EstimateRow {
                key: r.0,
                events: r.1,
                opportunities: f64::from(r.2),
                mean: r.3,
                low: r.4,
                high: r.5,
                priority: r.6,
                band_median: r.7,
                confidence: r.8,
            })
            .collect())
    }

    async fn ranked(&self, account: Uuid) -> anyhow::Result<(Vec<RankedItem>, String)> {
        let report = build_coaching_input_report(&self.pool, account, TIME_CLASS, bench_now())
            .await
            .map_err(|e| anyhow::anyhow!("coaching input: {e}"))?;
        let ranked = report
            .input
            .weaknesses
            .iter()
            .map(|w| {
                let key = match &w.variant {
                    Some(v) => format!("{}.{v}", w.id),
                    None => w.id.clone(),
                };
                RankedItem {
                    drills: ai::output::drills::drills_for(&w.id, w.variant.as_deref())
                        .into_iter()
                        .map(|d| d.as_str().to_owned())
                        .collect(),
                    key,
                    rank: w.rank,
                    priority: w.priority,
                    confidence: w.confidence,
                    posterior_rate: w.posterior_rate,
                    interval_low: w.interval_low,
                    interval_high: w.interval_high,
                    band_median: w.band_median,
                    events: w.events,
                    opportunities: w.opportunities,
                }
            })
            .collect();
        let fingerprint = serde_json::to_string(&report.input.weaknesses)?;
        Ok((ranked, fingerprint))
    }

    async fn benchmark_players(&self, account: Uuid) -> anyhow::Result<Option<i32>> {
        Ok(sqlx::query_scalar::<_, Option<i32>>(
            "SELECT n_players FROM account_benchmark WHERE account_id = $1 \
             AND time_class = $2::time_class",
        )
        .bind(account)
        .bind(TIME_CLASS.as_str())
        .fetch_optional(&self.pool)
        .await?
        .flatten())
    }

    /// [`Self::checkpoint`] under `rule`, with the process's rule restored afterwards. Nothing else
    /// builds a profile meanwhile: the players and their checkpoints run one after another.
    async fn checkpoint_under(
        &self,
        rule: PriorRule,
        account: Uuid,
        games: usize,
    ) -> anyhow::Result<(Checkpoint, String)> {
        let before = variant::current();
        variant::set(rule);
        let out = self.checkpoint(account, games).await;
        let again = match &out {
            Ok(_) => Some(self.ranked(account).await),
            Err(_) => None,
        };
        variant::set(before);
        let (c, f) = out?;
        // The determinism fingerprint is the second build of the same rows, under the same rule.
        let f = match again {
            Some(Ok((_, again))) if again == f => f,
            Some(Ok(_)) => String::new(),
            Some(Err(e)) => return Err(e),
            None => f,
        };
        Ok((c, f))
    }

    async fn checkpoint(
        &self,
        account: Uuid,
        games: usize,
    ) -> anyhow::Result<(Checkpoint, String)> {
        self.build(account).await?;
        let (ranked, fingerprint) = self.ranked(account).await?;
        Ok((
            Checkpoint {
                games,
                ranked,
                estimates: self.estimates(account).await?,
                benchmark_players: self.benchmark_players(account).await?,
            },
            fingerprint,
        ))
    }

    /// Runs one player through generation and the pipeline.
    pub async fn run_player(&self, suite: &Suite, p: &PlayerDef) -> anyhow::Result<PlayerResult> {
        let flaw = p.flaw();
        let specs = game_specs(p, suite.opponents);
        let started = std::time::Instant::now();
        let generated: Vec<anyhow::Result<(GeneratedGame, Tally)>> =
            futures::stream::iter(specs.iter())
                .map(|s| {
                    let flaw = flaw.clone();
                    async move { generate_game(&self.memo, s, flaw.as_ref()).await }
                })
                .buffered(self.engines.max(1))
                .collect()
                .await;
        let mut games = Vec::new();
        let mut tally = Tally::default();
        for g in generated {
            let (game, t) = g?;
            tally.add(&t);
            games.push(game);
        }
        let gen_seconds = started.elapsed().as_secs_f64();
        (self.log)(&format!(
            "{}: generated {} games in {gen_seconds:.0}s",
            p.name,
            games.len()
        ));

        let deps = self.analyse_deps();
        let account = insert_account(&self.pool, &p.name).await?;
        let mut ids = Vec::new();
        for g in &games {
            ids.push(insert_game(&self.pool, account, g, "").await?);
        }
        let side = match &self.variant {
            Some((name, rule)) => {
                let acct = insert_account(&self.pool, &format!("{}-v", p.name)).await?;
                let mut vids = Vec::new();
                for g in &games {
                    vids.push(insert_game(&self.pool, acct, g, "-v").await?);
                }
                Some((name.clone(), *rule, acct, vids))
            }
            None => None,
        };
        let mut variant_checkpoints = Vec::new();
        let mut variant_deterministic = true;
        let mut checkpoints = Vec::new();
        let mut done = 0usize;
        let mut last_fingerprint = String::new();
        for &cp in &suite.checkpoints {
            let cp = cp.min(games.len());
            if cp <= done {
                continue;
            }
            self.analyse(&deps, &ids[done..cp]).await?;
            if let Some((_, _, _, vids)) = &side {
                self.analyse(&deps, &vids[done..cp]).await?;
            }
            done = cp;
            set_now(games[cp - 1].played_at + time::Duration::hours(1));
            if let Some((name, rule, acct, _)) = &side {
                let (c, f) = self.checkpoint_under(*rule, *acct, cp).await?;
                variant_deterministic &= !f.is_empty();
                (self.log)(&format!(
                    "{}: {} games, variant {name} top {:?}",
                    p.name,
                    cp,
                    c.ranked
                        .iter()
                        .take(3)
                        .map(|r| r.key.as_str())
                        .collect::<Vec<_>>()
                ));
                variant_checkpoints.push(c);
            }
            let (c, f) = self.checkpoint(account, cp).await?;
            (self.log)(&format!(
                "{}: {} games, top {:?}",
                p.name,
                cp,
                c.ranked
                    .iter()
                    .take(3)
                    .map(|r| r.key.as_str())
                    .collect::<Vec<_>>()
            ));
            checkpoints.push(c);
            last_fingerprint = f;
        }
        // Determinism: the same rows and clock give the same CoachingInput weaknesses.
        let (_, again) = self.ranked(account).await?;
        let deterministic = again == last_fingerprint;

        let perturbed = if suite.perturb {
            let mut rng = Rng::new(p.seed ^ 0x5EED);
            let kept: Vec<&GeneratedGame> = games
                .iter()
                .filter(|_| rng.next_f64() >= PERTURB_DROP)
                .collect();
            let acct = insert_account(&self.pool, &format!("{}-p", p.name)).await?;
            let mut pids = Vec::new();
            for g in &kept {
                pids.push(insert_game(&self.pool, acct, g, "-p").await?);
            }
            self.analyse(&deps, &pids).await?;
            if let Some(last) = kept.last() {
                set_now(last.played_at + time::Duration::hours(1));
            }
            let (c, _) = self.checkpoint(acct, kept.len()).await?;
            Some(c)
        } else {
            None
        };

        let (searches, hits) = self.memo.counts();
        (self.log)(&format!(
            "{}: done in {:.0}s (engine searches so far {searches}, memo hits {hits})",
            p.name,
            started.elapsed().as_secs_f64()
        ));
        Ok(PlayerResult {
            name: p.name.clone(),
            flaw: flaw.as_ref().map(|f| FlawInfo {
                id: f.id().to_owned(),
                rate: f.rate(),
                target_key: f.target_key().to_owned(),
                unit: f.unit(),
                expected_drills: f
                    .expected_drills()
                    .iter()
                    .map(|d| (*d).to_owned())
                    .collect(),
            }),
            tally,
            generation: GenStats::of(&games, gen_seconds),
            checkpoints,
            perturbed,
            deterministic,
            variant: side.map(|(name, ..)| VariantResult {
                name,
                checkpoints: variant_checkpoints,
                deterministic: variant_deterministic,
            }),
        })
    }

    /// Runs every player of the suite, one after another.
    pub async fn run(&self, suite: &Suite) -> anyhow::Result<Vec<PlayerResult>> {
        warm_openings().await;
        let mut out = Vec::new();
        for p in &suite.players {
            out.push(self.run_player(suite, p).await?);
        }
        Ok(out)
    }
}

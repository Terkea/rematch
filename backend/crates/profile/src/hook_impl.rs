//! The analyse-game hook implementation (R2 Task 31): [`ProfileHook`] enriches the player's
//! moves (Task 25), computes the game's features, metrics, opening and weakness series
//! ([`compute_game_features`]) and writes them in the analysis transaction.
//!
//! - `persist` replaces the game's `move_features`, `game_metrics`, `game_openings` and
//!   `weakness_series_point` rows (re-analysis never duplicates them), sets
//!   `games.enrichment_protocol = 'enr-v1'` and `analysed_at = now()`, and deletes the game's
//!   `game_previews` row, all in the same transaction as the `done` transition (R-13, T28
//!   review).
//! - `after_commit` enqueues one `build-profile` per account × time class (deduped on
//!   `profile:<account>:<tc>`, 30 s later, so a batch of games builds once) and records the
//!   `enrich-game` `job_metrics` row (Task 25's stages, `features_ms` and the skipped-input
//!   counts).
//! - Difficulty comes only from Maia (owner override): with Maia down or not configured the
//!   moves stay `pending_maia`, and [`crate::requeue::run_difficulty_sweep`] recomputes the game
//!   once Maia has answered.

use std::sync::Arc;
use std::time::{Duration, Instant};

use analysis::hook::{AnalyseHook, HookError, HookInput, PreparedHook};
use chess_core::game_accuracy::game_accuracy;
use chess_core::score::win_percent;
use chess_core::{Color, MoveRecord, TimeClass};
use db::profile_rows::{
    GameMetricsRow, SeriesPointRow, insert_game_metrics, replace_series_points,
};
use db::stage_metrics::{StageMetrics, record_stage_metrics};
use engine::{ANALYSIS_PROTOCOL, EngineError};
use enrich::difficulty::DifficultyOutcome;
use enrich::enrich_game::{EnrichDeps, EnrichError, EnrichInput, enrich_game};
use enrich::persist::persist_move_features;
use enrich::protocol::ENRICHMENT_PROTOCOL;
use futures::future::BoxFuture;
use profile_core::openings::repertoire::{ExplorerIndex, opening_plies};
use profile_core::types::RawMoveFeatures;
use queue::queue::{Queue, QueueError};
use queue::{BuildProfilePayload, EnqueueOptions, KIND_BUILD_PROFILE, profile_key};
use sqlx::{PgConnection, PgPool};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::analyse_features::{
    FeatureInput, GameFeatures, compute_game_features, game_context, opening_name, parse_color,
    parse_time_class, previous_own_rating,
};

/// A `build-profile` job runs this long after the game that asked for it, so a batch of games
/// analysed together builds the profile once.
pub const BUILD_PROFILE_DELAY: Duration = Duration::from_secs(30);
/// `job_metrics.kind` of the hook's enrichment and feature pass.
pub const ENRICH_GAME_KIND: &str = "enrich-game";

/// The analyse-game hook of the profile engine (see the module docs).
pub struct ProfileHook {
    pub pool: PgPool,
    /// The enrichment's engines, store, tablebase and Maia. Its `cancel` is replaced by each
    /// game's run token.
    pub enrich: EnrichDeps,
    pub queue: Queue,
}

/// Enqueues the account × time class profile build ([`BUILD_PROFILE_DELAY`] from now). `false`
/// when a live job already holds `profile:<account>:<tc>`.
pub async fn enqueue_build_profile(
    queue: &Queue,
    account_id: Uuid,
    time_class: TimeClass,
) -> Result<bool, QueueError> {
    let payload = serde_json::to_value(BuildProfilePayload {
        account_id,
        time_class: time_class.as_str().to_owned(),
    })
    .unwrap_or_default();
    let run_at = OffsetDateTime::now_utc() + BUILD_PROFILE_DELAY;
    let id = queue
        .enqueue(
            KIND_BUILD_PROFILE,
            &payload,
            EnqueueOptions {
                dedupe_key: Some(profile_key(account_id, time_class.as_str())),
                user_key: Some(account_id),
                run_at: Some(run_at),
                ..EnqueueOptions::default()
            },
        )
        .await?;
    Ok(id.is_some())
}

/// The explorer of the player's band (or the nearest band with data) for the game's opening
/// positions. `None` on a read error (logged): the game then has no book exit, which is a lack
/// of evidence, never a failure of the analysis.
pub async fn load_game_explorer(
    pool: &PgPool,
    account_id: Uuid,
    time_class: TimeClass,
    parsed: &chess_core::ParsedGame,
) -> Option<ExplorerIndex> {
    let keys: Vec<String> = opening_plies(&parsed.plies)
        .into_iter()
        .map(|p| p.fen_key_before)
        .collect();
    if keys.is_empty() {
        return None;
    }
    let loaded = async {
        let (band, source) = refdata::repertoire::player_band(pool, account_id, time_class).await?;
        refdata::repertoire::load_explorer_with_fallback(pool, &keys, band, source, time_class)
            .await
    }
    .await;
    match loaded {
        Ok((index, _)) => Some(index),
        Err(e) => {
            tracing::warn!(%account_id, error = %e, "explorer unavailable; no book exit");
            None
        }
    }
}

/// The game a row set belongs to.
#[derive(Clone, Debug)]
pub struct DerivedTarget {
    pub game_id: Uuid,
    pub account_id: Uuid,
    pub time_class: TimeClass,
    pub played_at: OffsetDateTime,
    pub player_color: Color,
    pub platform_eco: Option<String>,
    pub platform_opening_name: Option<String>,
}

/// Replaces the game's `game_metrics`, `game_openings` and `weakness_series_point` rows (in the
/// caller's transaction). `move_features` is written separately (it carries the difficulty
/// status the sweep maintains).
pub async fn write_derived_rows(
    conn: &mut PgConnection,
    target: &DerivedTarget,
    features: &GameFeatures,
    parsed: &chess_core::ParsedGame,
    difficulty_model: Option<&str>,
) -> Result<(), sqlx::Error> {
    let tc = target.time_class.as_str();
    let metrics =
        serde_json::to_value(&features.metrics).map_err(|e| sqlx::Error::Encode(Box::new(e)))?;
    insert_game_metrics(
        &mut *conn,
        &GameMetricsRow {
            game_id: target.game_id,
            account_id: target.account_id,
            time_class: tc.to_owned(),
            played_at: target.played_at,
            protocol: ANALYSIS_PROTOCOL.to_owned(),
            enrichment_protocol: Some(ENRICHMENT_PROTOCOL.to_owned()),
            difficulty_model: difficulty_model.map(str::to_owned),
            metrics,
        },
    )
    .await?;

    let plies = opening_plies(&parsed.plies);
    let mut fen_keys: Vec<&str> = plies
        .first()
        .map(|p| p.fen_key_before.as_str())
        .into_iter()
        .collect();
    fen_keys.extend(plies.iter().map(|p| p.fen_key_after.as_str()));
    let ucis: Vec<&str> = plies.iter().map(|p| p.uci.as_str()).collect();
    let sans: Vec<&str> = plies.iter().map(|p| p.san.as_str()).collect();
    let line = features.opening.as_ref();
    let (eco, name) = opening_name(
        line,
        target.platform_eco.as_deref(),
        target.platform_opening_name.as_deref(),
    );
    let ply = |v: Option<u32>| v.and_then(|p| i32::try_from(p).ok());
    sqlx::query(
        "INSERT INTO game_openings (game_id, account_id, time_class, color, fen_keys, ucis, \
         sans, line_key, line_name, eco, line_kind, line_entry_ply, book_exit_ply, \
         player_deviation_ply) \
         VALUES ($1, $2, $3::time_class, $4::color, $5, $6, $7, $8, $9, $10, $11::line_kind, \
         $12, $13, $14) \
         ON CONFLICT (game_id) DO UPDATE SET account_id = EXCLUDED.account_id, \
         time_class = EXCLUDED.time_class, color = EXCLUDED.color, \
         fen_keys = EXCLUDED.fen_keys, ucis = EXCLUDED.ucis, sans = EXCLUDED.sans, \
         line_key = EXCLUDED.line_key, line_name = EXCLUDED.line_name, eco = EXCLUDED.eco, \
         line_kind = EXCLUDED.line_kind, line_entry_ply = EXCLUDED.line_entry_ply, \
         book_exit_ply = EXCLUDED.book_exit_ply, \
         player_deviation_ply = EXCLUDED.player_deviation_ply",
    )
    .bind(target.game_id)
    .bind(target.account_id)
    .bind(tc)
    .bind(target.player_color.as_str())
    .bind(&fen_keys)
    .bind(&ucis)
    .bind(&sans)
    .bind(line.map(|l| l.key.as_str()))
    .bind(name)
    .bind(eco)
    .bind(line.map(|l| l.kind.as_str()))
    .bind(ply(line.map(|l| l.entry_ply)))
    .bind(ply(features.book_exit_ply))
    .bind(ply(features.player_deviation_ply))
    .execute(&mut *conn)
    .await?;

    let count = |n: u32| i32::try_from(n).unwrap_or(i32::MAX);
    let rows: Vec<SeriesPointRow> = features
        .observation
        .series
        .iter()
        .map(|s| SeriesPointRow {
            account_id: target.account_id,
            time_class: tc.to_owned(),
            series_key: s.series_key.to_string(),
            played_at: target.played_at,
            events: count(s.events),
            opportunities: count(s.opportunities),
            epl: s.epl,
            protocol: ANALYSIS_PROTOCOL.to_owned(),
        })
        .collect();
    replace_series_points(&mut *conn, target.game_id, &rows).await
}

/// The target of a game row (`Err` names a column that does not parse).
pub fn derived_target(game: &db::games::GameRow) -> Result<DerivedTarget, String> {
    Ok(DerivedTarget {
        game_id: game.id,
        account_id: game.account_id,
        time_class: parse_time_class(&game.time_class)
            .ok_or_else(|| format!("time_class {:?}", game.time_class))?,
        played_at: game.played_at,
        player_color: parse_color(&game.player_color)
            .ok_or_else(|| format!("player_color {:?}", game.player_color))?,
        platform_eco: game.eco.clone(),
        platform_opening_name: game.opening_name.clone(),
    })
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

impl AnalyseHook for ProfileHook {
    fn prepare<'a>(
        &'a self,
        i: HookInput<'a>,
    ) -> BoxFuture<'a, Result<Box<dyn PreparedHook>, HookError>> {
        Box::pin(async move {
            let started = Instant::now();
            let game = i.game;
            let target = derived_target(game).map_err(HookError::Failed)?;
            let player = target.player_color;
            let deps = EnrichDeps {
                analyser: self.enrich.analyser.clone(),
                store: self.enrich.store.clone(),
                tb: self.enrich.tb.clone(),
                difficulty: self.enrich.difficulty.clone(),
                cancel: i.cancel.clone(),
            };
            let input = EnrichInput {
                parsed: i.parsed,
                evals: i.evals,
                records: i.records,
                colors: &[player],
                elo_white: game.white_rating,
                elo_black: game.black_rating,
            };
            let enriched = enrich_game(&deps, &input).await.map_err(|e| match e {
                EnrichError::Cancelled | EnrichError::Engine(EngineError::PoolClosed) => {
                    HookError::Cancelled
                }
                other => HookError::Failed(other.to_string()),
            })?;
            if i.cancel.is_cancelled() {
                return Err(HookError::Cancelled);
            }

            let t = Instant::now();
            let wins: Vec<f64> = i.evals.iter().map(|e| win_percent(e.score)).collect();
            let accuracy = (!i.records.is_empty()).then(|| game_accuracy(&wins, player));
            let previous = previous_own_rating(&self.pool, game.id)
                .await
                .map_err(|e| HookError::Failed(format!("previous rating: {e}")))?;
            let ctx = game_context(game, accuracy, ANALYSIS_PROTOCOL, previous)
                .map_err(HookError::Failed)?;
            let explorer =
                load_game_explorer(&self.pool, game.account_id, target.time_class, i.parsed).await;
            let explorer_ms = ms(t);
            let features = compute_game_features(FeatureInput {
                ctx: &ctx,
                parsed: i.parsed,
                evals: i.evals,
                records: i.records,
                raw: &enriched.raw,
                explorer: explorer.as_ref(),
            });
            let mut stages = enriched.stages.clone();
            stages.insert("features_ms".to_owned(), ms(t));
            stages.insert("explorer_ms".to_owned(), explorer_ms);
            stages.insert("enrich_ms".to_owned(), ms(started) - ms(t));
            stages.extend(features.counts.clone());
            Ok(Box::new(PreparedProfile {
                pool: self.pool.clone(),
                queue: self.queue.clone(),
                target,
                parsed: i.parsed.clone(),
                records: i.records.to_vec(),
                raw: enriched.raw,
                difficulty: enriched.difficulty,
                difficulty_model: enriched.difficulty_model,
                features,
                stages,
                prepare_ms: ms(started),
            }) as Box<dyn PreparedHook>)
        })
    }
}

/// One game's prepared rows.
struct PreparedProfile {
    pool: PgPool,
    queue: Queue,
    target: DerivedTarget,
    parsed: chess_core::ParsedGame,
    records: Vec<MoveRecord>,
    raw: Vec<Option<RawMoveFeatures>>,
    difficulty: Vec<Option<DifficultyOutcome>>,
    difficulty_model: Option<String>,
    features: GameFeatures,
    stages: std::collections::BTreeMap<String, f64>,
    prepare_ms: f64,
}

impl PreparedHook for PreparedProfile {
    fn persist<'a>(&'a self, tx: &'a mut PgConnection) -> BoxFuture<'a, Result<(), sqlx::Error>> {
        Box::pin(async move {
            let game_id = self.target.game_id;
            persist_move_features(
                &mut *tx,
                game_id,
                &self.records,
                &self.raw,
                Some(&self.features.derived),
                &self.difficulty,
            )
            .await?;
            write_derived_rows(
                &mut *tx,
                &self.target,
                &self.features,
                &self.parsed,
                self.difficulty_model.as_deref(),
            )
            .await?;
            sqlx::query(
                "UPDATE games SET enrichment_protocol = $2, analysed_at = now(), \
                 profile_stale_at = NULL WHERE id = $1",
            )
            .bind(game_id)
            .bind(ENRICHMENT_PROTOCOL)
            .execute(&mut *tx)
            .await?;
            // R-13: the full pass replaces the preview, in the done transition's transaction.
            sqlx::query("DELETE FROM game_previews WHERE game_id = $1")
                .bind(game_id)
                .execute(&mut *tx)
                .await?;
            Ok(())
        })
    }

    fn after_commit(self: Box<Self>) -> BoxFuture<'static, ()> {
        Box::pin(async move {
            let t = &self.target;
            if let Err(e) = enqueue_build_profile(&self.queue, t.account_id, t.time_class).await {
                // The next analysed game (or build-profile's own schedule) enqueues it again.
                tracing::warn!(game_id = %t.game_id, error = %e, "enqueue build-profile failed");
            }
            let m = StageMetrics {
                job_id: None,
                kind: ENRICH_GAME_KIND.to_owned(),
                game_id: Some(t.game_id),
                account_id: Some(t.account_id),
                total_ms: self.prepare_ms,
                stages: self.stages.clone(),
            };
            if let Err(e) = record_stage_metrics(&self.pool, &m).await {
                tracing::warn!(game_id = %t.game_id, error = %e, "recording enrich-game metrics failed");
            }
            tracing::info!(
                game_id = %t.game_id,
                total_ms = self.prepare_ms,
                stages = ?self.stages,
                "enrich-game"
            );
        })
    }
}

/// `Arc` form for `AnalyseGameDeps.hook`.
pub fn into_hook(hook: ProfileHook) -> Arc<dyn AnalyseHook> {
    Arc::new(hook)
}

/// Builds the opening dataset off the async threads (about 20 ms of CPU the first time, T16), so
/// the first analysed game does not block a runtime thread. Call once at worker start.
pub async fn warm_openings() {
    if let Err(e) =
        tokio::task::spawn_blocking(|| profile_core::openings::dataset::openings().len()).await
    {
        tracing::warn!(error = %e, "warming the opening dataset failed");
    }
}

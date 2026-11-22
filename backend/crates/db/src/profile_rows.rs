//! Row helpers for the profile-engine tables (R2 Tasks 31–33). Plain runtime-checked statements;
//! enum columns are bound as their Postgres text values with casts, numbers as `f64` (stored as
//! `double precision` in `weakness_estimates` and `kpi_snapshot`, so they round-trip exactly;
//! `real` elsewhere). Each takes a `&mut PgConnection`, so it runs on a pooled connection or
//! inside a caller's transaction (`&mut *tx`).
//!
//! `game_metrics.metrics` holds a serialised `PlayerGameMetrics`. Readers of that column must
//! apply the version gate: a row whose `metrics->>'version'` differs from
//! `PLAYER_GAME_METRICS_VERSION` is stale, never clean zero-evidence data.

use sqlx::PgConnection;
use time::OffsetDateTime;
use uuid::Uuid;

/// One `game_metrics` row. `time_class` is the `time_class` enum text.
#[derive(Debug, Clone, PartialEq)]
pub struct GameMetricsRow {
    pub game_id: Uuid,
    pub account_id: Uuid,
    pub time_class: String,
    pub played_at: OffsetDateTime,
    pub protocol: String,
    pub enrichment_protocol: Option<String>,
    pub difficulty_model: Option<String>,
    pub metrics: serde_json::Value,
}

/// Writes a game's metrics. One row per game: a re-analysis replaces the previous row (and resets
/// `created_at`).
pub async fn insert_game_metrics(
    conn: &mut PgConnection,
    row: &GameMetricsRow,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO game_metrics (game_id, account_id, time_class, played_at, protocol, \
         enrichment_protocol, difficulty_model, metrics) \
         VALUES ($1, $2, $3::time_class, $4, $5, $6, $7, $8) \
         ON CONFLICT (game_id) DO UPDATE SET account_id = EXCLUDED.account_id, \
         time_class = EXCLUDED.time_class, played_at = EXCLUDED.played_at, \
         protocol = EXCLUDED.protocol, enrichment_protocol = EXCLUDED.enrichment_protocol, \
         difficulty_model = EXCLUDED.difficulty_model, metrics = EXCLUDED.metrics, \
         created_at = now()",
    )
    .bind(row.game_id)
    .bind(row.account_id)
    .bind(&row.time_class)
    .bind(row.played_at)
    .bind(&row.protocol)
    .bind(&row.enrichment_protocol)
    .bind(&row.difficulty_model)
    .bind(&row.metrics)
    .execute(conn)
    .await?;
    Ok(())
}

/// One `weakness_series_point` row (the game id is given to [`replace_series_points`]).
#[derive(Debug, Clone, PartialEq)]
pub struct SeriesPointRow {
    pub account_id: Uuid,
    pub time_class: String,
    pub series_key: String,
    pub played_at: OffsetDateTime,
    pub events: i32,
    pub opportunities: i32,
    pub epl: f64,
    pub protocol: String,
}

/// Replaces every series point of `game_id` with `rows` (delete, then one bulk insert). Run it in
/// the caller's persist transaction so readers never see a half-replaced game.
pub async fn replace_series_points(
    conn: &mut PgConnection,
    game_id: Uuid,
    rows: &[SeriesPointRow],
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM weakness_series_point WHERE game_id = $1")
        .bind(game_id)
        .execute(&mut *conn)
        .await?;
    if rows.is_empty() {
        return Ok(());
    }
    let accounts: Vec<Uuid> = rows.iter().map(|r| r.account_id).collect();
    let time_classes: Vec<&str> = rows.iter().map(|r| r.time_class.as_str()).collect();
    let keys: Vec<&str> = rows.iter().map(|r| r.series_key.as_str()).collect();
    let played: Vec<OffsetDateTime> = rows.iter().map(|r| r.played_at).collect();
    let events: Vec<i32> = rows.iter().map(|r| r.events).collect();
    let opps: Vec<i32> = rows.iter().map(|r| r.opportunities).collect();
    let epl: Vec<f64> = rows.iter().map(|r| r.epl).collect();
    let protocols: Vec<&str> = rows.iter().map(|r| r.protocol.as_str()).collect();
    sqlx::query(
        "INSERT INTO weakness_series_point (account_id, time_class, series_key, game_id, \
         played_at, events, opportunities, epl, protocol) \
         SELECT a, tc::time_class, k, $9, p, e, o, l, pr \
         FROM UNNEST($1::uuid[], $2::text[], $3::text[], $4::timestamptz[], $5::int4[], \
         $6::int4[], $7::float8[], $8::text[]) AS t(a, tc, k, p, e, o, l, pr)",
    )
    .bind(&accounts)
    .bind(&time_classes)
    .bind(&keys)
    .bind(&played)
    .bind(&events)
    .bind(&opps)
    .bind(&epl)
    .bind(&protocols)
    .bind(game_id)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// The current estimate of one series (`weakness_estimates`). `state` is the `weakness_state`
/// enum text. Optional fields are null when there is no benchmark.
#[derive(Debug, Clone, PartialEq)]
pub struct WeaknessEstimateRow {
    pub account_id: Uuid,
    pub time_class: String,
    /// `side_scope` text: `both`, `white` or `black` (the colour split).
    pub side: String,
    pub series_key: String,
    pub k: f64,
    pub n: f64,
    pub n_unweighted: i32,
    pub k_unweighted: f64,
    pub alpha: f64,
    pub beta: f64,
    pub mean: f64,
    pub ci_low: f64,
    pub ci_high: f64,
    pub band_median: Option<f64>,
    pub band_p40: Option<f64>,
    pub confidence: f64,
    pub excess: Option<f64>,
    pub priority: f64,
    pub percentile: Option<f64>,
    pub state: String,
    pub p_imp: Option<f64>,
}

/// Inserts or overwrites the estimate for `(account_id, time_class, side, series_key)`.
pub async fn upsert_weakness_estimate(
    conn: &mut PgConnection,
    row: &WeaknessEstimateRow,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO weakness_estimates (account_id, time_class, series_key, k, n, n_unweighted, \
         k_unweighted, alpha, beta, mean, ci_low, ci_high, band_median, band_p40, confidence, \
         excess, priority, percentile, state, p_imp, side, updated_at) \
         VALUES ($1, $2::time_class, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, \
         $16, $17, $18, $19::weakness_state, $20, $21::side_scope, now()) \
         ON CONFLICT (account_id, time_class, side, series_key) DO UPDATE SET k = EXCLUDED.k, \
         n = EXCLUDED.n, n_unweighted = EXCLUDED.n_unweighted, \
         k_unweighted = EXCLUDED.k_unweighted, alpha = EXCLUDED.alpha, beta = EXCLUDED.beta, \
         mean = EXCLUDED.mean, ci_low = EXCLUDED.ci_low, ci_high = EXCLUDED.ci_high, \
         band_median = EXCLUDED.band_median, band_p40 = EXCLUDED.band_p40, \
         confidence = EXCLUDED.confidence, excess = EXCLUDED.excess, \
         priority = EXCLUDED.priority, percentile = EXCLUDED.percentile, \
         state = EXCLUDED.state, p_imp = EXCLUDED.p_imp, updated_at = now()",
    )
    .bind(row.account_id)
    .bind(&row.time_class)
    .bind(&row.series_key)
    .bind(row.k)
    .bind(row.n)
    .bind(row.n_unweighted)
    .bind(row.k_unweighted)
    .bind(row.alpha)
    .bind(row.beta)
    .bind(row.mean)
    .bind(row.ci_low)
    .bind(row.ci_high)
    .bind(row.band_median)
    .bind(row.band_p40)
    .bind(row.confidence)
    .bind(row.excess)
    .bind(row.priority)
    .bind(row.percentile)
    .bind(&row.state)
    .bind(row.p_imp)
    .bind(&row.side)
    .execute(conn)
    .await?;
    Ok(())
}

/// One `weakness_state_history` row. `kind` is `state_row_kind` text (`transition` | `onset`);
/// `from_state`/`to_state` are `weakness_state` text (`from_state = None` on a series' first row).
#[derive(Debug, Clone, PartialEq)]
pub struct StateHistoryRow {
    pub account_id: Uuid,
    pub time_class: String,
    pub series_key: String,
    pub kind: String,
    pub from_state: Option<String>,
    pub to_state: String,
    pub at_game_id: Option<Uuid>,
    pub at_played_at: Option<OffsetDateTime>,
    pub onset_game_id: Option<Uuid>,
    pub onset_at: Option<OffsetDateTime>,
    pub onset_low: Option<OffsetDateTime>,
    pub onset_high: Option<OffsetDateTime>,
    pub p_change: Option<f64>,
    pub rate_before: Option<f64>,
    pub rate_after: Option<f64>,
    pub evidence: serde_json::Value,
    pub cause_hints: serde_json::Value,
    pub detector_version: String,
    pub protocol: String,
}

/// Appends a history row and returns its id. Deciding whether a row is due is the caller's job.
pub async fn append_state_history(
    conn: &mut PgConnection,
    row: &StateHistoryRow,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        "INSERT INTO weakness_state_history (account_id, time_class, series_key, kind, \
         from_state, to_state, at_game_id, at_played_at, onset_game_id, onset_at, onset_low, \
         onset_high, p_change, rate_before, rate_after, evidence, cause_hints, detector_version, \
         protocol) \
         VALUES ($1, $2::time_class, $3, $4::state_row_kind, $5::weakness_state, \
         $6::weakness_state, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19) \
         RETURNING id",
    )
    .bind(row.account_id)
    .bind(&row.time_class)
    .bind(&row.series_key)
    .bind(&row.kind)
    .bind(&row.from_state)
    .bind(&row.to_state)
    .bind(row.at_game_id)
    .bind(row.at_played_at)
    .bind(row.onset_game_id)
    .bind(row.onset_at)
    .bind(row.onset_low)
    .bind(row.onset_high)
    .bind(row.p_change)
    .bind(row.rate_before)
    .bind(row.rate_after)
    .bind(&row.evidence)
    .bind(&row.cause_hints)
    .bind(&row.detector_version)
    .bind(&row.protocol)
    .fetch_one(conn)
    .await
}

/// One `kpi_snapshot` row (the DB shape; `profile-core` has its own `KpiSnapshotRow`).
/// `window_kind` is `kpi_window` text (`last_n` | `week`).
#[derive(Debug, Clone, PartialEq)]
pub struct KpiSnapshotRow {
    pub user_id: Option<Uuid>,
    pub account_id: Uuid,
    pub time_class: String,
    /// `side_scope` text: `both`, `white` or `black` (the colour split).
    pub side: String,
    pub metric: String,
    pub window_kind: String,
    pub window_key: String,
    pub value: Option<f64>,
    pub value_adjusted: Option<f64>,
    pub n: i32,
    pub ci_low: Option<f64>,
    pub ci_high: Option<f64>,
    pub protocol: String,
}

/// Appends a snapshot (append-only: identical rows are kept) and returns its id. Deduplicating
/// against the latest row is the caller's job.
pub async fn append_kpi_snapshot(
    conn: &mut PgConnection,
    row: &KpiSnapshotRow,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        "INSERT INTO kpi_snapshot (user_id, account_id, time_class, metric, window_kind, \
         window_key, value, value_adjusted, n, ci_low, ci_high, protocol, side) \
         VALUES ($1, $2, $3::time_class, $4, $5::kpi_window, $6, $7, $8, $9, $10, $11, $12, \
         $13::side_scope) \
         RETURNING id",
    )
    .bind(row.user_id)
    .bind(row.account_id)
    .bind(&row.time_class)
    .bind(&row.metric)
    .bind(&row.window_kind)
    .bind(&row.window_key)
    .bind(row.value)
    .bind(row.value_adjusted)
    .bind(row.n)
    .bind(row.ci_low)
    .bind(row.ci_high)
    .bind(&row.protocol)
    .bind(&row.side)
    .fetch_one(conn)
    .await
}

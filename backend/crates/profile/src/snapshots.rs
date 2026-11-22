//! KPI snapshot persistence (R2 Task 33, TS M2 Task 25 steps 5, 6 and the `profile.updated`
//! part of step 8).
//!
//! - **Windows.** `last50` and every ISO week that holds a loaded game. TS recomputes only the
//!   weeks holding a game analysed since the last snapshot. That check is not safe on
//!   timestamps: `analysed_at` is the analysis transaction's *start* (`now()`), so a game that
//!   started before a snapshot's `computed_at` but committed after that build's load would
//!   never get its week. Every week is recomputed instead; because the computation depends only
//!   on the set of games and rows are appended only when `value` or `n` differ from the latest
//!   stored row, the stored result is what the TS rule gives, plus the weeks it would miss.
//! - **Area scores** are computed per window from that window's rows and stored as
//!   `area:<area>` rows (`value` = score, `n`, no interval).
//! - **Change detection** compares `value` and `n` exactly with the latest row (highest id) of
//!   the same metric and window (`double precision`, so f64 round-trips).
//! - **Events.** One `profile.updated` per area with an appended row (component KPI or its area
//!   score, in any window), except areas without a benchmark (mindset). The payload lists the
//!   area's current `last50` rows.

use std::collections::{BTreeSet, HashMap};

use chess_core::TimeClass;
use db::profile_rows::KpiSnapshotRow;
use profile_core::kpi::area_scores::compute_area_scores;
use profile_core::kpi::catalogue::{KpiGame, kpi_definition};
use profile_core::kpi::iso_week::iso_week_key;
use profile_core::kpi::snapshots::{KpiValue, Window, compute_kpis};
use profile_core::types::{Area, BandReference, MetricId};
use serde_json::{Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

/// The profile's recent window.
pub const LAST_N: usize = 50;

/// The windows of a build: `last50`, then every ISO week holding a game, oldest first.
pub fn windows(games: &[KpiGame<'_>]) -> Vec<Window> {
    let weeks: BTreeSet<String> = games
        .iter()
        .map(|g| iso_week_key(g.ctx.played_at))
        .collect();
    std::iter::once(Window::LastN(LAST_N))
        .chain(weeks.into_iter().map(Window::Week))
        .collect()
}

/// The KPI rows and area-score rows of one window.
pub fn window_rows(
    games: &[KpiGame<'_>],
    w: &Window,
    reference: Option<&BandReference>,
) -> Vec<KpiValue> {
    let mut rows = compute_kpis(games, w, reference);
    let areas = compute_area_scores(&rows, reference);
    rows.extend(areas.into_iter().map(|a| KpiValue {
        metric: MetricId::Area(a.area),
        window_kind: w.kind(),
        window_key: w.key(),
        value: a.score,
        value_adjusted: None,
        value_adjusted_raw: None,
        n: a.n,
        ci_low: None,
        ci_high: None,
    }));
    rows
}

/// Every row of every window of the build.
pub fn compute_snapshots(
    games: &[KpiGame<'_>],
    reference: Option<&BandReference>,
) -> Vec<KpiValue> {
    windows(games)
        .iter()
        .flat_map(|w| window_rows(games, w, reference))
        .collect()
}

/// `(metric, window_key)` -> `(value, n)` of the latest stored row.
pub type Latest = HashMap<(String, String), (Option<f64>, i32)>;

/// The latest stored row per metric and window of the account × time class.
pub async fn latest_snapshots(
    conn: &mut PgConnection,
    account_id: Uuid,
    tc: TimeClass,
) -> Result<Latest, sqlx::Error> {
    let rows: Vec<(String, String, Option<f64>, i32)> = sqlx::query_as(
        "SELECT DISTINCT ON (metric, window_key) metric, window_key, value, n \
         FROM kpi_snapshot WHERE account_id = $1 AND time_class = $2::time_class \
         ORDER BY metric, window_key, id DESC",
    )
    .bind(account_id)
    .bind(tc.as_str())
    .fetch_all(conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(m, w, v, n)| ((m, w), (v, n)))
        .collect())
}

fn n_of(v: &KpiValue) -> i32 {
    i32::try_from(v.n).unwrap_or(i32::MAX)
}

/// The rows whose `value` or `n` differ from the latest stored row (or that have none).
pub fn rows_to_append<'a>(rows: &'a [KpiValue], latest: &Latest) -> Vec<&'a KpiValue> {
    rows.iter()
        .filter(|r| {
            latest
                .get(&(r.metric.to_string(), r.window_key.clone()))
                .is_none_or(|(value, n)| *value != r.value || *n != n_of(r))
        })
        .collect()
}

/// The `kpi_snapshot` row of a value.
pub fn snapshot_row(
    user_id: Option<Uuid>,
    account_id: Uuid,
    tc: TimeClass,
    v: &KpiValue,
    protocol: &str,
) -> KpiSnapshotRow {
    KpiSnapshotRow {
        user_id,
        account_id,
        time_class: tc.as_str().to_owned(),
        metric: v.metric.to_string(),
        window_kind: v.window_kind.as_str().to_owned(),
        window_key: v.window_key.clone(),
        value: v.value,
        value_adjusted: v.value_adjusted,
        n: n_of(v),
        ci_low: v.ci_low,
        ci_high: v.ci_high,
        protocol: protocol.to_owned(),
    }
}

/// The area a row belongs to (`area:<area>` rows to their area, KPI rows to their catalogue
/// area; overall rows to none).
pub fn row_area(metric: MetricId) -> Option<Area> {
    match metric {
        MetricId::Area(a) => Some(a),
        m => kpi_definition(m).and_then(|d| d.area),
    }
}

/// Areas without a band benchmark in this milestone (no `profile.updated` for them).
pub fn has_benchmark(area: Area) -> bool {
    area != Area::Mindset
}

/// The benchmarked areas with an appended row, in contract order.
pub fn changed_areas(appended: &[&KpiValue]) -> Vec<Area> {
    let set: BTreeSet<usize> = appended
        .iter()
        .filter_map(|v| row_area(v.metric))
        .filter(|a| has_benchmark(*a))
        .filter_map(|a| Area::ALL.iter().position(|x| *x == a))
        .collect();
    set.into_iter().map(|i| Area::ALL[i]).collect()
}

/// The `profile.updated` payload of an area: its `last50` rows (catalogue order, then the area
/// score).
pub fn profile_updated_payload(tc: TimeClass, area: Area, rows: &[KpiValue]) -> Value {
    let last = Window::LastN(LAST_N).key();
    let kpis: Vec<Value> = rows
        .iter()
        .filter(|r| r.window_key == last && row_area(r.metric) == Some(area))
        .map(|r| {
            json!({
                "metric": r.metric.to_string(),
                "value": r.value,
                "n": r.n,
                "ciLow": r.ci_low,
                "ciHigh": r.ci_high,
            })
        })
        .collect();
    json!({ "timeClass": tc.as_str(), "area": area.as_str(), "kpis": kpis })
}

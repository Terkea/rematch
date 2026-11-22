//! The weekly-review input (M5b Task B, ruling G2; TS plan B4 `WeeklyReviewInput`), built from
//! R2 data for one account × time class × ISO week (Monday to Sunday, UTC). ADR 0075.
//!
//! The shape is the R3 contract `ai::output::weekly::WeeklyReviewInput` field for field. This
//! crate does not depend on `ai`, so [`WeeklyInput`] mirrors it, and the tests validate the
//! output against the committed schema `fixtures/ai/input/weekly-review-input.schema.json` that
//! `ai` generates from its type.
//!
//! - **No review** ([`build_weekly_input`] gives `None`) for a week without a game in the time
//!   class, or whose games have no current-version metrics yet: there is nothing measured to
//!   review, and a review is never written about nothing.
//! - `gamesThisWeek`: the games played in the week; `basis`, `band`: the profile's
//!   ([`build_coaching_input`] at `now`, the time the review is written).
//! - `adherence`: `null`. No practice is planned or tracked yet, and "0 of 0 minutes" would claim
//!   a plan that does not exist.
//! - `metricDeltas`: every contract metric (R-2) with a weekly KPI snapshot both in this week
//!   (`after`) and in the latest earlier week that has one (`before`), from the latest stored row
//!   of each. The verdict is [`delta_verdict`]: a change is only called when both weeks have
//!   enough evidence and the two stored 80% intervals do not overlap.
//! - `focusUpdates`: the top 3 ranked weaknesses of the profile (the climb plan's focuses come
//!   from the same ranking), one per weakness id, with their current state and `P_imp`.
//! - `exemplars`: up to 2 positions of those focuses played in the week, from distinct games, in
//!   the profile's exemplar order.
//! - `changes`: the profile's timeline changes whose onset row was detected (`at_played_at`) in
//!   the week.

use std::collections::HashSet;

use chess_core::TimeClass;
use coaching_input::{
    AreaId, BandId, Basis, CoachingInput, ExemplarPosition, TimelineChange, WeaknessState,
};
use profile_core::kpi::catalogue::kpi_definition;
use profile_core::kpi::iso_week::iso_week_key;
use profile_core::kpi::snapshots::{KpiValue, WindowKind};
use profile_core::types::{MetricId, PLAYER_GAME_METRICS_VERSION, SeriesKey};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use time::{Date, Duration, OffsetDateTime, Time};
use uuid::Uuid;

use super::builder::{BuildError, build_coaching_input};
use super::mapping::{contract_line_key, metric};
use crate::analyse_features::parse_time_class;

/// `inputVersion`.
pub const WEEKLY_INPUT_VERSION: &str = "weekly-input-v1";
/// Focuses per week (the climb plan's at most 3).
pub const MAX_FOCUSES: usize = 3;
/// TS `exemplars: ExemplarPosition[] (max 2)`.
pub const MAX_EXEMPLARS: usize = 2;

/// Practice done against plan in the week.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Adherence {
    pub minutes_done: u32,
    pub minutes_planned: u32,
}

/// A metric at one end of the comparison.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MetricPoint {
    pub value: f64,
    pub n: u32,
    #[serde(deserialize_with = "coaching_input::nullable")]
    pub low: Option<f64>,
    #[serde(deserialize_with = "coaching_input::nullable")]
    pub high: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeltaVerdict {
    Improved,
    Worse,
    NoClearChange,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MetricDelta {
    pub key: String,
    pub area: AreaId,
    pub label: String,
    pub before: MetricPoint,
    pub after: MetricPoint,
    pub verdict: DeltaVerdict,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FocusUpdate {
    pub weakness_id: String,
    pub label: String,
    #[serde(deserialize_with = "coaching_input::nullable")]
    pub p_imp: Option<f64>,
    pub state: WeaknessState,
}

/// TS `WeeklyReviewInput` (plan B4); see the module docs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WeeklyInput {
    pub input_version: String,
    pub time_class: coaching_input::TimeClass,
    pub band: BandId,
    #[serde(with = "coaching_input::iso_date")]
    pub week_start: Date,
    #[serde(with = "coaching_input::iso_date")]
    pub week_end: Date,
    pub games_this_week: u32,
    pub basis: Basis,
    #[serde(deserialize_with = "coaching_input::nullable")]
    pub adherence: Option<Adherence>,
    pub metric_deltas: Vec<MetricDelta>,
    pub focus_updates: Vec<FocusUpdate>,
    pub exemplars: Vec<ExemplarPosition>,
    pub changes: Vec<TimelineChange>,
}

/// The verdict of a weekly change: `improved` or `worse` only when both weeks have enough
/// evidence (`sufficient`), the metric has a better direction, and the two intervals are
/// disjoint; otherwise `no_clear_change`. A point without an interval never gets a verdict.
pub fn delta_verdict(
    before: &MetricPoint,
    after: &MetricPoint,
    higher_is_better: Option<bool>,
    sufficient: bool,
) -> DeltaVerdict {
    let (Some(higher), true) = (higher_is_better, sufficient) else {
        return DeltaVerdict::NoClearChange;
    };
    let (Some(bl), Some(bh), Some(al), Some(ah)) = (before.low, before.high, after.low, after.high)
    else {
        return DeltaVerdict::NoClearChange;
    };
    let rose = al > bh;
    let fell = ah < bl;
    match (rose, fell) {
        (true, _) if higher => DeltaVerdict::Improved,
        (true, _) => DeltaVerdict::Worse,
        (_, true) if higher => DeltaVerdict::Worse,
        (_, true) => DeltaVerdict::Improved,
        _ => DeltaVerdict::NoClearChange,
    }
}

fn midnight(d: Date) -> OffsetDateTime {
    d.with_time(Time::MIDNIGHT).assume_utc()
}

/// The week's bounds: `[start, end)` as instants.
fn bounds(week_start: Date) -> (OffsetDateTime, OffsetDateTime) {
    let start = midnight(week_start);
    (start, start + Duration::days(7))
}

/// The accounts × time classes with a game played in the week of `week_start` and at least one
/// user linked to the account (an `account_links` row, or the CLI's `linked_accounts.user_id`):
/// nobody else would read the review. Sorted.
pub async fn weekly_due(pool: &PgPool, week_start: Date) -> sqlx::Result<Vec<(Uuid, TimeClass)>> {
    let (start, end) = bounds(week_start);
    let rows: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT DISTINCT g.account_id, g.time_class::text FROM games g \
         JOIN linked_accounts la ON la.id = g.account_id \
         WHERE g.played_at >= $1 AND g.played_at < $2 \
           AND (la.user_id IS NOT NULL OR EXISTS (SELECT 1 FROM account_links al \
             WHERE al.linked_account_id = g.account_id)) \
         ORDER BY 1, 2",
    )
    .bind(start)
    .bind(end)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(|(a, tc)| parse_time_class(&tc).map(|tc| (a, tc)))
        .collect())
}

/// The latest weekly row per (metric, week) up to `week_key`.
async fn weekly_rows(
    pool: &PgPool,
    account_id: Uuid,
    tc: TimeClass,
    week_key: &str,
) -> sqlx::Result<Vec<KpiValue>> {
    type Raw = (
        String,
        String,
        Option<f64>,
        Option<f64>,
        i32,
        Option<f64>,
        Option<f64>,
    );
    let rows: Vec<Raw> = sqlx::query_as(
        "SELECT DISTINCT ON (metric, window_key) metric, window_key, value, value_adjusted, n, \
           ci_low, ci_high FROM kpi_snapshot \
         WHERE account_id = $1 AND time_class = $2::time_class AND side = 'both' \
           AND window_kind = 'week' \
           AND window_key <= $3 \
         ORDER BY metric, window_key, id DESC",
    )
    .bind(account_id)
    .bind(tc.as_str())
    .bind(week_key)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(|(m, key, value, value_adjusted, n, ci_low, ci_high)| {
            Some(KpiValue {
                metric: m.parse::<MetricId>().ok()?,
                window_kind: WindowKind::Week,
                window_key: key,
                value,
                value_adjusted,
                value_adjusted_raw: None,
                n: u32::try_from(n).ok()?,
                ci_low,
                ci_high,
            })
        })
        .collect())
}

fn point(m: &coaching_input::Metric) -> MetricPoint {
    MetricPoint {
        value: m.value,
        n: m.n,
        low: m.low,
        high: m.high,
    }
}

/// `metricDeltas` (module docs), in catalogue-id order.
fn metric_deltas(rows: &[KpiValue], week_key: &str) -> Vec<MetricDelta> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < rows.len() {
        let id = rows[i].metric;
        let mut j = i;
        while j < rows.len() && rows[j].metric == id {
            j += 1;
        }
        // Rows of one metric, by week ascending.
        let group = &rows[i..j];
        i = j;
        let Some(def) = kpi_definition(id) else {
            continue;
        };
        let Some(after_row) = group.iter().find(|r| r.window_key == week_key) else {
            continue;
        };
        let Some(before_row) = group
            .iter()
            .rfind(|r| r.window_key.as_str() < week_key && r.value.is_some())
        else {
            continue;
        };
        let (Some(after), Some(before)) =
            (metric(def, after_row, None), metric(def, before_row, None))
        else {
            continue;
        };
        let (b, a) = (point(&before), point(&after));
        out.push(MetricDelta {
            verdict: delta_verdict(
                &b,
                &a,
                def.higher_is_better,
                before.sufficient && after.sufficient,
            ),
            key: after.key,
            area: after.area,
            label: after.label,
            before: b,
            after: a,
        });
    }
    out
}

/// The contract series key of a stored history key (R-7, as the builder maps it).
fn contract_series_key(raw: &str) -> Option<String> {
    match raw.parse::<SeriesKey>().ok()? {
        SeriesKey::Opening(line) => Some(format!("opening:{}", contract_line_key(&line))),
        other => Some(other.to_string()),
    }
}

/// `changes`: the profile's changes whose onset row was detected in the week.
async fn changes_in_week(
    pool: &PgPool,
    account_id: Uuid,
    tc: TimeClass,
    week_start: Date,
    ci: &CoachingInput,
) -> sqlx::Result<Vec<TimelineChange>> {
    let (start, end) = bounds(week_start);
    let rows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT h.series_key, g.platform_game_id, h.to_state::text \
         FROM weakness_state_history h JOIN games g ON g.id = h.onset_game_id \
         WHERE h.account_id = $1 AND h.time_class = $2::time_class AND h.kind = 'onset' \
           AND h.at_played_at >= $3 AND h.at_played_at < $4",
    )
    .bind(account_id)
    .bind(tc.as_str())
    .bind(start)
    .bind(end)
    .fetch_all(pool)
    .await?;
    let detected: HashSet<(String, String, String)> = rows
        .into_iter()
        .filter_map(|(k, g, s)| Some((contract_series_key(&k)?, g, s)))
        .collect();
    Ok(ci
        .changes
        .iter()
        .filter(|c| {
            detected.contains(&(
                c.series_key.clone(),
                c.onset_game_id.clone(),
                c.to_state.as_str().to_owned(),
            ))
        })
        .cloned()
        .collect())
}

/// Builds the weekly-review input of the week starting `week_start` (a Monday), or `None` when
/// there is nothing to review (module docs). `now` is when the review is written: the profile
/// (band, basis, focuses, changes) is read as of then.
pub async fn build_weekly_input(
    pool: &PgPool,
    account_id: Uuid,
    tc: TimeClass,
    week_start: Date,
    now: OffsetDateTime,
) -> Result<Option<WeeklyInput>, BuildError> {
    let (start, end) = bounds(week_start);
    let (played, analysed): (i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM games WHERE account_id = $1 \
                   AND time_class = $2::time_class AND played_at >= $3 AND played_at < $4), \
                (SELECT count(*) FROM game_metrics WHERE account_id = $1 \
                   AND time_class = $2::time_class AND played_at >= $3 AND played_at < $4 \
                   AND metrics_version = $5)",
    )
    .bind(account_id)
    .bind(tc.as_str())
    .bind(start)
    .bind(end)
    .bind(i32::try_from(PLAYER_GAME_METRICS_VERSION).unwrap_or(i32::MAX))
    .fetch_one(pool)
    .await?;
    if played == 0 || analysed == 0 {
        return Ok(None);
    }
    let ci = build_coaching_input(pool, account_id, tc, now).await?;

    let week_key = iso_week_key(start);
    let rows = weekly_rows(pool, account_id, tc, &week_key).await?;
    let metric_deltas = metric_deltas(&rows, &week_key);

    let mut ids = HashSet::new();
    let focuses: Vec<_> = ci
        .weaknesses
        .iter()
        .filter(|w| ids.insert(w.id.clone()))
        .take(MAX_FOCUSES)
        .collect();
    let focus_updates = focuses
        .iter()
        .map(|w| FocusUpdate {
            weakness_id: w.id.clone(),
            label: w.label.clone(),
            p_imp: w.p_imp,
            state: w.state,
        })
        .collect();
    let mut games = HashSet::new();
    let exemplars = focuses
        .iter()
        .flat_map(|w| w.exemplars.iter())
        .filter(|e| e.played_at >= start && e.played_at < end)
        .filter(|e| games.insert(e.game_id.clone()))
        .take(MAX_EXEMPLARS)
        .cloned()
        .collect();
    let changes = changes_in_week(pool, account_id, tc, week_start, &ci).await?;

    Ok(Some(WeeklyInput {
        input_version: WEEKLY_INPUT_VERSION.to_owned(),
        time_class: tc.into(),
        band: ci.band,
        week_start,
        week_end: week_start + Duration::days(6),
        games_this_week: u32::try_from(played).unwrap_or(u32::MAX),
        basis: ci.basis.clone(),
        adherence: None,
        metric_deltas,
        focus_updates,
        exemplars,
        changes,
    }))
}

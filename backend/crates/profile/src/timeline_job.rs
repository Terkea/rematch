//! Timeline updates (R2 Task 33, TS M2 Task 25 step 4): [`evaluate_series`] per series, the
//! history rows that are new, and the `weakness.changed` events they give.
//!
//! - **Reference per series.** A weakness series uses its key's band statistics
//!   (`weakness:<key>`) with `n0(key)` and `min_evidence(key)`. A `motif.missed:<tag>` series
//!   uses `motif_miss_rate:<tag>` with the W3.shallow constants (a missed motif is a missed own
//!   tactic). `motif.allowed:*`, `opening:*` and `endgame:*` have no band statistic, so they are
//!   only ever `not_an_issue` (Task 14: no reference claims nothing). A statistic without a
//!   pooled rate is no reference.
//! - **Append-only dedupe.** A replayed row is new when no stored row of the series has the same
//!   `(kind, to_state, at_game_id)`. Only those columns are compared, never the `real`
//!   `p_change`/`rate_*` columns, so their precision cannot fake a change (T14 hand-off: no
//!   widening is needed while nothing compares them).
//! - **Cause hints** are computed for onset rows only: the up to [`CAUSE_BEFORE_GAMES`] series
//!   games before the onset against the games from the onset to the confirming game.
//! - **Events.** One `weakness.changed` per new `transition` row, except a series' first
//!   neutral row (`null -> not_an_issue`, which is no change: R-7 maps `null` to
//!   `not_an_issue`). `onsetAt` is the onset of a change confirmed at the same game, else null.

use std::collections::{BTreeMap, HashMap, HashSet};

use chess_core::TimeClass;
use coaching_input::CandidateCause;
use db::profile_rows::StateHistoryRow;
use profile_core::constants::{min_evidence, n0};
use profile_core::timeline::causes::{CauseGame, cause_hints};
use profile_core::timeline::series::{
    RowKind, SeriesOutcome, SeriesPoint, SeriesRef, TimelineRow, evaluate_series,
};
use profile_core::timeline::states::DETECTOR_VERSION;
use profile_core::types::{
    BandReference, GameContext, MetricId, SeriesKey, WeaknessKey, WeaknessState,
};
use serde_json::{Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

/// Series games before an onset that the cause hints compare against.
pub const CAUSE_BEFORE_GAMES: usize = 50;

/// How one series is evaluated.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SeriesSetup {
    pub reference: Option<SeriesRef>,
    pub n0: f64,
    pub min_evidence: f64,
}

/// The reference and constants of a series (module docs).
pub fn series_setup(key: &SeriesKey, reference: Option<&BandReference>) -> SeriesSetup {
    let (metric, constants_of) = match key {
        SeriesKey::Weakness(k) if k.is_binomial() => (Some(MetricId::Weakness(*k)), *k),
        SeriesKey::MotifMissed(t) => (Some(MetricId::MotifMissRate(*t)), WeaknessKey::W3Shallow),
        _ => (None, WeaknessKey::W1),
    };
    let series_ref = metric
        .and_then(|m| reference.and_then(|r| r.get(&m)))
        .and_then(|s| {
            Some(SeriesRef {
                median: s.median,
                p40: s.p40,
                pooled_rate: s.pooled_rate?,
            })
        });
    match series_ref {
        Some(r) => SeriesSetup {
            reference: Some(r),
            n0: n0(constants_of),
            min_evidence: min_evidence(constants_of),
        },
        None => SeriesSetup {
            reference: None,
            n0: 0.0,
            min_evidence: f64::INFINITY,
        },
    }
}

/// [`evaluate_series`] for every series.
pub fn evaluate_all(
    series: &BTreeMap<SeriesKey, Vec<SeriesPoint>>,
    reference: Option<&BandReference>,
) -> BTreeMap<SeriesKey, SeriesOutcome> {
    series
        .iter()
        .map(|(key, points)| {
            let s = series_setup(key, reference);
            (
                key.clone(),
                evaluate_series(points, s.reference.as_ref(), s.n0, s.min_evidence),
            )
        })
        .collect()
}

/// `(series_key, kind, to_state, at_game_id)` of a history row.
pub type RowKey = (String, String, String, Option<Uuid>);

/// The dedupe keys of every stored history row of the account × time class.
pub async fn stored_row_keys(
    conn: &mut PgConnection,
    account_id: Uuid,
    tc: TimeClass,
) -> Result<HashSet<RowKey>, sqlx::Error> {
    let rows: Vec<RowKey> = sqlx::query_as(
        "SELECT series_key, kind::text, to_state::text, at_game_id FROM weakness_state_history \
         WHERE account_id = $1 AND time_class = $2::time_class",
    )
    .bind(account_id)
    .bind(tc.as_str())
    .fetch_all(conn)
    .await?;
    Ok(rows.into_iter().collect())
}

fn row_key(key: &SeriesKey, row: &TimelineRow) -> RowKey {
    (
        key.to_string(),
        row.kind.as_str().to_owned(),
        row.to.as_str().to_owned(),
        Some(row.at_game_id),
    )
}

/// A replayed row that is not stored yet.
#[derive(Clone, Debug, PartialEq)]
pub struct NewRow {
    pub key: SeriesKey,
    pub row: TimelineRow,
    pub causes: Vec<CandidateCause>,
    /// The onset of a change confirmed at the same game (for the event).
    pub onset_at: Option<time::OffsetDateTime>,
}

/// Per-game facts the cause hints compare.
pub struct CauseFacts<'a> {
    pub games: &'a HashMap<Uuid, &'a GameContext>,
    pub line_keys: &'a HashMap<Uuid, String>,
}

fn cause_game(facts: &CauseFacts<'_>, p: &SeriesPoint) -> Option<CauseGame> {
    let ctx = facts.games.get(&p.game_id)?;
    let tc = match (ctx.base_seconds, ctx.increment_seconds) {
        (Some(b), inc) => format!("{b}+{}", inc.unwrap_or(0)),
        (None, _) => "daily".to_owned(),
    };
    Some(CauseGame {
        tc,
        opponent_rating: ctx.opponent_rating,
        line_key: facts.line_keys.get(&p.game_id).cloned(),
        played_at: p.played_at,
        protocol: p.protocol.clone(),
    })
}

fn onset_causes(
    points: &[SeriesPoint],
    row: &TimelineRow,
    facts: &CauseFacts<'_>,
) -> Vec<CandidateCause> {
    let Some(onset) = &row.onset else {
        return Vec::new();
    };
    // The replay order of `evaluate_series`.
    let mut sorted: Vec<&SeriesPoint> = points
        .iter()
        .filter(|p| p.events <= p.opportunities)
        .collect();
    sorted.sort_by_key(|p| (p.played_at, p.game_id, p.events, p.opportunities));
    let (Some(at_onset), Some(at_row)) = (
        sorted.iter().position(|p| p.game_id == onset.game_id),
        sorted.iter().position(|p| p.game_id == row.at_game_id),
    ) else {
        return Vec::new();
    };
    let before: Vec<CauseGame> = sorted[at_onset.saturating_sub(CAUSE_BEFORE_GAMES)..at_onset]
        .iter()
        .filter_map(|p| cause_game(facts, p))
        .collect();
    let after: Vec<CauseGame> = sorted[at_onset..=at_row.max(at_onset)]
        .iter()
        .filter_map(|p| cause_game(facts, p))
        .collect();
    cause_hints(&before, &after)
}

/// The replayed rows not stored yet, in series then replay order.
pub fn new_rows(
    series: &BTreeMap<SeriesKey, Vec<SeriesPoint>>,
    outcomes: &BTreeMap<SeriesKey, SeriesOutcome>,
    stored: &HashSet<RowKey>,
    facts: &CauseFacts<'_>,
) -> Vec<NewRow> {
    let mut out = Vec::new();
    for (key, outcome) in outcomes {
        let mut seen: HashSet<RowKey> = HashSet::new();
        for row in &outcome.rows {
            let k = row_key(key, row);
            if stored.contains(&k) || !seen.insert(k) {
                continue;
            }
            let onset_at = outcome
                .rows
                .iter()
                .find(|r| r.kind == RowKind::Onset && r.at_game_id == row.at_game_id)
                .and_then(|r| r.onset.as_ref().map(|o| o.played_at));
            let causes = match row.kind {
                RowKind::Onset => series
                    .get(key)
                    .map(|pts| onset_causes(pts, row, facts))
                    .unwrap_or_default(),
                RowKind::Transition => Vec::new(),
            };
            out.push(NewRow {
                key: key.clone(),
                row: row.clone(),
                causes,
                onset_at,
            });
        }
    }
    out
}

/// The `weakness_state_history` row of a new row. `protocol` is the analysis protocol of the
/// game the row is dated at.
pub fn history_row(account_id: Uuid, tc: TimeClass, r: &NewRow, protocol: &str) -> StateHistoryRow {
    let row = &r.row;
    StateHistoryRow {
        account_id,
        time_class: tc.as_str().to_owned(),
        series_key: r.key.to_string(),
        kind: row.kind.as_str().to_owned(),
        from_state: row.from.map(|s| s.as_str().to_owned()),
        to_state: row.to.as_str().to_owned(),
        at_game_id: Some(row.at_game_id),
        at_played_at: Some(row.at_played_at),
        onset_game_id: row.onset.as_ref().map(|o| o.game_id),
        onset_at: row.onset.as_ref().map(|o| o.played_at),
        onset_low: row.onset.as_ref().map(|o| o.low),
        onset_high: row.onset.as_ref().map(|o| o.high),
        p_change: row.p_change,
        rate_before: row.rate_before,
        rate_after: row.rate_after,
        evidence: serde_json::to_value(row.evidence).unwrap_or(Value::Null),
        cause_hints: serde_json::to_value(&r.causes).unwrap_or_else(|_| json!([])),
        detector_version: DETECTOR_VERSION.to_owned(),
        protocol: protocol.to_owned(),
    }
}

/// The `weakness.changed` payload of a new row, or `None` when the row is no change (module
/// docs).
pub fn weakness_changed_payload(tc: TimeClass, r: &NewRow) -> Option<Value> {
    let row = &r.row;
    if row.kind != RowKind::Transition
        || (row.from.is_none() && row.to == WeaknessState::NotAnIssue)
    {
        return None;
    }
    Some(json!({
        "id": r.key.to_string(),
        "timeClass": tc.as_str(),
        "state": row.to.as_str(),
        "fromState": row.from.unwrap_or(WeaknessState::NotAnIssue).as_str(),
        "onsetAt": r.onset_at.map(common::js_time::to_iso_string),
    }))
}

//! Timeline updates (R2 Task 33, TS M2 Task 25 step 4): [`evaluate_series`] per series, the
//! stored history made equal to the current replay, and the `weakness.changed` events.
//!
//! - **Reference per series.** A weakness series uses its key's band statistics
//!   (`weakness:<key>`) with `n0(key)` and `min_evidence(key)`. A `motif.missed:<tag>` series
//!   uses `motif_miss_rate:<tag>` with the W3.shallow constants (a missed motif is a missed own
//!   tactic). `motif.allowed:*`, `opening:*` and `endgame:*` have no band statistic, so they are
//!   only ever `not_an_issue` (Task 14: no reference claims nothing). A statistic without a
//!   pooled rate is no reference.
//! - **History is the current replay** (T33 review I1, ruling a). Each build replays every
//!   series from its first game, so a game analysed later but played earlier (the scheduler's
//!   P2 backfill, a re-analysis) moves the replay's rows to other games. Appending the moved rows
//!   next to the old ones would leave two "first" rows and contradicting changes. So, in the
//!   build's transaction and for the current [`DETECTOR_VERSION`] only, stored rows that are not
//!   in the replay are deleted and replayed rows that are not stored are inserted. Rows written
//!   by other detector versions are left alone.
//! - **Row identity** is `(series_key, kind, from_state, to_state, at_game_id, onset_game_id)`:
//!   text and uuid columns only, never the `real` `p_change`/`rate_*` columns, so their
//!   precision cannot fake a change (the T14 widening hand-off does not apply). `evaluate_series`
//!   emits at most one row per kind and game, so a replay has no duplicate identities.
//! - **Cause hints** are computed for onset rows only: the up to [`CAUSE_BEFORE_GAMES`] series
//!   games before the onset against the games from the onset to the confirming game.
//! - **Events.** One `weakness.changed` per series whose current state differs from its stored
//!   current state, read under the build's lock: `weakness_estimates.state` for weakness keys,
//!   the latest stored transition's `to_state` for other series, `not_an_issue` when nothing is
//!   stored (R-7). A replay that only moves rows publishes nothing. `onsetAt` is the onset of the
//!   latest onset row that led into the current state within its current episode (at or after
//!   the transition that entered it; T33 N3), else null.

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
use time::OffsetDateTime;
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

/// `(series_key, kind, from_state, to_state, at_game_id, onset_game_id)` of a history row.
pub type RowKey = (
    String,
    String,
    Option<String>,
    String,
    Option<Uuid>,
    Option<Uuid>,
);

/// The stored history of the current detector version: row ids by identity, and the latest
/// stored transition state per series.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StoredHistory {
    pub rows: HashMap<RowKey, Vec<i64>>,
    /// Latest transition state per series, current detector version.
    pub latest_state: HashMap<String, WeaknessState>,
    /// Latest transition state per series over every detector version (the fallback for a
    /// replayed series that has no current-version row yet, as after a version bump).
    pub latest_state_any: HashMap<String, WeaknessState>,
}

/// Reads [`StoredHistory`] (run it under the build's lock).
pub async fn stored_history(
    conn: &mut PgConnection,
    account_id: Uuid,
    tc: TimeClass,
) -> Result<StoredHistory, sqlx::Error> {
    type Raw = (
        i64,
        String,
        String,
        Option<String>,
        String,
        Option<Uuid>,
        Option<Uuid>,
    );
    let raw: Vec<Raw> = sqlx::query_as(
        "SELECT id, series_key, kind::text, from_state::text, to_state::text, at_game_id, \
           onset_game_id \
         FROM weakness_state_history \
         WHERE account_id = $1 AND time_class = $2::time_class AND detector_version = $3 \
         ORDER BY series_key, at_played_at NULLS FIRST, id",
    )
    .bind(account_id)
    .bind(tc.as_str())
    .bind(DETECTOR_VERSION)
    .fetch_all(&mut *conn)
    .await?;
    let mut out = StoredHistory::default();
    for (id, key, kind, from, to, at, onset) in raw {
        if kind == RowKind::Transition.as_str()
            && let Ok(state) = to.parse::<WeaknessState>()
        {
            // Rows come oldest first, so the last insert is the latest transition.
            out.latest_state.insert(key.clone(), state);
        }
        out.rows
            .entry((key, kind, from, to, at, onset))
            .or_default()
            .push(id);
    }
    let any: Vec<(String, String)> = sqlx::query_as(
        "SELECT DISTINCT ON (series_key) series_key, to_state::text \
         FROM weakness_state_history \
         WHERE account_id = $1 AND time_class = $2::time_class AND kind = 'transition' \
         ORDER BY series_key, at_played_at DESC NULLS LAST, id DESC",
    )
    .bind(account_id)
    .bind(tc.as_str())
    .fetch_all(&mut *conn)
    .await?;
    out.latest_state_any = any
        .into_iter()
        .filter_map(|(k, s)| s.parse().ok().map(|s| (k, s)))
        .collect();
    Ok(out)
}

fn row_key(key: &SeriesKey, row: &TimelineRow) -> RowKey {
    (
        key.to_string(),
        row.kind.as_str().to_owned(),
        row.from.map(|s| s.as_str().to_owned()),
        row.to.as_str().to_owned(),
        Some(row.at_game_id),
        row.onset.as_ref().map(|o| o.game_id),
    )
}

/// A replayed row that is not stored yet.
#[derive(Clone, Debug, PartialEq)]
pub struct NewRow {
    pub key: SeriesKey,
    pub row: TimelineRow,
    pub causes: Vec<CandidateCause>,
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

/// The replayed rows that are not stored, in series then replay order.
pub fn new_rows(
    series: &BTreeMap<SeriesKey, Vec<SeriesPoint>>,
    outcomes: &BTreeMap<SeriesKey, SeriesOutcome>,
    stored: &StoredHistory,
    facts: &CauseFacts<'_>,
) -> Vec<NewRow> {
    let mut out = Vec::new();
    for (key, outcome) in outcomes {
        for row in &outcome.rows {
            if stored.rows.contains_key(&row_key(key, row)) {
                continue;
            }
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
            });
        }
    }
    out
}

/// Ids of stored rows (current detector version) that are not in the replay, plus extra
/// copies of a replayed row, oldest copy kept.
pub fn stale_row_ids(
    outcomes: &BTreeMap<SeriesKey, SeriesOutcome>,
    stored: &StoredHistory,
) -> Vec<i64> {
    let replay: HashSet<RowKey> = outcomes
        .iter()
        .flat_map(|(key, o)| o.rows.iter().map(move |r| row_key(key, r)))
        .collect();
    let mut ids: Vec<i64> = stored
        .rows
        .iter()
        .flat_map(|(k, ids)| {
            let keep = usize::from(replay.contains(k));
            let mut sorted = ids.clone();
            sorted.sort_unstable();
            sorted.into_iter().skip(keep)
        })
        .collect();
    ids.sort_unstable();
    ids
}

/// Deletes history rows by id (in the build's transaction).
///
/// The statement carries its own scope (account, time class, current detector version), so a
/// wrong id can never delete another account's, time class's or detector's history.
pub async fn delete_history_rows(
    conn: &mut PgConnection,
    account_id: Uuid,
    tc: TimeClass,
    ids: &[i64],
) -> sqlx::Result<u64> {
    if ids.is_empty() {
        return Ok(0);
    }
    let done = sqlx::query(
        "DELETE FROM weakness_state_history WHERE id = ANY($1) AND account_id = $2 \
         AND time_class = $3::time_class AND detector_version = $4",
    )
    .bind(ids)
    .bind(account_id)
    .bind(tc.as_str())
    .bind(DETECTOR_VERSION)
    .execute(conn)
    .await?;
    Ok(done.rows_affected())
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

/// A series whose current state changed in this build.
#[derive(Clone, Debug, PartialEq)]
pub struct StateChange {
    pub key: SeriesKey,
    pub from: WeaknessState,
    pub to: WeaknessState,
    pub onset_at: Option<OffsetDateTime>,
}

/// The current state of every series that changed against `stored_state` (module docs). Every
/// binomial weakness key is considered (a key without points is `not_an_issue`), plus every
/// replayed series.
pub fn state_changes(
    outcomes: &BTreeMap<SeriesKey, SeriesOutcome>,
    stored_estimates: &HashMap<String, WeaknessState>,
    stored: &StoredHistory,
) -> Vec<StateChange> {
    let mut keys: Vec<SeriesKey> = WeaknessKey::ALL
        .into_iter()
        .filter(|k| k.is_binomial())
        .map(SeriesKey::Weakness)
        .collect();
    keys.extend(outcomes.keys().cloned());
    // Series with current-version rows that left the replay: they are `not_an_issue` now.
    keys.extend(
        stored
            .latest_state
            .keys()
            .filter_map(|k| k.parse::<SeriesKey>().ok()),
    );
    keys.sort();
    keys.dedup();
    keys.into_iter()
        .filter_map(|key| {
            let name = key.to_string();
            let outcome = outcomes.get(&key);
            let to = outcome.map_or(WeaknessState::NotAnIssue, |o| o.state);
            let from = match key {
                SeriesKey::Weakness(_) => stored_estimates.get(&name),
                // A replayed series without current-version rows (a detector bump) falls back
                // to the latest row of any version, so an unchanged state is not re-announced.
                // A series outside the replay counts only with current-version rows, or it
                // would be announced again on every build.
                _ => stored.latest_state.get(&name).or_else(|| {
                    outcome
                        .is_some()
                        .then(|| stored.latest_state_any.get(&name))
                        .flatten()
                }),
            }
            .copied()
            .unwrap_or(WeaknessState::NotAnIssue);
            (from != to).then(|| StateChange {
                onset_at: outcome.and_then(|o| current_episode_onset(o, to)),
                key,
                from,
                to,
            })
        })
        .collect()
}

/// The onset of the current episode of `to` (T33 N3, fixed in T34): the latest onset row into
/// `to` dated at or after the transition that entered the current state. An episode entered
/// without its own onset row has none: an earlier episode's onset is never borrowed.
fn current_episode_onset(o: &SeriesOutcome, to: WeaknessState) -> Option<OffsetDateTime> {
    let entered = o
        .rows
        .iter()
        .rfind(|r| r.kind == RowKind::Transition)
        .filter(|r| r.to == to)
        .map(|r| r.at_played_at);
    o.rows
        .iter()
        .filter(|r| r.kind == RowKind::Onset && r.to == to)
        .filter(|r| entered.is_none_or(|at| r.at_played_at >= at))
        .filter_map(|r| r.onset.as_ref().map(|on| (r.at_played_at, on.played_at)))
        .max()
        .map(|(_, onset)| onset)
}

/// The stored `weakness_estimates.state` per series key (run it under the build's lock).
pub async fn stored_estimate_states(
    conn: &mut PgConnection,
    account_id: Uuid,
    tc: TimeClass,
) -> Result<HashMap<String, WeaknessState>, sqlx::Error> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT series_key, state::text FROM weakness_estimates \
         WHERE account_id = $1 AND time_class = $2::time_class",
    )
    .bind(account_id)
    .bind(tc.as_str())
    .fetch_all(conn)
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(|(k, s)| s.parse().ok().map(|s| (k, s)))
        .collect())
}

/// The `weakness.changed` payload of a state change.
pub fn weakness_changed_payload(tc: TimeClass, c: &StateChange) -> Value {
    json!({
        "id": c.key.to_string(),
        "timeClass": tc.as_str(),
        "state": c.to.as_str(),
        "fromState": c.from.as_str(),
        "onsetAt": c.onset_at.map(common::js_time::to_iso_string),
    })
}

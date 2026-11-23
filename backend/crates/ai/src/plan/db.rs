//! Loads one user's plan context (ADR 0094) from `focuses` (ADR 0091), `drill_items` and
//! `drill_attempts` (ADR 0090), and the account's analysed games. Every row is found by
//! `user_id`, and the user must be linked to the account: another user's focus or training is
//! never read, even on a shared account.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde_json::Value;
use sqlx::PgConnection;
use time::{Duration, OffsetDateTime, Time};
use uuid::Uuid;

use super::compute::{
    GameCount, MotifGame, SOURCES, before_after, focus_progress, pattern_found, source_label,
    verdict_of, weakness_label,
};
use super::types::{
    PatternEfficacy, PlanContext, PlanFocus, PlanSide, PlanTraining, SourceEfficacy,
    SourceTraining, TrainingWindow,
};
use crate::input::types::TimeClass;

fn day(t: OffsetDateTime) -> String {
    let d = t.to_offset(time::UtcOffset::UTC).date();
    format!("{:04}-{:02}-{:02}", d.year(), u8::from(d.month()), d.day())
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

fn nonneg(n: i64) -> u32 {
    u32::try_from(n.max(0)).unwrap_or(u32::MAX)
}

/// The account's verified owner (`account_links_one_verified_owner`): the one reader of the
/// account's AI artefacts (ADR-FIX-N-1), whose plan an account-level text may use.
pub async fn verified_owner(
    conn: &mut PgConnection,
    account_id: Uuid,
) -> sqlx::Result<Option<Uuid>> {
    sqlx::query_scalar(
        "SELECT user_id FROM account_links WHERE linked_account_id = $1 \
         AND verified_at IS NOT NULL ORDER BY verified_at LIMIT 1",
    )
    .bind(account_id)
    .fetch_optional(conn)
    .await
}

struct Item {
    id: Uuid,
    source_key: String,
    source_keys: Vec<String>,
    held_out: bool,
    retired_at: Option<OffsetDateTime>,
    tags: Vec<String>,
}

struct Attempt {
    item: Uuid,
    at: OffsetDateTime,
    correct: bool,
}

/// `user_id`'s plan for `account_id` and `tc` as of `now`; `None` when the user is not linked
/// to the account, or has neither a running focus there nor a trainer position.
/// `label_of` names a series the catalogue does not (a motif or opening line).
pub async fn load_plan(
    conn: &mut PgConnection,
    user_id: Uuid,
    account_id: Uuid,
    tc: TimeClass,
    now: OffsetDateTime,
    label_of: &(dyn Fn(&str) -> Option<String> + Sync),
) -> sqlx::Result<Option<PlanContext>> {
    let linked: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM account_links WHERE user_id = $1 AND linked_account_id = $2)",
    )
    .bind(user_id)
    .bind(account_id)
    .fetch_one(&mut *conn)
    .await?;
    if !linked {
        return Ok(None);
    }
    let focus = load_focus(conn, user_id, account_id, tc, label_of).await?;
    let items: Vec<Item> = sqlx::query_as::<
        _,
        (
            Uuid,
            String,
            Vec<String>,
            bool,
            Option<OffsetDateTime>,
            Vec<String>,
        ),
    >(
        "SELECT id, source_key, source_keys, held_out, retired_at, tags FROM drill_items \
         WHERE user_id = $1 AND account_id = $2 AND time_class = $3::time_class",
    )
    .bind(user_id)
    .bind(account_id)
    .bind(tc.as_str())
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .map(
        |(id, source_key, source_keys, held_out, retired_at, tags)| Item {
            id,
            source_key,
            source_keys,
            held_out,
            retired_at,
            tags,
        },
    )
    .collect();
    if focus.is_none() && items.is_empty() {
        return Ok(None);
    }
    let attempts: Vec<Attempt> = sqlx::query_as::<_, (Uuid, OffsetDateTime, bool)>(
        "SELECT a.item_id, a.at, a.correct FROM drill_attempts a \
         JOIN drill_items i ON i.id = a.item_id \
         WHERE a.user_id = $1 AND i.user_id = $1 AND i.account_id = $2 \
           AND i.time_class = $3::time_class",
    )
    .bind(user_id)
    .bind(account_id)
    .bind(tc.as_str())
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .map(|(item, at, correct)| Attempt { item, at, correct })
    .collect();

    let training = PlanTraining {
        last_week: window(&items, &attempts, now, 7),
        last_month: window(&items, &attempts, now, 30),
        active_items: count(
            items
                .iter()
                .filter(|i| !i.held_out && i.retired_at.is_none())
                .count(),
        ),
    };
    let (is_it_working, patterns) = if items.is_empty() {
        (Vec::new(), None)
    } else {
        efficacy(conn, account_id, tc, &items, &attempts).await?
    };
    Ok(Some(PlanContext {
        as_of: day(now),
        focus,
        training,
        is_it_working,
        patterns,
    }))
}

async fn load_focus(
    conn: &mut PgConnection,
    user_id: Uuid,
    account_id: Uuid,
    tc: TimeClass,
    label_of: &(dyn Fn(&str) -> Option<String> + Sync),
) -> sqlx::Result<Option<PlanFocus>> {
    type Row = (String, String, f64, i32, OffsetDateTime, f64, i32, i32);
    let row: Option<Row> = sqlx::query_as(
        "SELECT series_key, side::text, target_rate, games_n, started_at, baseline_rate, \
           baseline_k, baseline_n FROM focuses \
         WHERE user_id = $1 AND account_id = $2 AND time_class = $3::time_class \
           AND ended_at IS NULL LIMIT 1",
    )
    .bind(user_id)
    .bind(account_id)
    .bind(tc.as_str())
    .fetch_optional(&mut *conn)
    .await?;
    let Some((series_key, side, target, games_n, started_at, b_rate, b_k, b_n)) = row else {
        return Ok(None);
    };
    // Web `focusCounts`: the first N analysed games after the start, of the focus's colour.
    let (games, events, chances): (i64, i64, i64) = sqlx::query_as(
        "WITH in_focus AS ( \
           SELECT gm.game_id FROM game_metrics gm JOIN games g ON g.id = gm.game_id \
           WHERE gm.account_id = $1 AND gm.time_class = $2::time_class AND gm.played_at > $3 \
             AND ($4 = 'both' OR g.player_color::text = $4) \
           ORDER BY gm.played_at, gm.game_id LIMIT $5) \
         SELECT count(*), coalesce(sum(w.events), 0)::bigint, \
           coalesce(sum(w.opportunities), 0)::bigint \
         FROM in_focus f LEFT JOIN weakness_series_point w ON w.game_id = f.game_id \
           AND w.account_id = $1 AND w.time_class = $2::time_class AND w.series_key = $6",
    )
    .bind(account_id)
    .bind(tc.as_str())
    .bind(started_at)
    .bind(&side)
    .bind(i64::from(games_n))
    .bind(&series_key)
    .fetch_one(&mut *conn)
    .await?;
    let games_n = nonneg(i64::from(games_n));
    let p = focus_progress(
        nonneg(events),
        nonneg(chances),
        nonneg(games),
        games_n,
        target,
    );
    let label = weakness_label(&series_key)
        .map(str::to_owned)
        .or_else(|| label_of(&series_key))
        .unwrap_or_else(|| series_key.clone());
    Ok(Some(PlanFocus {
        label,
        side: match side.as_str() {
            "white" => PlanSide::White,
            "black" => PlanSide::Black,
            _ => PlanSide::Both,
        },
        started_on: day(started_at),
        target_rate: target,
        games_n,
        games_played: p.games_played,
        progress: p.rate,
        status: p.status,
        more_chances: p.more_chances,
        outcome: p.outcome,
        baseline_rate: b_rate,
        baseline_events: nonneg(i64::from(b_k)),
        baseline_chances: nonneg(i64::from(b_n)),
        series_key,
    }))
}

/// The attempts of the last `days` UTC days, today included.
fn window(items: &[Item], attempts: &[Attempt], now: OffsetDateTime, days: u32) -> TrainingWindow {
    let today = now.to_offset(time::UtcOffset::UTC).date();
    let from = (today - Duration::days(i64::from(days) - 1))
        .with_time(Time::MIDNIGHT)
        .assume_utc();
    let source: HashMap<Uuid, &str> = items
        .iter()
        .map(|i| (i.id, i.source_key.as_str()))
        .collect();
    let mut days_seen = BTreeSet::new();
    let mut by: BTreeMap<&str, (u32, u32)> = BTreeMap::new();
    let (mut n, mut right) = (0u32, 0u32);
    for a in attempts.iter().filter(|a| a.at >= from && a.at <= now) {
        days_seen.insert(a.at.to_offset(time::UtcOffset::UTC).date());
        n += 1;
        right += u32::from(a.correct);
        let e = by
            .entry(source.get(&a.item).copied().unwrap_or("blunder"))
            .or_default();
        e.0 += 1;
        e.1 += u32::from(a.correct);
    }
    let rate = |r: u32, n: u32| (n > 0).then(|| f64::from(r) / f64::from(n));
    TrainingWindow {
        days,
        days_practised: count(days_seen.len()),
        attempts: n,
        right,
        first_try_rate: rate(right, n),
        items_retired: count(
            items
                .iter()
                .filter(|i| i.retired_at.is_some_and(|t| t >= from && t <= now))
                .count(),
        ),
        by_source: SOURCES
            .iter()
            .filter_map(|s| {
                by.get(s).map(|(n, r)| SourceTraining {
                    source: (*s).to_owned(),
                    label: source_label(s),
                    attempts: *n,
                    right: *r,
                    first_try_rate: rate(*r, *n),
                })
            })
            .collect(),
    }
}

fn num(v: Option<&Value>) -> Option<f64> {
    v.and_then(Value::as_f64).filter(|x| x.is_finite())
}

fn as_count(x: f64) -> u32 {
    x.round().clamp(0.0, f64::from(u32::MAX)) as u32
}

/// "Is it working?" per source and drilled against held back (web `loadTrainPage`'s efficacy).
async fn efficacy(
    conn: &mut PgConnection,
    account_id: Uuid,
    tc: TimeClass,
    items: &[Item],
    attempts: &[Attempt],
) -> sqlx::Result<(Vec<SourceEfficacy>, Option<PatternEfficacy>)> {
    let mut first: HashMap<Uuid, OffsetDateTime> = HashMap::new();
    for a in attempts {
        let f = first.entry(a.item).or_insert(a.at);
        if a.at < *f {
            *f = a.at;
        }
    }
    let rows: Vec<(OffsetDateTime, Option<Value>, Option<Value>)> = sqlx::query_as(
        "SELECT played_at, metrics -> 'weakness', metrics -> 'missedByMotif' FROM game_metrics \
         WHERE account_id = $1 AND time_class = $2::time_class ORDER BY played_at",
    )
    .bind(account_id)
    .bind(tc.as_str())
    .fetch_all(&mut *conn)
    .await?;
    let wants_blunders = items
        .iter()
        .any(|i| i.source_keys.iter().any(|s| s == "blunder"));
    let blunders: Vec<GameCount> = if wants_blunders {
        sqlx::query_as::<_, (OffsetDateTime, i64, i64)>(
            "SELECT g.played_at, count(*) FILTER (WHERE m.judgement = 'blunder'), count(*) \
             FROM games g JOIN moves m ON m.game_id = g.id AND m.color = g.player_color \
             WHERE g.account_id = $1 AND g.time_class = $2::time_class \
             GROUP BY g.id, g.played_at",
        )
        .bind(account_id)
        .bind(tc.as_str())
        .fetch_all(&mut *conn)
        .await?
        .into_iter()
        .map(|(played_at, e, n)| GameCount {
            played_at,
            events: nonneg(e),
            chances: nonneg(n),
        })
        .collect()
    } else {
        Vec::new()
    };
    let mut out = Vec::new();
    for source in SOURCES {
        let own: Vec<&Item> = items
            .iter()
            .filter(|i| i.source_keys.iter().any(|s| s == source))
            .collect();
        if own.is_empty() {
            continue;
        }
        let firsts: Vec<OffsetDateTime> = own
            .iter()
            .filter_map(|i| first.get(&i.id).copied())
            .collect();
        let first_drill = firsts.iter().min().copied();
        let per_game: Vec<GameCount> = if source == "blunder" {
            blunders.clone()
        } else {
            rows.iter()
                .filter_map(|(played_at, w, _)| {
                    let w = w.as_ref()?.get(source)?;
                    let events = num(w.get("events"))?;
                    let opp = num(w.get("opportunities"))?;
                    (opp > 0.0).then(|| GameCount {
                        played_at: *played_at,
                        events: as_count(events),
                        chances: as_count(opp),
                    })
                })
                .collect()
        };
        let (before, after, verdict) = before_after(&per_game, first_drill);
        out.push(SourceEfficacy {
            source: source.to_owned(),
            label: source_label(source),
            drilled: count(firsts.len()),
            first_drill: first_drill.map(day),
            before,
            after,
            verdict,
        });
    }
    let since = first.values().min().copied();
    let drilled_tags: BTreeSet<String> = items
        .iter()
        .filter(|i| first.contains_key(&i.id))
        .flat_map(|i| i.tags.iter().cloned())
        .collect();
    let held_tags: BTreeSet<String> = items
        .iter()
        .filter(|i| i.held_out)
        .flat_map(|i| {
            i.tags
                .iter()
                .filter(|t| !drilled_tags.contains(*t))
                .cloned()
        })
        .collect();
    let games: Vec<MotifGame> = rows
        .iter()
        .map(|(played_at, _, m)| MotifGame {
            played_at: *played_at,
            by_motif: m
                .as_ref()
                .and_then(Value::as_object)
                .map(|o| {
                    o.iter()
                        .filter_map(|(k, v)| {
                            Some((
                                k.clone(),
                                (num(v.get("opportunities"))?, num(v.get("misses"))?),
                            ))
                        })
                        .collect()
                })
                .unwrap_or_default(),
        })
        .collect();
    let far = OffsetDateTime::new_utc(
        time::Date::from_calendar_date(9999, time::Month::December, 31).expect("a date"),
        Time::MIDNIGHT,
    );
    let drilled = pattern_found(&games, &drilled_tags, since.unwrap_or(far));
    let held_out = pattern_found(&games, &held_tags, since.unwrap_or(far));
    Ok((
        out,
        Some(PatternEfficacy {
            since: since.map(day),
            verdict: verdict_of(&held_out, &drilled),
            drilled,
            held_out,
        }),
    ))
}

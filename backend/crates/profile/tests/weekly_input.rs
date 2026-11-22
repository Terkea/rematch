//! M5b Task B (ruling G2): the weekly-review input (TS plan B4 `WeeklyReviewInput`) built from
//! R2 data, on the Task 33 seed with real moves. The seed's 40 blitz games are split over
//! 2026-W46 (from Mon 9 Nov) and 2026-W47 (from Mon 16 Nov).

mod common {
    pub mod moves;
    pub mod seed;
}

use std::collections::HashSet;
use std::time::Duration;

use chess_core::TimeClass;
use coaching_input::{BandId, CoachingInput, WeaknessState};
use common::moves::{EXEMPLAR_GAMES, seed_with_moves};
use common::seed::{SEED_NOW, SeedOptions, seed_now};
use profile::build_profile::{BuildProfileDeps, handle_build_profile};
use profile::coaching::build_coaching_input;
use profile::coaching::weekly::{
    DeltaVerdict, MetricPoint, WEEKLY_INPUT_VERSION, WeeklyInput, build_weekly_input,
    delta_verdict, weekly_due,
};
use queue::BuildProfilePayload;
use queue::queue::Queue;
use serde_json::{Value, json};
use sqlx::PgPool;
use time::macros::{date, datetime};
use time::{Date, OffsetDateTime};
use uuid::Uuid;

const TEST_TIMEOUT: Duration = Duration::from_secs(120);
/// 2026-W46 and 2026-W47.
const W46: Date = date!(2026 - 11 - 09);
const W47: Date = date!(2026 - 11 - 16);
/// The Monday 04:00 run after W47.
const AFTER_W47: OffsetDateTime = datetime!(2026-11-23 04:00 UTC);

async fn within<T>(what: &str, fut: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(TEST_TIMEOUT, fut)
        .await
        .unwrap_or_else(|_| panic!("timed out after {TEST_TIMEOUT:?} waiting for {what}"))
}

async fn run_build(pool: &PgPool, account_id: Uuid) {
    let deps = BuildProfileDeps {
        pool: pool.clone(),
        queue: Queue::new(pool.clone()),
        now: seed_now,
        benchmark: profile::peer_benchmark::BenchmarkSource::LichessBands,
    };
    within(
        "build-profile",
        handle_build_profile(
            &deps,
            BuildProfilePayload {
                account_id,
                time_class: "blitz".to_owned(),
            },
        ),
    )
    .await
    .expect("build-profile");
}

async fn built(pool: &PgPool) -> Uuid {
    let f = seed_with_moves(pool, SeedOptions::default()).await;
    run_build(pool, f.account_id).await;
    f.account_id
}

async fn weekly(pool: &PgPool, account_id: Uuid, week: Date) -> Option<WeeklyInput> {
    within(
        "build_weekly_input",
        build_weekly_input(pool, account_id, TimeClass::Blitz, week, AFTER_W47),
    )
    .await
    .unwrap_or_else(|e| panic!("build_weekly_input: {e:?}"))
}

async fn coaching(pool: &PgPool, account_id: Uuid) -> CoachingInput {
    within(
        "build_coaching_input",
        build_coaching_input(pool, account_id, TimeClass::Blitz, AFTER_W47),
    )
    .await
    .unwrap()
}

fn schema() -> Value {
    let path = format!(
        "{}/../../fixtures/ai/input/weekly-review-input.schema.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    serde_json::from_str(&text).unwrap()
}

fn errors(instance: &Value) -> Vec<String> {
    let v = jsonschema::options()
        .should_validate_formats(true)
        .build(&schema())
        .expect("schema compiles");
    v.iter_errors(instance)
        .map(|e| format!("{} at {}", e, e.instance_path()))
        .collect()
}

// ------------------------------------------------------------------ the contract

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn weekly_input_validates_against_the_committed_contract_schema(pool: PgPool) {
    let a = built(&pool).await;
    for week in [W46, W47] {
        let input = weekly(&pool, a, week).await.expect("a week with games");
        let v = serde_json::to_value(&input).unwrap();
        assert_eq!(errors(&v), Vec::<String>::new(), "{week}");
        // It reads back unchanged through the strict type.
        let back: WeeklyInput = serde_json::from_value(v.clone()).unwrap();
        assert_eq!(back, input);
    }
    // The validator is live: an extra key, a third exemplar and a missing field are rejected.
    let v = serde_json::to_value(weekly(&pool, a, W47).await.unwrap()).unwrap();
    let mut extra = v.clone();
    extra["focus"] = json!("W1");
    assert!(!errors(&extra).is_empty());
    let mut three = v.clone();
    let e = three["exemplars"][0].clone();
    three["exemplars"] = json!([e.clone(), e.clone(), e]);
    assert!(!errors(&three).is_empty());
    let mut missing = v;
    missing.as_object_mut().unwrap().remove("adherence");
    assert!(!errors(&missing).is_empty());
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn the_week_is_named_and_counted(pool: PgPool) {
    let a = built(&pool).await;
    let ci = coaching(&pool, a).await;
    let w = weekly(&pool, a, W47).await.unwrap();
    assert_eq!(w.input_version, WEEKLY_INPUT_VERSION);
    assert_eq!(WEEKLY_INPUT_VERSION, "weekly-input-v1");
    assert_eq!(w.time_class, coaching_input::TimeClass::Blitz);
    assert_eq!((w.week_start, w.week_end), (W47, date!(2026 - 11 - 22)));
    assert_eq!(w.band, BandId::B2);
    assert_eq!(w.band, ci.band);
    // Half of the 40 seed games are played in each week.
    assert_eq!(w.games_this_week, 20);
    // The evidence behind the estimates is the profile's.
    assert_eq!(w.basis, ci.basis);
    // No practice is tracked yet, so nothing was planned: null, never 0 of 0.
    assert_eq!(w.adherence, None);
    assert_eq!(serde_json::to_value(&w).unwrap()["adherence"], Value::Null);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_week_without_games_has_no_review(pool: PgPool) {
    let a = built(&pool).await;
    assert!(weekly(&pool, a, date!(2026 - 11 - 02)).await.is_none());
    assert!(weekly(&pool, a, date!(2026 - 11 - 23)).await.is_none());
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_week_whose_games_are_not_analysed_has_no_review(pool: PgPool) {
    let a = built(&pool).await;
    sqlx::query(
        "DELETE FROM game_metrics WHERE account_id = $1 AND played_at >= $2 AND played_at < $3",
    )
    .bind(a)
    .bind(datetime!(2026-11-16 00:00 UTC))
    .bind(datetime!(2026-11-23 00:00 UTC))
    .execute(&pool)
    .await
    .unwrap();
    assert!(weekly(&pool, a, W47).await.is_none());
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn metric_deltas_compare_the_week_with_the_week_before(pool: PgPool) {
    let a = built(&pool).await;
    let w = weekly(&pool, a, W47).await.unwrap();
    assert!(!w.metric_deltas.is_empty());
    let keys: HashSet<&str> = w.metric_deltas.iter().map(|d| d.key.as_str()).collect();
    assert_eq!(keys.len(), w.metric_deltas.len(), "one delta per metric");
    for d in &w.metric_deltas {
        let stored = |week: &str| {
            let pool = pool.clone();
            let key = d.key.clone();
            let week = week.to_owned();
            async move {
                let row: (Option<f64>, i32) = sqlx::query_as(
                    "SELECT value, n FROM kpi_snapshot WHERE account_id = $1 \
                     AND side = 'both' AND metric = $2 AND window_key = $3 \
                     ORDER BY id DESC LIMIT 1",
                )
                .bind(a)
                .bind(&key)
                .bind(&week)
                .fetch_one(&pool)
                .await
                .unwrap();
                row
            }
        };
        let (after, after_n) = stored("2026-W47").await;
        let (before, before_n) = stored("2026-W46").await;
        assert_eq!(Some(d.after.value), after, "{}", d.key);
        assert_eq!(Some(d.before.value), before, "{}", d.key);
        assert_eq!(i64::from(d.after.n), i64::from(after_n), "{}", d.key);
        assert_eq!(i64::from(d.before.n), i64::from(before_n), "{}", d.key);
        // The two seed weeks are identical, so nothing changed.
        assert_eq!(d.verdict, DeltaVerdict::NoClearChange, "{}", d.key);
        assert!(!d.label.is_empty());
    }
    // The first week has no week before it: no deltas, never a comparison with nothing.
    assert!(
        weekly(&pool, a, W46)
            .await
            .unwrap()
            .metric_deltas
            .is_empty()
    );
}

#[test]
fn a_verdict_needs_separate_intervals_in_a_known_direction() {
    let p = |value: f64, low: f64, high: f64| MetricPoint {
        value,
        n: 40,
        low: Some(low),
        high: Some(high),
    };
    let (lo, hi) = (p(0.2, 0.15, 0.25), p(0.4, 0.3, 0.5));
    assert_eq!(
        delta_verdict(&lo, &hi, Some(true), true),
        DeltaVerdict::Improved
    );
    assert_eq!(
        delta_verdict(&lo, &hi, Some(false), true),
        DeltaVerdict::Worse
    );
    assert_eq!(
        delta_verdict(&hi, &lo, Some(true), true),
        DeltaVerdict::Worse
    );
    assert_eq!(
        delta_verdict(&hi, &lo, Some(false), true),
        DeltaVerdict::Improved
    );
    // Overlapping intervals, no direction, too little evidence or no interval: no verdict.
    let overlap = p(0.3, 0.2, 0.35);
    assert_eq!(
        delta_verdict(&lo, &overlap, Some(true), true),
        DeltaVerdict::NoClearChange
    );
    assert_eq!(
        delta_verdict(&lo, &hi, None, true),
        DeltaVerdict::NoClearChange
    );
    assert_eq!(
        delta_verdict(&lo, &hi, Some(true), false),
        DeltaVerdict::NoClearChange
    );
    let bare = MetricPoint {
        value: 0.9,
        n: 40,
        low: None,
        high: None,
    };
    assert_eq!(
        delta_verdict(&lo, &bare, Some(true), true),
        DeltaVerdict::NoClearChange
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn focus_updates_are_the_top_ranked_weaknesses(pool: PgPool) {
    let a = built(&pool).await;
    let ci = coaching(&pool, a).await;
    let w = weekly(&pool, a, W47).await.unwrap();
    assert!(!w.focus_updates.is_empty(), "W1 is ranked");
    assert!(w.focus_updates.len() <= 3);
    let mut seen = HashSet::new();
    let ranked: Vec<_> = ci
        .weaknesses
        .iter()
        .filter(|r| seen.insert(r.id.clone()))
        .take(3)
        .collect();
    assert_eq!(w.focus_updates.len(), ranked.len());
    for (f, r) in w.focus_updates.iter().zip(ranked) {
        assert_eq!(
            (&f.weakness_id, &f.label, f.p_imp, f.state),
            (&r.id, &r.label, r.p_imp, r.state)
        );
    }
    assert_eq!(w.focus_updates[0].weakness_id, "W1");
    assert_eq!(w.focus_updates[0].state, WeaknessState::Active);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn exemplars_are_positions_from_the_week(pool: PgPool) {
    let a = built(&pool).await;
    let games: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT id, platform_game_id FROM games WHERE account_id = $1 ORDER BY played_at",
    )
    .bind(a)
    .fetch_all(&pool)
    .await
    .unwrap();
    let platform = |i: usize| games[i].1.clone();
    let w47 = weekly(&pool, a, W47).await.unwrap();
    let mut ids: Vec<String> = w47.exemplars.iter().map(|e| e.game_id.clone()).collect();
    ids.sort();
    // Seed games 30 and 35 carry W1 exemplar plies in W47; game 5 is in W46.
    let mut want = vec![platform(EXEMPLAR_GAMES[1]), platform(EXEMPLAR_GAMES[2])];
    want.sort();
    assert_eq!(ids, want);
    let w46 = weekly(&pool, a, W46).await.unwrap();
    let ids: Vec<String> = w46.exemplars.iter().map(|e| e.game_id.clone()).collect();
    assert_eq!(ids, vec![platform(EXEMPLAR_GAMES[0])]);
    for e in w47.exemplars.iter().chain(&w46.exemplars) {
        let d = e.played_at.date();
        assert!(d >= W46 && d < date!(2026 - 11 - 23), "{d}");
    }
}

/// A planted onset row of W1, detected (`at_played_at`) at `at_game`, with its onset at
/// `onset_game`.
async fn plant_onset(pool: &PgPool, account_id: Uuid, at_game: Uuid, onset_game: Uuid) {
    sqlx::query(
        "INSERT INTO weakness_state_history (account_id, time_class, series_key, kind, \
           from_state, to_state, at_game_id, at_played_at, onset_game_id, onset_at, onset_low, \
           onset_high, p_change, rate_before, rate_after, evidence, detector_version, protocol) \
         SELECT $1, 'blitz', 'W1', 'onset', 'not_an_issue', 'emerging', a.id, \
           a.played_at, o.id, o.played_at, o.played_at - interval '1 day', \
           o.played_at + interval '1 day', 0.9, 0.02, 0.05, '{}'::jsonb, \
           (SELECT detector_version FROM weakness_state_history LIMIT 1), 'p' \
         FROM games a, games o WHERE a.id = $2 AND o.id = $3",
    )
    .bind(account_id)
    .bind(at_game)
    .bind(onset_game)
    .bind(profile_core::timeline::states::DETECTOR_VERSION)
    .execute(pool)
    .await
    .unwrap();
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn changes_are_the_ones_detected_in_the_week(pool: PgPool) {
    let f = seed_with_moves(&pool, SeedOptions::default()).await;
    run_build(&pool, f.account_id).await;
    let a = f.account_id;
    sqlx::query("DELETE FROM weakness_state_history WHERE kind = 'onset'")
        .execute(&pool)
        .await
        .unwrap();
    // Detected in W46 (onset game 3, confirmed at game 10) and in W47 (onset game 12, which
    // is in W46, confirmed at game 25).
    plant_onset(&pool, a, f.games[10], f.games[3]).await;
    plant_onset(&pool, a, f.games[25], f.games[12]).await;
    let platform: Vec<String> = {
        let mut v = Vec::new();
        for g in &f.games {
            v.push(
                sqlx::query_scalar("SELECT platform_game_id FROM games WHERE id = $1")
                    .bind(g)
                    .fetch_one(&pool)
                    .await
                    .unwrap(),
            );
        }
        v
    };
    let ci = coaching(&pool, a).await;
    assert_eq!(ci.changes.len(), 2, "both onsets are recent changes");
    let w47 = weekly(&pool, a, W47).await.unwrap();
    let onsets: Vec<&str> = w47
        .changes
        .iter()
        .map(|c| c.onset_game_id.as_str())
        .collect();
    assert_eq!(onsets, vec![platform[12].as_str()]);
    assert!(ci.changes.contains(&w47.changes[0]));
    let w46 = weekly(&pool, a, W46).await.unwrap();
    let onsets: Vec<&str> = w46
        .changes
        .iter()
        .map(|c| c.onset_game_id.as_str())
        .collect();
    assert_eq!(onsets, vec![platform[3].as_str()]);
    let v = serde_json::to_value(&w47).unwrap();
    assert_eq!(errors(&v), Vec::<String>::new());
}

// ------------------------------------------------------------------ due accounts

async fn link(pool: &PgPool, account_id: Uuid) {
    let user = testkit::insert_user(pool, &format!("u-{account_id}")).await;
    sqlx::query(
        "INSERT INTO account_links (user_id, linked_account_id, method) VALUES ($1, $2, 'username')",
    )
    .bind(user)
    .bind(account_id)
    .execute(pool)
    .await
    .unwrap();
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn due_lists_linked_accounts_with_games_in_the_week(pool: PgPool) {
    let a = built(&pool).await;
    // Not linked to any user: nobody would read the review.
    assert!(weekly_due(&pool, W47).await.unwrap().is_empty());
    link(&pool, a).await;
    assert_eq!(
        weekly_due(&pool, W47).await.unwrap(),
        vec![(a, TimeClass::Blitz)]
    );
    assert_eq!(
        weekly_due(&pool, W46).await.unwrap(),
        vec![(a, TimeClass::Blitz)]
    );
    // No game that week.
    assert!(
        weekly_due(&pool, date!(2026 - 11 - 23))
            .await
            .unwrap()
            .is_empty()
    );
    let _ = SEED_NOW;
}

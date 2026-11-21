use ai::chat::anchor::ChatAnchor;
use ai::entitlements::{
    DefaultEntitlements, Entitlements, Plan, Quota, QuotaWindow, chat_quotas, engine_quotas,
    exhausted, window_reset, window_start,
};
use ai::output::codes::SectionKey;
use serde_json::json;
use time::macros::datetime;
use uuid::Uuid;

const fn q(limit: u32, window: QuotaWindow) -> Quota {
    Quota { limit, window }
}

#[test]
fn quota_tables_match_owner_input() {
    assert_eq!(chat_quotas(Plan::Free), &[q(3, QuotaWindow::Week)]);
    assert_eq!(
        chat_quotas(Plan::Pro),
        &[q(30, QuotaWindow::Day), q(300, QuotaWindow::Month)]
    );
    assert_eq!(engine_quotas(Plan::Free), &[q(3, QuotaWindow::Week)]);
    assert_eq!(engine_quotas(Plan::Pro), &[q(20, QuotaWindow::Day)]);
    assert_eq!(serde_json::to_value(Plan::Pro).unwrap(), "pro");
}

#[test]
fn week_starts_monday_utc() {
    let now = datetime!(2026-11-22 23:59 UTC); // a Sunday
    assert_eq!(
        window_start(QuotaWindow::Week, now),
        datetime!(2026-11-16 00:00 UTC)
    );
    assert_eq!(
        window_reset(QuotaWindow::Week, now),
        datetime!(2026-11-23 00:00 UTC)
    );
    // Monday 00:00 is the first instant of its own week.
    let monday = datetime!(2026-11-23 00:00 UTC);
    assert_eq!(window_start(QuotaWindow::Week, monday), monday);
    // A non-UTC offset is converted first: Monday 01:00 +02:00 is Sunday 23:00 UTC.
    assert_eq!(
        window_start(QuotaWindow::Week, datetime!(2026-11-23 01:00 +2)),
        datetime!(2026-11-16 00:00 UTC)
    );
}

#[test]
fn day_and_month_windows_are_utc() {
    let now = datetime!(2026-12-31 15:30 UTC);
    assert_eq!(
        window_start(QuotaWindow::Day, now),
        datetime!(2026-12-31 00:00 UTC)
    );
    assert_eq!(
        window_reset(QuotaWindow::Day, now),
        datetime!(2027-01-01 00:00 UTC)
    );
    assert_eq!(
        window_start(QuotaWindow::Month, now),
        datetime!(2026-12-01 00:00 UTC)
    );
    assert_eq!(
        window_reset(QuotaWindow::Month, now),
        datetime!(2027-01-01 00:00 UTC)
    );
    assert_eq!(
        window_reset(QuotaWindow::Month, datetime!(2026-02-10 00:00 UTC)),
        datetime!(2026-03-01 00:00 UTC)
    );
}

#[test]
fn exhausted_reports_the_first_full_quota_and_its_reset() {
    let now = datetime!(2026-11-21 12:00 UTC); // a Saturday
    let pro = chat_quotas(Plan::Pro);
    // Room in both windows.
    assert_eq!(exhausted(pro, |_| 29, now), None);
    // The day quota is full: it resets at the next midnight.
    assert_eq!(
        exhausted(pro, |_| 30, now),
        Some((q(30, QuotaWindow::Day), datetime!(2026-11-22 00:00 UTC)))
    );
    // Only the month quota is full.
    assert_eq!(
        exhausted(
            pro,
            |w| match w {
                QuotaWindow::Day => 0,
                _ => 300,
            },
            now
        ),
        Some((q(300, QuotaWindow::Month), datetime!(2026-12-01 00:00 UTC)))
    );
    // Both full: the first one in table order is reported.
    assert_eq!(
        exhausted(pro, |_| 1000, now).map(|(quota, _)| quota),
        Some(q(30, QuotaWindow::Day))
    );
    // Free: the fourth message of the week is refused until Monday.
    assert_eq!(exhausted(chat_quotas(Plan::Free), |_| 2, now), None);
    assert_eq!(
        exhausted(chat_quotas(Plan::Free), |_| 3, now),
        Some((q(3, QuotaWindow::Week), datetime!(2026-11-23 00:00 UTC)))
    );
}

#[tokio::test]
async fn default_entitlements_are_free_and_per_account() {
    let id = Uuid::from_u128(7);
    assert_eq!(DefaultEntitlements.plan(id).await, Plan::Free);
    assert_eq!(DefaultEntitlements.budget_scope(id).await, vec![id]);
}

#[test]
fn chat_anchor_json_shape() {
    let raw = json!({"kind": "move", "gameId": "g1", "ply": 24});
    let anchor: ChatAnchor = serde_json::from_value(raw.clone()).unwrap();
    assert_eq!(
        anchor,
        ChatAnchor::Move {
            game_id: "g1".into(),
            ply: 24
        }
    );
    assert_eq!(serde_json::to_value(&anchor).unwrap(), raw);

    let section = ChatAnchor::ReportSection {
        report_id: Uuid::from_u128(1),
        section: SectionKey::ClimbPlan,
    };
    assert_eq!(
        serde_json::to_value(&section).unwrap(),
        json!({"kind": "report_section", "reportId": "00000000-0000-0000-0000-000000000001", "section": "climb_plan"})
    );
    let line = json!({"kind": "opening_line", "lineKey": "w-italian"});
    assert_eq!(
        serde_json::to_value(serde_json::from_value::<ChatAnchor>(line.clone()).unwrap()).unwrap(),
        line
    );
    assert!(serde_json::from_value::<ChatAnchor>(json!({"kind": "move", "gameId": "g1"})).is_err());
    assert!(serde_json::from_value::<ChatAnchor>(json!({"kind": "nope"})).is_err());
}

#[test]
fn chat_anchor_rejects_extra_keys() {
    for raw in [
        json!({"kind": "move", "gameId": "g1", "ply": 24, "x": 1}),
        json!({"kind": "game", "gameId": "g1", "accountId": "a"}),
        json!({"kind": "weakness", "weaknessId": "W1", "weakness_id": "W2"}),
    ] {
        assert!(
            serde_json::from_value::<ChatAnchor>(raw.clone()).is_err(),
            "accepted {raw}"
        );
    }
}

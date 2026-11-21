//! The ONLY place plan and quota decisions are made (owner input, binding). R3 owns it until M7
//! takes it over. Quotas are per account until users exist (ruling Q3).
//!
//! | Plan | Chat messages | Engine checks |
//! |---|---|---|
//! | Free | 3 per week | 3 per week |
//! | Pro | 30 per day and 300 per month | 20 per day |
//!
//! Windows are UTC: a day starts at 00:00, a week on Monday 00:00, a month on the 1st 00:00.

use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use time::{Date, Duration, Month, OffsetDateTime, Time};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Plan {
    Free,
    Pro,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QuotaWindow {
    Day,
    Week,
    Month,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Quota {
    pub limit: u32,
    pub window: QuotaWindow,
}

const FREE_CHAT: [Quota; 1] = [Quota {
    limit: 3,
    window: QuotaWindow::Week,
}];
const PRO_CHAT: [Quota; 2] = [
    Quota {
        limit: 30,
        window: QuotaWindow::Day,
    },
    Quota {
        limit: 300,
        window: QuotaWindow::Month,
    },
];
const FREE_ENGINE: [Quota; 1] = [Quota {
    limit: 3,
    window: QuotaWindow::Week,
}];
const PRO_ENGINE: [Quota; 1] = [Quota {
    limit: 20,
    window: QuotaWindow::Day,
}];

/// Chat-message quotas of a plan.
pub fn chat_quotas(plan: Plan) -> &'static [Quota] {
    match plan {
        Plan::Free => &FREE_CHAT,
        Plan::Pro => &PRO_CHAT,
    }
}

/// Interactive engine-check quotas of a plan.
pub fn engine_quotas(plan: Plan) -> &'static [Quota] {
    match plan {
        Plan::Free => &FREE_ENGINE,
        Plan::Pro => &PRO_ENGINE,
    }
}

fn midnight(date: Date) -> OffsetDateTime {
    date.with_time(Time::MIDNIGHT).assume_utc()
}

fn first_of_month(year: i32, month: Month) -> Date {
    // Day 1 exists in every month of every representable year.
    Date::from_calendar_date(year, month, 1).expect("day 1 is valid")
}

/// The start (inclusive, UTC) of the window containing `now`.
pub fn window_start(w: QuotaWindow, now: OffsetDateTime) -> OffsetDateTime {
    let today = now.to_offset(time::UtcOffset::UTC).date();
    match w {
        QuotaWindow::Day => midnight(today),
        QuotaWindow::Week => {
            let back = i64::from(today.weekday().number_days_from_monday());
            midnight(today - Duration::days(back))
        }
        QuotaWindow::Month => midnight(first_of_month(today.year(), today.month())),
    }
}

/// The end (exclusive, UTC) of the window containing `now`: when its quota resets.
pub fn window_reset(w: QuotaWindow, now: OffsetDateTime) -> OffsetDateTime {
    let start = window_start(w, now);
    match w {
        QuotaWindow::Day => start + Duration::days(1),
        QuotaWindow::Week => start + Duration::days(7),
        QuotaWindow::Month => {
            let d = start.date();
            let (year, month) = match d.month() {
                Month::December => (d.year() + 1, Month::January),
                m => (d.year(), m.next()),
            };
            midnight(first_of_month(year, month))
        }
    }
}

/// The first exhausted quota (`used(window) >= limit`) and its reset time, or `None` when every
/// quota has room. `used(window)` returns the count already used in that window.
pub fn exhausted(
    quotas: &[Quota],
    used: impl Fn(QuotaWindow) -> u32,
    now: OffsetDateTime,
) -> Option<(Quota, OffsetDateTime)> {
    quotas
        .iter()
        .find(|q| used(q.window) >= q.limit)
        .map(|q| (*q, window_reset(q.window, now)))
}

/// Plan and budget-scope lookup. M7 replaces [`DefaultEntitlements`] with the billing-backed one.
pub trait Entitlements: Send + Sync {
    fn plan<'a>(&'a self, account_id: Uuid) -> BoxFuture<'a, Plan>;
    /// The accounts that share one budget (including `account_id`).
    fn budget_scope<'a>(&'a self, account_id: Uuid) -> BoxFuture<'a, Vec<Uuid>>;
}

/// Every account is on the free plan and has its own budget.
#[derive(Clone, Copy, Debug, Default)]
pub struct DefaultEntitlements;

impl Entitlements for DefaultEntitlements {
    fn plan<'a>(&'a self, _account_id: Uuid) -> BoxFuture<'a, Plan> {
        Box::pin(async { Plan::Free })
    }

    fn budget_scope<'a>(&'a self, account_id: Uuid) -> BoxFuture<'a, Vec<Uuid>> {
        Box::pin(async move { vec![account_id] })
    }
}

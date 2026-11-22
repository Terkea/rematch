//! The ONLY place plan and quota decisions are made (owner input, binding). R3 owns it until M7
//! takes it over. Quotas are per account until users exist (ruling Q3).
//!
//! | Plan | Chat messages | Engine checks | Manual reports |
//! |---|---|---|---|
//! | Free | 3 per week | 3 per week | one non-onboarding report per 30 days |
//! | Pro | 30 per day and 300 per month | 20 per day | no limit |
//! | Beta | 100 per week (`AI_BETA_CHAT_WEEKLY`) | 20 per day | 1 per UTC day (`AI_BETA_MANUAL_REPORTS_PER_DAY`, 1 to 24) |
//!
//! Beta is the private beta's plan (M5b ruling G1, ADR 0075). The worker gives every account
//! the `AI_DEFAULT_PLAN` plan (default `beta`); the web reads the same three variables. Its budget
//! is the Pro one, with the global cap on top ([`crate::client::cost::Budgets::for_plan`]).
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
    /// The private beta (M5b ruling G1).
    Beta,
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
/// The beta defaults (M5b ruling G1).
pub const BETA_CHAT_PER_WEEK: u32 = 100;
pub const BETA_MANUAL_REPORTS_PER_DAY: u32 = 1;
const BETA_CHAT: [Quota; 1] = [Quota {
    limit: BETA_CHAT_PER_WEEK,
    window: QuotaWindow::Week,
}];
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
        Plan::Beta => &BETA_CHAT,
    }
}

/// Interactive engine-check quotas of a plan. Beta has the Pro allowance.
pub fn engine_quotas(plan: Plan) -> &'static [Quota] {
    match plan {
        Plan::Free => &FREE_ENGINE,
        Plan::Pro | Plan::Beta => &PRO_ENGINE,
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

/// `AI_DEFAULT_PLAN`: every account's AI plan (`beta`, `free` or `pro`; default `beta`).
pub const AI_DEFAULT_PLAN_ENV: &str = "AI_DEFAULT_PLAN";
/// `AI_BETA_CHAT_WEEKLY`: the beta plan's coach messages per week.
pub const BETA_CHAT_WEEKLY_ENV: &str = "AI_BETA_CHAT_WEEKLY";
/// `AI_BETA_MANUAL_REPORTS_PER_DAY`: the beta plan's manual reports per UTC day, 1 to 24.
pub const BETA_MANUAL_REPORTS_PER_DAY_ENV: &str = "AI_BETA_MANUAL_REPORTS_PER_DAY";
/// The allowed `AI_BETA_MANUAL_REPORTS_PER_DAY` values.
pub const BETA_MANUAL_REPORTS_PER_DAY_RANGE: std::ops::RangeInclusive<u32> = 1..=24;

/// The beta plan's configurable limits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BetaLimits {
    pub chat_per_week: u32,
    pub manual_reports_per_day: u32,
}

impl Default for BetaLimits {
    /// [`BETA_CHAT_PER_WEEK`] and [`BETA_MANUAL_REPORTS_PER_DAY`].
    fn default() -> Self {
        BetaLimits {
            chat_per_week: BETA_CHAT_PER_WEEK,
            manual_reports_per_day: BETA_MANUAL_REPORTS_PER_DAY,
        }
    }
}

/// A beta-limit variable that is set but is not a non-negative integer (or, for the manual
/// reports, not 1 to 24).
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{var} must be a non-negative integer (manual reports: 1 to 24), got {value:?}")]
pub struct BetaLimitsError {
    pub var: String,
    pub value: String,
}

impl BetaLimits {
    /// Reads [`BETA_CHAT_WEEKLY_ENV`] and [`BETA_MANUAL_REPORTS_PER_DAY_ENV`]. An unset (or
    /// empty) variable keeps the default; an invalid one is a startup error.
    pub fn from_env() -> Result<Self, BetaLimitsError> {
        Self::from_vars(|k| std::env::var(k).ok())
    }

    /// [`BetaLimits::from_env`] over any variable lookup.
    pub fn from_vars(get: impl Fn(&str) -> Option<String>) -> Result<Self, BetaLimitsError> {
        let read = |var: &str,
                    default: u32,
                    allowed: std::ops::RangeInclusive<u32>|
         -> Result<u32, BetaLimitsError> {
            let bad = |v: String| BetaLimitsError {
                var: var.to_owned(),
                value: v,
            };
            match get(var) {
                None => Ok(default),
                Some(v) if v.trim().is_empty() => Ok(default),
                Some(v) => match v.trim().parse::<u32>() {
                    Ok(n) if allowed.contains(&n) => Ok(n),
                    _ => Err(bad(v)),
                },
            }
        };
        let d = BetaLimits::default();
        Ok(BetaLimits {
            chat_per_week: read(BETA_CHAT_WEEKLY_ENV, d.chat_per_week, 0..=u32::MAX)?,
            manual_reports_per_day: read(
                BETA_MANUAL_REPORTS_PER_DAY_ENV,
                d.manual_reports_per_day,
                BETA_MANUAL_REPORTS_PER_DAY_RANGE,
            )?,
        })
    }

    /// The chat quotas of `plan` with these beta limits.
    pub fn chat_quotas(&self, plan: Plan) -> Vec<Quota> {
        match plan {
            Plan::Beta => vec![Quota {
                limit: self.chat_per_week,
                window: QuotaWindow::Week,
            }],
            other => chat_quotas(other).to_vec(),
        }
    }

    /// The manual reports `plan` may build per UTC day; `None` when the plan has no daily
    /// limit (the free plan has its 30-day interval instead).
    pub fn manual_reports_per_day(&self, plan: Plan) -> Option<u32> {
        (plan == Plan::Beta).then_some(self.manual_reports_per_day)
    }
}

/// Plan and budget-scope lookup. M7 replaces [`DefaultEntitlements`] with the billing-backed one.
pub trait Entitlements: Send + Sync {
    fn plan<'a>(&'a self, account_id: Uuid) -> BoxFuture<'a, Plan>;
    /// The accounts that share one budget (including `account_id`).
    fn budget_scope<'a>(&'a self, account_id: Uuid) -> BoxFuture<'a, Vec<Uuid>>;
    /// The beta plan's limits; the built-in defaults unless configured.
    fn beta_limits(&self) -> BetaLimits {
        BetaLimits::default()
    }
    /// The chat quotas of `plan`.
    fn chat_quotas(&self, plan: Plan) -> Vec<Quota> {
        self.beta_limits().chat_quotas(plan)
    }
    /// The manual reports `plan` may build per UTC day (`None`: no daily limit).
    fn manual_reports_per_day(&self, plan: Plan) -> Option<u32> {
        self.beta_limits().manual_reports_per_day(plan)
    }
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

/// One plan for every account, each with its own budget, and the configured beta limits. The
/// worker wires it from `AI_DEFAULT_PLAN` and the `AI_BETA_*` variables until M7's
/// billing-backed entitlements. The default is the beta plan with its default limits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConfiguredEntitlements {
    pub plan: Plan,
    pub beta: BetaLimits,
}

impl Default for ConfiguredEntitlements {
    fn default() -> Self {
        ConfiguredEntitlements {
            plan: Plan::Beta,
            beta: BetaLimits::default(),
        }
    }
}

impl Entitlements for ConfiguredEntitlements {
    fn plan<'a>(&'a self, _account_id: Uuid) -> BoxFuture<'a, Plan> {
        let p = self.plan;
        Box::pin(async move { p })
    }

    fn budget_scope<'a>(&'a self, account_id: Uuid) -> BoxFuture<'a, Vec<Uuid>> {
        Box::pin(async move { vec![account_id] })
    }

    fn beta_limits(&self) -> BetaLimits {
        self.beta
    }
}

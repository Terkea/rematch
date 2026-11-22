# 0075. Coach data for the beta: the beta AI plan, weekly-review inputs, the first report

Status: accepted (2026-11-22, M5b Task B; coordinator rulings G1, G2 and G3)

## Context

The M5b coach pages need real data. Three gaps in the R3 wiring blocked that:

- **G1.** The worker wired `DefaultEntitlements`, so every beta tester was on the free plan: 3 coach messages a week, and one manual report per 30 days. That is too little to test the coach.
- **G2.** The weekly schedule used `NullWeeklyProvider`, so no weekly review was ever written.
- **G3.** Nothing in the backend requested a report. Only the web's "Build my report" button did.

## Decision

### G1: a beta AI plan

- `ai::entitlements::Plan::Beta` (`"beta"`). The migration `20261122000000_ai_plan_beta.sql` adds the value to `ai_plan`, so `report_runs.plan` can record it. The Drizzle schema is regenerated.
- Its limits are `BetaLimits`:
  - `AI_BETA_CHAT_WEEKLY` sets the coach messages per week (default 100);
  - `AI_BETA_MANUAL_REPORTS_PER_DAY` sets the manual reports per UTC day (default 1, allowed 1 to 24).
  - An empty variable keeps the default. Any other invalid value is a startup error.
  - The names, defaults and range are the coordinator's, shared with the web, which reads the same variables. Compose sets them for both `rust-worker` and the web services, and `.env.example` documents them.
- `Entitlements` gains `beta_limits()`, `chat_quotas(plan)` and `manual_reports_per_day(plan)`. Their defaults are the built-in tables, so existing implementations are unchanged.
- `ConfiguredEntitlements { plan, beta }` gives every account one plan, each account with its own budget. The worker builds it with `ai_entitlements(env)`: the plan is `AI_DEFAULT_PLAN` (`beta`, `free` or `pro`; default `beta`), and the limits come from the environment. `AI_DEFAULT_PLAN` is separate from `WORKER_DEFAULT_PLAN`, the analysis plan. It replaces `DefaultEntitlements` in the worker. That type stays, as the free plan for tests.
- Chat checks the quotas that the entitlements return (`check_chat_allowance_with`).
- A **manual** report run on a plan with a daily limit fails with `plan_interval` once that many other manual reports of the account × time class exist in the current UTC day. The check runs again under the report-version lock, so two racing runs get one report.
  - Onboarding, scheduled and profile-change runs are never gated.
  - The free plan's 30-day interval is unchanged.
- **Budgets and metering are unchanged.** The beta plan's monthly cap is the Pro one (USD 2 per account, `AI_BUDGET_PRO_MICRO_USD`). The global cap applies on top, and every call is still metered.
- Engine checks: beta gets the Pro allowance of 20 a day. The ruling sets only chat and reports, and 3 a week would block testing the chat's engine tool.
- The monthly report batch stays Pro-only, so beta testers rebuild their reports on demand.

### G2: weekly-review inputs from R2 data

- `profile::coaching::weekly::build_weekly_input(pool, account, tc, week_start, now)` builds TS plan B4's `WeeklyReviewInput` field for field. `profile` does not depend on `ai`, so the crate has a mirror type. The tests validate its output against `backend/fixtures/ai/input/weekly-review-input.schema.json`. `ai` generates that schema from its own type, and a test keeps it current (`WEEKLY_SCHEMA_WRITE=1`).
- **No review:** the builder gives `None` for a week with no game in the time class, or one whose games have no current-version metrics yet. There is nothing to review, and no tokens are spent on nothing.
- The fields:
  - `gamesThisWeek` is the games played in the week.
  - `band` and `basis` are the profile's, from the coaching input built at `now`, the time the review is written.
  - `adherence` is `null`. No practice is tracked yet, and "0 of 0" would claim a plan that does not exist.
  - `metricDeltas` covers each contract metric with a weekly snapshot both in this week and in the latest earlier week that has one. The values come from the latest stored rows, unchanged.
    - The verdict is `improved` or `worse` only when both weeks are sufficient, the metric has a better direction, and the stored intervals are disjoint. Otherwise it is `no_clear_change`.
    - A first week has no deltas.
  - `focusUpdates` are the top 3 ranked weaknesses, one per weakness id, with their state and `P_imp`.
  - `exemplars` are up to 2 of those focuses' positions played in the week, from distinct games.
  - `changes` are the profile's timeline changes whose onset row was detected (`at_played_at`) in the week.
- `weekly_due(pool, week_start)` lists the account × time classes with a game in the week and at least one linked user (`account_links`, or `linked_accounts.user_id`).
- The worker's `ProfileWeeklyProvider` wires both into the weekly batch schedule. It runs the non-`Send` builder on a blocking thread, as `ProfileInputProvider` does.
- **Changed in T20:** the Monday 04:00 schedule now reviews the last complete week (`reviewed_week(now)`, the Monday before this week's). It used to review the week starting that same Monday, which had no games yet. `weekly_reviews.week_start` is the reviewed week.
- Weekly reviews still go through the Batch API only, so they need `ANTHROPIC_API_KEY`. That is unchanged from ADR 0063.

### G3: the first report

- `worker::coach_triggers::FirstReportOnBuild` wraps the `build-profile` handler when the AI runners run. After a successful build it calls `request_first_report`.
- `request_first_report` takes a transaction-scoped advisory lock (class 30040, `first-report:<account>:<tc>`). It then creates a run (`trigger = onboarding`, the account's plan, `is_first_report`) only when all of these hold:
  - a user is linked to the account;
  - the account × time class has no report;
  - it has no unfinished run (`queued`, `running`, `awaiting_retry` or `awaiting_batch`);
  - it has no run that failed in the last day (`FIRST_REPORT_RETRY_AFTER`).
- After the commit it enqueues `generate-report` with T18's payload `{reportRunId}` and dedupe key `report:<run id>` (`kinds::enqueue_report`). A lost enqueue is repaired by `ai-reconcile`.
- A failure here is logged and never fails the build.
- Reports are account data (ADR 0064 §18), so one run serves every linked user.

## Consequences

- Beta testers can use the coach: 100 messages a week and a report a day, within a hard per-account monthly cap and the global cap. The cost if this is wrong: more of the owner's quota, still capped.
- The web reads the same three variables, so its allowance display and report-request rules match the worker's.
- The first report arrives without a button press, as soon as a linked account's first profile build completes.
- The `profile` mirror of `WeeklyReviewInput` can drift from `ai`'s type only if the schema fixture is regenerated without rerunning the `profile` tests. The gate runs both.

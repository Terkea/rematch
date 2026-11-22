# 0069. Report requests and report states on the web

Date: 2026-11-22. Status: accepted (M5b Task 2). Builds on ADR 0032 (R1 queue), ADR 0059 (AI
jobs), ADR 0061 (models, budgets and degradation), ADR 0063 (batch regeneration) and ADR 0064
§18 (coaching rows are account data).

## Context

M5b's report page needs "Build my report" and "Rebuild report", and must show a report honestly
while it is being built, after it degraded and after it failed. R3's `request_report` inserts the
`report_runs` row, commits, then enqueues `generate-report` in a second transaction; a lost
enqueue waits for `ai-reconcile` (up to 5 minutes). The worker refuses a free-plan `manual` run
inside the 30-day interval only after it is claimed (`plan_interval`).

## Decision

**Requests (ruling R4).** `requestReport` (`apps/web/src/server/coaching/request.ts`) and
`POST /api/coach/report` do, in **one** transaction:

1. Scope check: the account must be one of the user's links (`scopedAccountIds`), else
   `not_found` (404; a malformed id is also 404, a body that is not `{ accountId, timeClass }` 400).
2. `pg_advisory_xact_lock(30030, hashtext(account_id || ':' || time_class))`. Class 30030 is
   unused (R3 uses 30001–30004 and 30020), so two clicks, two tabs or a re-post serialise.
3. A live run (`queued`, `running`, `awaiting_retry`, `awaiting_batch`) is returned (202
   `{ runId, building: true }`); nothing is inserted.
4. Trigger `onboarding` and `is_first_report = true` when the scope has no `reports` row,
   otherwise `manual`. A `manual` request less than `FREE_REPORT_INTERVAL_DAYS` (30) after the
   newest report is refused on the web (429 `{ error: "plan_interval", availableAt }`), so the
   player never waits for a job that would fail. The interval is a parameter (`intervalDays`),
   so the beta plan (ruling G1, one manual report a day, configured through env) can pass its own
   value once the worker's beta entitlement lands.
5. Insert the run (`plan = "free"`, `prompt_version = "report@1"`, `mode = sync`) and its job
   with `enqueueJob` (`kind = generate-report`, payload `{ reportRunId }`, dedupe key
   `report:<runId>`, `user_key` = account id, priority 50, `max_attempts` 3), the exact values of
   `backend/crates/ai/src/jobs/kinds.rs`, pinned by `test/coaching/enqueue.test.ts`.

`enqueueJob` (`apps/web/src/server/jobs/enqueue.ts`) is R1's `ENQUEUE_SQL`: the same columns,
`run_at = now()`, the live-only `ON CONFLICT (kind, dedupe_key) ... DO NOTHING` and
`pg_notify('jobs', kind)`, which Postgres delivers at commit.

**Why one transaction.** A run row without its job shows "building" for up to 5 minutes until
the reconciler repairs it, and a job without its run is impossible to explain. On the web the
two commit together or not at all; the reconciler still covers a job lost later (a dead worker).

**States.** `reportState(run, report, ready)` reads the newest run and the newest report of the
scope. The copy each state maps to (Task 6 renders it; the computed part of the newest report
always stays visible):

| State | When | Copy |
|---|---|---|
| `none` | no run, no report | "Your first report is built from your analysed games." with "Build my report" |
| `building` | run `queued` or `running` | "Building your report. Sections appear as they are written." Ready sections list in `ready`; the others read "Being written" |
| `building`, `retryAt` | run `awaiting_retry` | "The AI service is busy. We will try again at 14:05." |
| `building`, `batch` | run `awaiting_batch` | "Your report is queued for tonight's batch and is ready by morning." |
| `ready` `complete` | run `done` | the report, with its AI disclosure |
| `ready` `partial` | some sections dropped | each dropped section reads "This section was held back because it did not pass our checks." |
| `ready`, `aiOff: no_api_key` | `ai_disabled_reason` | "Coaching text is switched off on this server. The engine findings below are complete." |
| `ready`, `aiOff: budget` | `ai_disabled_reason` | "This report reached its AI budget. The remaining sections are engine findings only." |
| `failed` `plan_interval` | `last_error` | "You can rebuild once every 30 days. Next on 12 Dec 2026." |
| `failed` `budget` | | "The AI budget for today is used up. Try again tomorrow." |
| `failed` `no_input` / `invalid_input` | | "We need more analysed games before a report can be built." / "Something in your analysed games could not be read. Try again after your next import." |
| `failed` `ai_unavailable` / `ai_auth` / `ai_error` / `job_dead` / `reconcile_cap` / `unknown` | | "The report could not be built. Try again later." |

`failed` with `hasPrevious` keeps the previous report on the page under the failure line. An
unknown `last_error` is `unknown`, never shown raw. `getReport` returns `nextManualAt` (newest
report `created_at` + 30 days, while in the future) so the button can say when it is available.

**Payload.** `assembleReportPayload` is the TS port of Rust `assemble_report_payload`: sections
in `SECTION_KEYS` order, a dropped section null, a section not generated absent, `dropped`
without `detail`, status from the ready count, the disclosure dropped for `computed_only`. It is
checked against `backend/fixtures/ai/payload/report-payload*.example.json`.

## Consequences

- The web writes `report_runs` and `jobs` only; all reads go through `readOnly`.
- The web duplicates the free interval check; the worker's check stays authoritative.
- Report, weekly-review and note rows are readable by every user linked to the account (ADR 0064
  §18); a foreign or malformed account sees nothing.

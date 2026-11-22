# 0074. Coach e2e: a guarded coach seed, a simulated worker, and the coach baselines

Status: accepted (2026-11-22, M5b Task 11)

## Context

The coach pages (report, weaknesses, skill areas, progress, the coach chat) show what the R3 worker writes: report sections arriving one by one, chat answers streamed as `chat.chunk` events and settled by `chat.message.ready` or `chat.message.failed`. The e2e stack runs `web` and `fakes` only. It has no worker, and no test may call a model (M5b Global Constraints, "AI in tests"). The M5a game-page specs already seed a synthetic account (`e2e-fixtures`, ADR 0065) and refuse a database that holds a real account.

## Decision

### The coach seed (`apps/web/e2e/helpers/seed-coach.ts`)

- `seedE2eCoach()` writes the coach world for the adult e2e user on `e2e-fixtures`, rapid:
  - `game_metrics` rows for the 15 seeded rapid games, so the coach scope resolves to them;
  - 8 weakness estimates across every state, one of them an early read (`W7.traps`, 12 of the 40 chances it needs), with state history and per-game series points;
  - weekly KPI rows for 2026-W36 to 2026-W47, with 2026-W41 stored as a null value with n 0 (a gap, never a zero);
  - area scores, with calculation stored null and no Mindset score;
  - two report versions from R3's synthetic `report-payload.example.json` (14 Sep and 13 Oct 2026). The newer run holds the synthetic `coaching-input.b2-rapid.json` as its input, so the weakness pages have example boards (ADR 0068 R2);
  - one ready and one pending weekly review, and one ready change note;
  - a thread with a done answer, and a second thread whose answer is still streaming (the privacy check streams chunks to it).
- It reuses M5a's `seedE2eGames` and the Task 1, 2 and 5 test writers. Before writing anything it refuses a database whose linked accounts are not all e2e ones. The message is the same as M5a's: "Refusing to seed: this database holds real accounts".
- Every call first clears the account's coaching rows, then writes them again. `support/game-pages.ts` `seedFixtures()` runs it after the games seed. So the overview, the coach pages and their baselines read the same whichever spec ran before. For example, `coach.spec.ts` rebuilds a report and asks questions, and the next seed takes those rows away.
- **All dates are fixed.** The brief asked for a report "40 days before the run", so that a rebuild would be allowed under the free plan's 30-day interval. Ruling G1 (ADR 0075) gives the beta one manual report a day, so 13 Oct 2026 allows a rebuild on any later day. Fixed dates also keep the text snapshots stable. The seeded chat answers are dated before the current week, so the allowance line does not change from week to week.
- `link <email>` links a second, already signed-in user to the account (unverified, by username). `spend <n>` adds `n` done answers created now, as another device would, for the cap check.

### The simulated worker (`apps/web/e2e/helpers/simulate-worker.ts`)

- `simulateWorker(db, step)` plays one step of the worker. Each step runs in one transaction and writes only what the R3 job writes, with its event last:
  - **`report-section`:** the first step of a queued run claims it (`running`). It writes the next report version as `computed_only` with `report.section.ready {section: "computed"}`. Then it writes the section row from the fixture and refreshes the report's status and disclosure (as `refresh_status` does). It announces `areas` once per area item (`area:<id>`). Once every section is stored, the run is `done` and `report.ready` follows.
  - **`chat-chunk`:** `pending` becomes `streaming`, and `chat.chunk` is emitted.
  - **`chat-done`:** stores the validated text (the chunks joined, or with `replace` a fixed validated text), its segments and the disclosure, then emits `chat.message.ready {replace}`.
  - **`chat-failed`:** `failed` with the reason, then `chat.message.failed`. Both chat endings act only on an unfinished message, as `finish_message` and `fail_message` do.
  - **`report-failed`:** `runs::set_failed` (`failed`, `last_error`, `finished_at`), the run's job failed, then `job.failed {kind: "generate-report", reason}`.
  - **`import-done`:** the account's due `import-account` job is done, an `import_runs` row is closed as `done` with its count, and `import.progress {runId, imported, done: true}` follows. It writes no games, and it leaves `linked_accounts.last_synced_at` unset: the web never reads it, and the seeds' guard treats a synced account as a real one.
- Chat events carry the thread owner's `user_id` (`emit_for_user`); report and import events carry none. Chat events carry their `attempt` (Task 9's Rust field): the first chunk reserves one (`turn_attempts + 1`). No AI call is made.
- `test/e2e-simulate-worker.test.ts` pins these writes against Postgres. It caught one mistake while being written: a section's content is its envelope (`{ areas: { items } }`).

### The specs

- `e2e/coach.spec.ts` runs the 9 steps of the brief in order (`describe.serial`). Step 5 reads the second user's event log by wrapping `EventSource.addEventListener` in an init script. It asserts that an account event arrives, so the stream is shown to be live, and that no `chat.chunk` does.
- Step 6's denial is realistic: 99 of the week's 100 answers are used, another device uses the last while the question waits, and the job's `weekly_cap` then arrives.
- `report.spec.ts`: "Build my report" from a scope with no report, the five sections arriving in order without a reload, then a failed build whose immediate retry is throttled with its time.
- `sync.spec.ts`: Sync now enqueues the real `import-account` job, the panel says it is checking and Sync now is unavailable, then a simulated import finishes, and both panels read "Synced just now. No new games." with the cooldown.
- `touch-targets.spec.ts` hit-tests the coach states' "Try again" across its 44px band (a parked Task 3 item).
- `game-pages-quality.spec.ts` covers the coach routes too: no sideways scroll at 360, visible focus and no broken numbers. The coach routes also get axe (WCAG 2 A and AA, serious and critical), as `quality.spec.ts` does.
- `game-pages-visual.spec.ts` gains "the coach pages look as they did": the report, a weakness, the endgame area, progress and a coach thread, at 390 and 1440, day and night, each as a PNG and a text snapshot. The overview, games and review baselines are regenerated because the header now has seven sections. Baselines are made in the Linux `qa` container only (ADR 0065).

### Navigation

- The header lists Overview, Games, Report, Weaknesses, Progress, Coach and Accounts.
- Seven sections do not fit a 360px row. Below `sm`, the row keeps Overview, Games, Report and Coach, and the account menu lists the rest under "More pages", so the row never scrolls sideways.
- The command menu adds a "Skill areas" group with the seven area pages and an "Ask your coach" action (`/app/coach/new`).

### What the e2e found

- **The browser dropped every coach event.** `use-event-stream.ts` listened only for the four M4 types, and an `EventSource` delivers only the types it listens for. So in SSE mode no report section, weekly review, note, chat chunk or profile event reached a page; only polling would have brought them. `STREAM_TYPES` now lists every kind, and `test/events/stream-types-parity.test.ts` reads them from the Rust sources (`events::store`, `ai::events`).
- **Sync now got 415.** The account routes' CSRF guard needs a declared JSON body, and the panel sent none. It now does, and the panel test pins the header.

### Coach chat switch

`lib/coach-chat.ts` `coachChatLive()` is the one place that takes the chat out of the product: when false, the header hides Coach, the command menu drops "Ask your coach", and the report, weakness and area pages leave out their ask links. It was false while Task 9 was still finishing, and it is true now that Task 9 is merged.

## Consequences

- Every spec that seeds the fixtures also gets the coach rows. A coach-page change can move the overview's baseline (its lead finding), and the baseline is regenerated with it.
- `seedE2eCoach({ report: false })` (CLI `no-report`) seeds the same world with no report. Each seed also clears the account's imports and import jobs, so the overview reads "Not synced yet." on every run.
- The simulator mirrors the jobs by hand. If an R3 job changes what it writes, `simulate-worker.ts` and its test change with it.

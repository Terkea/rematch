# 0059. AI tables, jobs and events

Date: 2026-11-21. Status: accepted. Builds on ADR 0032 (Postgres job queue), ADR 0033 (SQL migrations own the schema), ADR 0034 (user events) and ADR 0062 (Agent SDK sidecar, amendment 1).

## Context

The AI layer needs durable state for report runs, reports and their sections, weekly reviews, weakness-change notes, the coach chat, interactive engine checks, usage and Message Batches. In TS M3 this state lived in Drizzle tables, with the work running on BullMQ queues. The Rust backend has one Postgres job queue and one user-events table, so the AI layer is built on those.

## Decision

### Migration

There is one migration, `db/migrations/20261121030000_ai_coaching.sql`. R3 owns versions `20261121030000` to `20261121039999`, R1 owns `…000000` to `…009999`, and R2 owns `…020000` to `…029999`. Enums are bound as text and cast in SQL, and they are read back as `col::text`.

### Tables and their states

| Table | States |
|---|---|
| `report_runs` | `queued` → `running` (claimed by one atomic `UPDATE … WHERE status IN ('queued','awaiting_retry')`) → `done` or `failed`. A transient AI failure goes to `awaiting_retry`, with `retry_count` and `next_attempt_at`. Sections submitted to the Batches API go to `awaiting_batch`. `heartbeat_at` lets the reconciler find dead runs. `input` freezes the CoachingInput, so a resume regenerates from the same data. |
| `reports` | Versioned per (account, time class): `UNIQUE (account_id, time_class, version)`. The status is `computed_only` (no section ready), `partial` or `complete` (all five ready). |
| `report_sections` | One row per (report, section), either `ready` or `dropped`. An upsert reports `Changed(true)` only when the row is new or its status or content differ (jsonb equality). Callers emit `report.section.ready` only on a change. |
| `weekly_reviews`, `weakness_change_notes` | `pending` → `ready`, `dropped` (validation failed twice) or `failed` (retries exhausted). A transient failure stays `pending` with `next_attempt_at`. |
| `coach_conversations`, `coach_messages` | Messages go `pending` → `streaming` → `done` or `failed`. User messages are inserted `done`. `(conversation_id, seq)` is unique. `session` (`new`, `resumed` or `rebuilt`) records how the Agent SDK session served each assistant turn, for metrics. `session_model_usage` stores the SDK's cumulative per-session `modelUsage` after the turn. The next turn meters only the difference from `chat::previous_session_model_usage`. `user_id` is indexed. Its FK to the web's `"user"` (`ON DELETE CASCADE`) is added by Task 26's integration migration, because r3/base has no web auth tables. |
| `interactive_evals` | `pending` → `done` or `failed`. `source` is checked to be `'interactive'`. No KPI, stats, profile or report code reads it. Each row is one charge against the quota. A failure the player did not cause is refunded with `charged = false`. Deleting a conversation sets `message_id` to NULL but keeps the row, so deleting can never reset the quota. |
| `ai_usage` | One row per model call, with our own cost (ADR 0062 decision 4). It feeds `spend_since` and `global_spend_since`. |
| `ai_batches`, `ai_batch_items` | A batch goes `submitted` → `ended` → `processed`, or to `failed`. Each item is processed at most once (`processed_at`), so a re-poll cannot apply a result twice. |
| `ai_sdk_session_entries` | The opaque Agent SDK transcript entries (ADR 0062 amendment 1). The PK is `(project_key, session_id, subpath, seq)`, with an index on `session_id`. A partial unique index on `(project_key, session_id, subpath, entry->>'uuid') WHERE entry ? 'uuid'` lets the sidecar append with `ON CONFLICT DO NOTHING`. There is no FK to the conversation, because the sidecar may write before Rust commits. The trigger `coach_conversations_delete_session_entries` (an `AFTER DELETE` row trigger on `coach_conversations`) deletes a conversation's entries, and `ai-reconcile` sweeps orphans (see Deletion). |

### Report versions

Report versions are serialised with a transaction-scoped advisory lock on (account, time class), then `coalesce(max(version), 0) + 1` is read in a new statement. The plan said `FOR UPDATE` on the account's reports. That cannot lock a first report that does not exist yet, and Postgres rejects `FOR UPDATE` together with `max()`. The lock uses the two-int key space (class `30001`), which never overlaps R1's single-bigint user-events lock.

Callers use READ COMMITTED, the default. Under REPEATABLE READ or SERIALIZABLE, the caller's snapshot predates the lock wait, so `max()` misses a version committed in the meantime. The insert then fails on the unique constraint (or with a serialization failure) and must be retried. The data is never wrong.

### Indexes for the background scans

Each scan has a partial index that it reads in order. A test guards each plan with sequential and bitmap scans disabled:

| Scan | Index |
|---|---|
| live runs (`runs::live_runs`, the four live statuses as literals) | `report_runs (created_at)`, partial on the live statuses |
| one status (`runs::runs_with_status`, the status as a literal) | `report_runs_live_idx` or `report_runs_status_idx`, never a seq scan |
| `ai-reconcile` pages (`jobs::reconcile::sweep_pages`) | the partial `created_at` index of each table, including `coach_messages (created_at) WHERE status = 'pending'` (migration `20261121030100`) |
| pending evals (`interactive::stale_pending`) | `interactive_evals (created_at)`, partial on `WHERE status = 'pending'` |
| pending reviews and notes | `weekly_reviews (created_at)` and `weakness_change_notes (created_at)`, partial on `WHERE status = 'pending'` |
| open batches (`batches::open_batches`) | `ai_batches (last_polled_at NULLS FIRST, submitted_at)`, partial on `WHERE status IN ('submitted','ended')` |

Statuses are literals in the SQL, never bound. A bound `report_run_status[]` proved the partial predicate only in a custom plan: under a cached generic plan the planner fell back to `report_runs_status_idx` plus a sort (amendment 1). The plan guards run every scan under both `force_custom_plan` and `force_generic_plan`. The FK lookups have their own indexes: `interactive_evals (message_id)` serves the `SET NULL`, `ai_batch_items (report_run_id)` and `(weekly_review_id)` serve the cascades, and `coach_conversations (user_id)` serves the M4 cascade.

### Deletion

The `AFTER DELETE` row trigger `coach_conversations_delete_session_entries` deletes `ai_sdk_session_entries WHERE session_id = OLD.id` in the same transaction as the conversation. It fires on every delete path: `chat::delete_conversation(account_id, id)` and `delete_all_conversations(account_id)` (both scoped to the account), a raw SQL delete, the `linked_accounts` cascade and the future `"user"` cascade. Messages go by cascade.

The sidecar can still write entries for a conversation that has not committed yet, or that was deleted in the middle of a turn. `ai-reconcile` runs `chat::sweep_orphan_session_entries(ORPHAN_SESSION_MIN_AGE = 10 min)` to delete entries whose session has no conversation and whose `created_at` is older than 10 minutes. The 10-minute grace covers a first turn whose conversation commits after the sidecar's first write. Task 21 wires the call into the `ai-reconcile` handler.

### The sidecar role

The migration creates the role `ai_sidecar` as NOLOGIN and grants it SELECT, INSERT and DELETE on `ai_sdk_session_entries` only. No secret goes in SQL: operations grant LOGIN and set the password, and the sidecar connects through `AI_SESSION_DATABASE_URL`. Roles are cluster-wide, so creation is idempotent and tolerates concurrent migrations (parallel test databases). If the migrating user lacks CREATEROLE, the role is left to operations, the grant is skipped and a NOTICE is raised. The grants are `USAGE` on schema `public` and SELECT, INSERT and DELETE on `ai_sdk_session_entries`. A test asserts that the role holds no other privilege on any relation or sequence in `public`.

Operations, once per environment (or whenever the migration skipped the role):

```sql
-- as a superuser or a CREATEROLE user, in the application database
DO $$ BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'ai_sidecar') THEN
    CREATE ROLE ai_sidecar NOLOGIN;
  END IF;
END $$;
ALTER ROLE ai_sidecar LOGIN PASSWORD :'ai_sidecar_password';   -- psql -v ai_sidecar_password=...
GRANT CONNECT ON DATABASE chess TO ai_sidecar;                  -- the application database
GRANT USAGE ON SCHEMA public TO ai_sidecar;
GRANT SELECT, INSERT, DELETE ON ai_sdk_session_entries TO ai_sidecar;
```

The sidecar's `AI_SESSION_DATABASE_URL` then names `ai_sidecar` and that password.

### Events are emitted in the transaction

`events::emit(tx, account_id, type, payload)` wraps R1's `insert_user_event` on the caller's connection. The event commits with the state it describes: a rolled-back write leaves no event, and a client that refetches on an event always sees the new state. It must be the last statement before the commit, because of the advisory lock that orders events (ADR 0034).

The event types are `report.section.ready`, `report.ready`, `report.degraded`, `weekly_review.ready`, `weakness.note.ready`, `chat.chunk`, `chat.message.ready`, `chat.message.failed` and `job.failed`. Payloads are camelCase.

### Job kinds replace the BullMQ queues

| Kind | Payload | Dedupe key | Priority | Max attempts | Recurrence |
|---|---|---|---|---|---|
| `generate-report` | `{reportRunId}` | `report:<id>` | 50 | 3 | |
| `weekly-review` | `{weeklyReviewId}` | `weekly:<id>` | 100 | 3 (default) | |
| `explain-weakness-change` | `{noteId}` | `wchange:<id>` | 100 | 3 (default) | |
| `coach-chat` | `{messageId}` | `chat:<id>` | 10 | 2 | |
| `interactive-analyse` | `{evalId}` | `ieval:<id>` | 1 | 1 | |
| `report-batch-schedule` | `{}` | the kind | 100 | | every 24 h, first at the next 03:00 UTC |
| `weekly-batch-schedule` | `{}` | the kind | 100 | | every 7 d, first at the next Monday 04:00 UTC |
| `ai-batch-poll` | `{}` | the kind | 100 | | every 600 s |
| `ai-reconcile` | `{}` | the kind | 100 | | every 300 s |

`user_key` is the account id. The plan is not in the chat payload, because the handler asks `Entitlements`. `ensure_ai_recurring` is idempotent, because a recurring job never leaves the live states.

`Queue::enqueue` commits in its own transaction, so a row (a run, a review, a note or a message) can commit while its enqueue fails or the process dies. `ai-reconcile` is the recovery: it re-enqueues unfinished rows that have no live job. The dedupe keys make a double enqueue harmless.

### Retry schedule through `Release`

A transient AI failure (unavailable, rate limited, or an open breaker) does not use a queue attempt. The handler stores the incremented `retry_count` and `next_attempt_at` on its row and returns `JobError::Release { delay }`, where the delay is `retry_delay(n, retry_after) = max(RETRY_DELAYS[n-1], min(retry_after, 24 h))` and `RETRY_DELAYS` is 5 min, 30 min, 2 h, 6 h and 24 h. After the fifth retry, `retry_delay` returns `None` and the item fails. Queue attempts are left for crashes and stalls.

### The web hand-off (M4)

M4 inserts `coach_messages` rows (the user turn `done`, then the assistant turn `pending` with the next `seq`). If it enqueues outside that transaction, `ai-reconcile` recovers a lost job. M4 enqueues `coach-chat` jobs through the same `jobs` table: kind `coach-chat`, payload `{"messageId": "<uuid>"}`, dedupe key `chat:<uuid>`, `user_key` the account id, priority 10 and `max_attempts` 2. It reads `reports`, `report_sections` and `coach_messages` (with `segments`). When M4 deletes a conversation (or the user, through Task 26's `user_id` FK), the trigger `coach_conversations_delete_session_entries` removes the Agent SDK transcript, so M4 needs no extra delete. The Drizzle schema is regenerated from this migration (ADR 0033). Two normaliser rules were added: the empty-array default of `interactive_evals.moves`, and the empty-string default of `ai_sdk_session_entries.subpath`, which drizzle-kit emits as a broken literal.

## Consequences

- One migration holds every AI table, and later R3 tasks change behaviour, not schema.
- Every repository function takes `&mut PgConnection`, so state changes and their events compose into one transaction.
- `load_report_rows` returns the raw report and its sections. Task 14's `assemble_report_payload` builds the `ReportPayload` from them, which keeps the db layer free of payload types.
- Session entries written by an in-flight turn after a delete are swept by `ai-reconcile` after 10 minutes. The chat handler should still check that the conversation exists before it streams (Task 25).

## Amendment 1 (Task 21): the `ai-reconcile` sweep

`jobs::reconcile::reconcile_ai(pool, queue, now)` runs every 300 s through `AiReconcileHandler`. It follows R1's reconciler: fenced updates, keyset pages, and every await bounded. Every step runs even if an earlier one fails; the errors come back together (`AiReconcileError::Steps`).

| Sweep | Repair |
|---|---|
| `queued` runs older than 5 min with no live `report:<id>` job | enqueue. A batch-mode run younger than `BATCH_SUBMIT_GRACE` (30 min) may be mid-submit and is skipped. |
| `running` runs with a heartbeat older than 10 min and no live job | set `queued` (fenced on `running` and the stale heartbeat), then enqueue |
| `awaiting_retry` runs more than 5 min past `next_attempt_at` with no live job | enqueue |
| due `pending` weekly reviews and change notes older than 5 min with no live job | enqueue. A batch-mode review is skipped while it is younger than `BATCH_SUBMIT_GRACE` or has an unprocessed item in a `submitted` or `ended` batch (the poller's). |
| `pending` chat messages older than 2 min with no live job | set `failed` with `"lost"` and emit `chat.message.failed` in the same transaction. A chat answer is stale by then, so it is never retried. |
| `streaming` chat messages whose `streaming_at` is older than `CHAT_TURN_TIMEOUT` (10 min) + 30 s + 60 s, with no live job | the same, fenced on `streaming` and the cutoff |
| `pending` interactive evals older than 2 min with no live `ieval:<id>` job | set `failed` with `charged = false` (a refund) |
| session entries with no conversation, between 10 min and 24 h old | delete in pages of 1,000 (`chat::sweep_orphan_session_entries_page`), on every sweep, within a 60 s budget. Older orphans cannot exist: the delete trigger removes a deleted conversation's entries. |

`awaiting_batch` runs belong to the batch poller. `runs::set_awaiting_batch` is fenced on `queued` only, so a run a worker is generating synchronously cannot also be paid for in a batch.

**Crash cap.** Each reconciler re-enqueue (the queued, stale-running, retry, weekly and note sweeps) first bumps the row's `reconcile_count` (`smallint`, migration `20261121030100`) in a fenced update in its own transaction. When the count passes `RECONCILE_CAP = 3`, the row is failed instead (`last_error = 'reconcile_cap'` on runs; weekly reviews and notes have no error column) and the same transaction emits `report.degraded` (a run with a linked report, so the computed report still shows) or `job.failed {kind, reason: "reconcile_cap"}`. The bump precedes the enqueue, so a sweep cut off in between over-counts by one and never loops.

**Batch grace.** `BATCH_SUBMIT_GRACE` is 30 minutes. Task 20 keeps its whole submit (retries included) below it and inserts batch-mode rows so the sweep ignores them until the submit commits.

**Ordering.** A stale run is reset before its enqueue. If the enqueue came first, a job could claim nothing, because the row is still `running`, and then finish. A reset whose enqueue is lost leaves a `queued` run past the grace, and the next sweep's first step finds it.

**Paging.** Pages are keyset windows ordered by `(age column, id)`, with a literal status, read from each table's partial index on that column (an Incremental Sort adds the `id` tie-break). Where the age is the order column, the cutoff bounds the scan. Otherwise a `due` flag carries the age test, as in R1. Migration `20261121030100_ai_reconcile_indexes.sql` adds `coach_messages_pending_idx (created_at) WHERE status = 'pending'`, `coach_messages.streaming_at` (set by `chat::set_streaming`) with `coach_messages_streaming_idx (streaming_at) WHERE status = 'streaming'`, and the `reconcile_count` columns. The orphan page reads `ai_sdk_session_entries_created_idx` from an inclusive cursor. All pages are plan-guarded under the custom and the generic plan.

## Amendment 2 (Task 26): integration

- **Migrations.** R3 owns `20261121030000`, `030100`, `030200`, `030300`, `030400`, `030500` and
  `031800`. Each version is unique across `main` (web `010000`/`010100`), R1 (`0000xx`) and R2
  (`0200xx`), and all sort after the R1 and R2 tables they reference (`linked_accounts`,
  `games`, `time_class`). No renumbering was needed. `030500` adds the user FK of ADR 0064 §20.
  The comments of the already-applied R3 migrations still name the placeholders (R3-1 … R3-8 =
  0057 … 0064): sqlx checksums every applied file, so they are never edited.
- **The reconcile sweep is bounded twice over.** An orphan session-entry page reads at most its
  limit of rows in time order, orphan or not, and moves its cursor past them, so live entries are
  never rescanned within a sweep; a tied instant wider than a page is cleared at once and
  stepped past. **Fix round 1 (review I1):** the sweep resumes where the last one stopped
  (`ai_sweep_cursors`, migration `030600`, clamped to the 24 h window). At the window's end the
  cursor stays at the newest entry read: an entry's orphan status is fixed once it is past the
  10 min grace (its conversation committed or never will; a deleted conversation's entries go
  with it), so nothing behind the cursor is read again, and orphans behind more live entries
  than one budget covers are reached within a few sweeps. This assumes the worker's and the
  database's clocks differ by less than the 10 min grace. The whole sweep starts no step after `SWEEP_DEADLINE` (3 min, so the last step's
  60 s budget and one 35 s page still end below the runner's 5 min); skipped steps run next sweep (`skippedSteps`). The unbudgeted
  `sweep_orphan_session_entries` is gone from the public API.
- **Events.** See ADR 0064 §18 (the audience table and the `emit` guard).

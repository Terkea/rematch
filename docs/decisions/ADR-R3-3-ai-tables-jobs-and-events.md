# ADR-R3-3. AI tables, jobs and events

Date: 2026-11-21. Status: accepted. Builds on ADR 0032 (Postgres job queue), ADR 0033 (SQL migrations own the schema), ADR 0034 (user events) and ADR-R3-6 (Agent SDK sidecar, amendment 1).

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
| `coach_conversations`, `coach_messages` | Messages go `pending` → `streaming` → `done` or `failed`. User messages are inserted `done`. `(conversation_id, seq)` is unique. `session` (`new`, `resumed` or `rebuilt`) records how the Agent SDK session served each assistant turn, for metrics. |
| `interactive_evals` | `pending` → `done` or `failed`. `source` is checked to be `'interactive'`. No KPI, stats, profile or report code reads it. Each row is one charge against the quota. A failure the player did not cause is refunded with `charged = false`. Deleting a conversation sets `message_id` to NULL but keeps the row, so deleting can never reset the quota. |
| `ai_usage` | One row per model call, with our own cost (ADR-R3-6 decision 4). It feeds `spend_since` and `global_spend_since`. |
| `ai_batches`, `ai_batch_items` | A batch goes `submitted` → `ended` → `processed`, or to `failed`. Each item is processed at most once (`processed_at`), so a re-poll cannot apply a result twice. |
| `ai_sdk_session_entries` | The opaque Agent SDK transcript entries (ADR-R3-6 amendment 1). The PK is `(project_key, session_id, subpath, seq)`, with an index on `session_id`. A partial unique index on `(project_key, session_id, subpath, entry->>'uuid') WHERE entry ? 'uuid'` lets the sidecar append with `ON CONFLICT DO NOTHING`. There is no FK to the conversation, because the sidecar may write before Rust commits. |

### Report versions

Report versions are serialised with a transaction-scoped advisory lock on (account, time class), then `coalesce(max(version), 0) + 1` is read in a new statement. The plan said `FOR UPDATE` on the account's reports. That cannot lock a first report that does not exist yet, and Postgres rejects `FOR UPDATE` together with `max()`. The lock uses the two-int key space (class `30001`), which never overlaps R1's single-bigint user-events lock.

### Deletion

`chat::delete_conversation(account_id, id)` and `delete_all_conversations(account_id)` delete the conversations, their messages (by cascade) and their `ai_sdk_session_entries` rows (`session_id` = conversation id) in one statement, so in one transaction. They are scoped to the account.

### The sidecar role

The migration creates the role `ai_sidecar` as NOLOGIN and grants it SELECT, INSERT and DELETE on `ai_sdk_session_entries` only. No secret goes in SQL: operations grant LOGIN and set the password, and the sidecar connects through `AI_SESSION_DATABASE_URL`. Roles are cluster-wide, so creation is idempotent and tolerates concurrent migrations (parallel test databases). If the migrating user lacks CREATEROLE, the role is left to operations, the grant is skipped and a NOTICE is raised.

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

### Retry schedule through `Release`

A transient AI failure (unavailable, rate limited, or an open breaker) does not use a queue attempt. The handler stores the incremented `retry_count` and `next_attempt_at` on its row and returns `JobError::Release { delay }`, where the delay is `retry_delay(n, retry_after) = max(RETRY_DELAYS[n-1], min(retry_after, 24 h))` and `RETRY_DELAYS` is 5 min, 30 min, 2 h, 6 h and 24 h. After the fifth retry, `retry_delay` returns `None` and the item fails. Queue attempts are left for crashes and stalls.

### The web hand-off (M4)

M4 inserts `coach_messages` rows (the user turn `done`, then the assistant turn `pending` with the next `seq`) and enqueues `coach-chat` jobs through the same `jobs` table: kind `coach-chat`, payload `{"messageId": "<uuid>"}`, dedupe key `chat:<uuid>`, `user_key` the account id, priority 10 and `max_attempts` 2. It reads `reports`, `report_sections` and `coach_messages` (with `segments`). The Drizzle schema is regenerated from this migration (ADR 0033). Two normaliser rules were added: the empty-array default of `interactive_evals.moves`, and the empty-string default of `ai_sdk_session_entries.subpath`, which drizzle-kit emits as a broken literal.

## Consequences

- One migration holds every AI table, and later R3 tasks change behaviour, not schema.
- Every repository function takes `&mut PgConnection`, so state changes and their events compose into one transaction.
- `load_report_rows` returns the raw report and its sections. Task 14's `assemble_report_payload` builds the `ReportPayload` from them, which keeps the db layer free of payload types.
- Session entries written by an in-flight turn after a delete would survive. The chat handler must check that the conversation still exists before streaming (Task 25).

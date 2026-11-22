# 0070. Coach chat: the web's hand-off to the worker, and the allowance it shows

Date: 2026-11-22. Status: accepted (M5b Task 5). Builds on ADR 0064 (Ask your coach), ADR 0062
(the Agent SDK sidecar), ADR-WEB-8 (SSE delivery) and R3 Task 25 (`backend/crates/ai/src/chat`).
Rulings R5 and R6 of the M5b plan, and the coordinator's ruling G1 (beta entitlements).

## Context

R3 built the chat in Rust: `chat::threads` creates, posts, lists, reads and deletes threads,
and the `coach-chat` job answers one pending assistant message, streaming `chat.chunk` events
and ending with `chat.message.ready` or `chat.message.failed`. Those three events are
user-owned (`user_events.user_id`, `Audience::User`). The web (TypeScript) has to offer the
same operations without a Rust HTTP API, and without ever talking to the sidecar.

## Decision

1. **Streaming over the existing event stream (R5).** The web writes `coach_conversations`,
   `coach_messages` and the `coach-chat` job, and nothing else. The worker's `chat.*` events reach
   the browser through `/api/events`, whose hub and replay already deliver a user-owned event
   only to its owner (`eventVisibleTo`). There is no web-to-sidecar route; the sidecar's URL and
   token are not in the web's environment (pinned by `test/coach/hand-off.test.ts`).
2. **One-transaction post.** `postUserMessage` ports `post_user_message`: blank (only Unicode
   `White_Space`, as Rust `str::trim`; a BOM is not blank) is `empty`, more
   than 2000 Unicode scalar values is `too_long`, the conversation row is locked `FOR UPDATE`
   (scoped by `user_id`), an assistant answer `pending` or `streaming` makes it `busy`, the user
   turn (`done`) goes at `max(seq) + 1` and the `pending` answer after it, `last_message_at =
   now()`. The job (`{"messageId": …}`, key `chat:<id>`, `user_key` the account, priority 10,
   2 attempts) is enqueued **in the same transaction** (R1's `ENQUEUE_SQL` on the caller's
   transaction, Task 2). That is stronger than Rust, which enqueues after its commit; the
   reconciler still fails an answer whose job was lost.
3. **One-transaction start, deduplicated.** `POST /api/coach/threads` with a question runs
   `startConversation`: the create, both turns and the job commit together, so a refused (429)
   or failed first post leaves no thread behind. Under the user's create lock, a repeat of the
   same start (account, time class, anchor and question, or an empty start with no messages)
   within 60 seconds answers the first thread, so a retry or a double submit never makes a
   second thread, job or quota charge. No client key is needed and no migration.
4. **Ownership.** Every read, post and delete is filtered by `coach_conversations.user_id`;
   another user of the same linked account gets not-found (404), never 403. Creating and posting
   also need the user's own `account_links` row; posting after unlinking is `403 not_linked`,
   while reading and deleting still work (as Rust). Creating on an account the user is not
   linked to is answered as not-found, so no account's existence leaks; so is listing threads of
   such an account (`GET /api/coach/threads?account=` is 200, `[]` when empty, only for a linked
   account). A single thread stays readable by id after unlinking.
5. **Chunk replay.** On a reload mid-answer, `getThread` returns, for each `pending` or
   `streaming` answer, the `chat.chunk` payloads already in `user_events` with `user_id` = the
   user, the conversation's account, and `payload->>'messageId'` = the message, deduplicated and
   in `seq` order. Live chunks then append through `applyCoachEvent` (deduplicated by `seq`), so
   nothing streamed is lost or doubled. Events older than the 7-day retention are gone, but by
   then the answer has long ended.
6. **Copy parity.** The chat's fixed server copy (fallback, refusal, off-topic, no-data,
   unavailable, engine-pending, the dropped-statement and engine-limit notes, the cap texts and the
   fallback titles) is mirrored byte for byte from `limits.rs` and `summary.rs`, pinned by
   `test/coach/copy-parity.test.ts`. Web-only `failureCopy` covers the reasons Rust has no player
   text for (`budget`, `not_linked`, `lost`, anything new).
7. **The allowance (R6).** `chatAllowance` counts exactly as the job's `used_per_window`:
   assistant answers in the account's conversations that are `streaming`, or `done` with a
   non-null `ai_disclosure` (fixed no-data copy is free), created inside each UTC window (day from
   00:00, week from Monday 00:00, month from the 1st). The composer is disabled with the reset copy
   when a quota is exhausted, but the web never refuses a post for quota: the job is the
   authority, and its denial arrives as `chat.message.failed` with the cap reason.
8. **The plan.** Quotas are `free` 3 a week and `pro` 30 a day and 300 a month (Rust
   `entitlements.rs`), plus `beta` for the private beta (ruling G1): 100 a week, set by
   `AI_BETA_CHAT_WEEKLY`. The web reads the deployment's plan from `AI_DEFAULT_PLAN` (default
   `beta`), the variable the plan names for the worker's entitlements (Task B wires the Rust
   side to the same names and defaults), so the composer shows what the job will enforce.
9. **Abuse bounds, not quotas.** A user may post 10 questions per 10 minutes over all their
   threads (429 `rate_limited` with `retry-after`) and start 30 conversations an hour. Each is
   counted under a per-user transaction advisory lock (classes 30031 for posts, 30032 for creates;
   R4 uses 30030, R3 30001–30004 and 30020), so the count is exact. Bodies are capped at 16 KiB;
   strings with U+0000 or lone surrogates are refused (400 `invalid_body`); a malformed anchor is
   dropped, never stored.

## Consequences

- A chat answer needs no new service, port or secret on the web side.
- The post holds a row lock for a few statements only; two concurrent posts give one `posted`
  and one `busy`.
- Follow-up for M7: `AI_DEFAULT_PLAN` and `AI_BETA_CHAT_WEEKLY` give way to the billing-backed
  entitlements, read through one function on each side (`aiPlan()` on the web,
  `Entitlements::plan` in Rust).
- Follow-up for M7: the quota's budget scope is the account (Rust `DefaultEntitlements::
  budget_scope`), so a user with several linked accounts (verified or not) gets one allowance per
  account. The billing-backed entitlements should scope it per user, or count verified links only.
- If the worker's plan variable and the web's ever disagree, the composer's count is wrong but
  the quota still holds (the job decides).

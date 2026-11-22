# 0063. Monthly and weekly regeneration through the Message Batches API

Date: 2026-11-22. Status: accepted (Task 20). Builds on ADR 0059 (AI tables and jobs), ADR 0060
(sectioned generation; Task 16's batch processing), ADR 0061 (budgets) and ADR 0062 (batches stay
on the minimal Rust Messages client and need `ANTHROPIC_API_KEY`).

## Context

Pro players get a fresh report every month and a weekly review every week. Neither is waited on
by a player, so both can use the Message Batches API, which costs half of a sync call. Batches
are asynchronous (up to 24 hours), results come back in any order, and a submit or a results
read can fail half-way. A result must never be applied to the wrong player or report, and every
billed call must be metered exactly once.

## Decision

1. **What goes through batches.** The monthly report regeneration (`report-batch-schedule`,
   daily at 03:00 UTC: each (account, time class) on the Pro plan, within budget, whose newest
   report is at least 30 days old and that has no unfinished run) and the weekly reviews
   (`weekly-batch-schedule`, Mondays at 04:00 UTC, for the items a `WeeklyInputProvider`
   reports due). Both are metered at batch rates (50%) with `ai_usage.batch = true`. Nothing is
   scheduled without an AI client or when `!batches_enabled()` (no API key), checked before any
   row is written.
2. **Identical requests.** A batch item is exactly the sync request of its generator
   (`build_section_request`, `build_weekly_request`), so the system prompt and cached prefix are
   the same as the sync path's, and a result is validated by the same validator.
3. **Keyed by custom id, resolved in any process.** The custom id is
   `<schema>__v<version>__<item uuid>` (Task 16); `RegistryResolver` maps it to the schema in any
   worker. The poller decodes the item id, looks up the `ai_batch_items` row by custom id, and
   applies the result only if the row's id equals the decoded id and the row belongs to the
   batch being polled. The run (or review), its input snapshot and its section come from that
   row, never from the result. A result for another batch or an unknown item is ignored and
   logged; it is never applied. A row whose run or review no longer waits for the batch gets its
   billed calls metered and nothing else.
4. **Idempotent per item.** One transaction per result marks the item processed (fenced on
   `processed_at IS NULL`, under a row lock of the run or review), meters its calls with
   `meter_keyed` under `batch:<item id>`, stores the section or review, and emits its events
   last. A batch is polled under a transaction-scoped advisory lock (released on every path,
   including a cancelled poll), with its status re-read under the lock, so two pollers never
   process it at once; the item fence and the keyed metering each also hold on their own. A
   re-poll skips processed items; a processed batch is not polled again.
5. **Fallback to sync.** `RetrySync` (unusable output, a misrouted schema, or an `errored`,
   `canceled` or `expired` result), a failed repair, and an item that got no result from a
   complete stream (none was sent, or its line could not be decoded: such a line is skipped and
   the rest of the stream still applies) store nothing for that item. When a run's last item is processed it is done
   (`report.ready`) if all five sections are stored; otherwise it goes back to `queued` and
   `generate-report` is enqueued, which generates only the missing sections. A weekly review gets
   `weekly-review` enqueued. Errored, canceled and expired results are not billed and meter
   nothing.
6. **Limits and expiry.** Items are packed by whole runs into batches of at most 10,000 requests
   and 200 MiB of create body (the API allows 256 MB). A submit, retries included, is bounded by
   10 min, below the reconciler's 30 min `BATCH_SUBMIT_GRACE`; rows are created in batch mode,
   which the reconciler ignores for that grace, and a failed submit leaves them for the
   reconciler to generate synchronously. The API ends every batch within 24 hours and keeps
   results for 29 days; the poller (every 10 min) bounds each status call (60 s) and each results
   read (20 min; a cut-off read resumes next poll). Every batch has an end:
   - a batch not ended 48 hours after its submit is given up;
   - a batch whose results cannot be read completely for 24 hours after it ended (`ended_at`,
     set at the first poll that sees it ended) is given up the same way, so a results endpoint
     that keeps failing (a revoked key, retention passed) cannot hold runs in `awaiting_batch`;
   - giving up sends the unprocessed items to sync and marks the batch `failed`.
7. **No `fallbacks`.** The API's server-side `fallbacks` is not used (ADR-AI-6, ruling Q1).

## Consequences

- Monthly and weekly AI costs halve; a regenerated report can take up to a day.
- **Accepted (controller ruling): a lost submit can be paid twice.** If the API created a batch
  but its response is lost (or the commit that records it fails), the database does not know the
  batch, and after `BATCH_SUBMIT_GRACE` the reconciler regenerates its rows synchronously. That
  work is paid twice (batch plus sync) but never applied twice: the batch's items were never
  recorded, so its results match no item. It is rare (a 2xx that cannot be read or recorded) and
  logged with the batch id. Avoiding it would need an idempotency key the Batches API does not
  offer.
- **Accepted (controller ruling): the weekly schedule checks budget, not plan.** Weekly reviews
  go to whatever the `WeeklyInputProvider` reports due; the beta plan is unlimited, and the
  per-scope monthly budget still bounds the spend.
- **Results of a deleted account are not metered.** When a run or review (and its account) is
  deleted while its batch is open, the cascade deletes its items, so its billed results match no
  item and are not metered. The spend of a deleted account is not tracked.
- Dates are `time::Date` throughout (`WeeklyInputProvider::due(week_start: Date)`); chrono is not
  used (ADR 0031).
- Sections are stored by Task 18's `report_job::persist_section_outcome` (store, refresh the
  status and disclosure, return the event labels), shared by the sync `on_section` and the
  poller; only the metering key differs.

## Amendment (Task 26): a paid repair is metered when it returns

The parked T20 item. A sync repair of a batch result is now metered right after it returns, in
its own transaction, under `batch:<item>:repair:<uuid>:<n>`; the item's transaction meters only
the batch result (`batch:<item>:<n>`, once). If the item's write then fails, the next poll
repairs again and that repair is metered too: both were paid. Test:
`a_paid_repair_is_metered_when_its_item_write_fails`.

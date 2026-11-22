# 0076. Recurring incremental sync of linked accounts

Date: 2026-11-22. Status: accepted (M5b Task S). Builds on ADR 0032 (the job queue), ADR 0035
(resumable imports), ADR 0043 (the anonymous User-Agent) and ADR-R2-16 (import, then
schedule-analysis).

## Context

The owner asked for "a way to poll the latest games and analyse the progress". Until now an
import ran only when something enqueued it (the CLI, the reconciler after a stale run). New games
played on chess.com or Lichess never reached the coach on their own.

## Decision

1. **A recurring `sync-accounts` job** (worker, with the reconciler). Each linked account (an
   `account_links` row, or the CLI's `linked_accounts.user_id`) is synced about once per
   `SYNC_INTERVAL_MINUTES` (default 30, at least 1). The job recurs every sixth of the interval.
   A tick enqueues incremental imports for the due accounts: no live import job, and the newest
   `import_runs.started_at` more than `interval - tick` ago (or none). The longest-waiting go
   first, at most `ceil(linked / 6)` per tick, so the accounts spread over the interval instead
   of reaching the platforms in a burst. A new interval moves the live job's `recur_every` at
   the next start.
2. **Dedupe.** Sync imports keep the `import:<account>` dedupe key, so an account never has two
   live imports. Games upsert with `ON CONFLICT DO NOTHING`, so a sync never re-imports, resets
   or re-analyses a stored game.
3. **Incremental imports.** `ImportAccountPayload.incremental` (JSON `incremental: true`, omitted
   when false) fetches only games newer than the cursor's `newestSeenAt`. chess.com reads only the
   monthly archives from that month on. Lichess sends `since=`. An open backfill frontier is left
   to a full import. A Lichess sync re-checks 1 day below the watermark, except on the account's
   first sync of a UTC day (`last_synced_at` before today), which keeps the 30-day look-back for
   correspondence games that finish late. A full import is unchanged.
4. **Politeness.** Sync imports run on the import runner (2 at a time) through the shared
   per-platform clients, throttles, 429 `Retry-After` handling, breakers and the fixed
   `chess-improver/0.1 (+https://github.com/chess-improver)` User-Agent. No email is sent.
5. **Analysis and events.** The import enqueues `schedule-analysis` as before, and the analyses'
   profile hook enqueues `build-profile`. Each run publishes the account-scoped `import.progress`
   events (checkpoints, then `done: true`), and `job.failed` when it fails for good. The analyses
   publish `game.analysed`, and build-profile publishes `profile.updated` and `weakness.changed`.
6. **Manual "Sync now".** The web enqueues the same job (`import-account`, payload
   `{"accountId": "<id>", "incremental": true}`, dedupe key `import:<id>`, user key the account
   id, priority 100, max attempts 3), limited to once per 2 minutes per account.

## Consequences

- With 30 minutes, an active account costs one chess.com archive request, or one small Lichess
  stream, per half hour, plus one 30-day Lichess re-check a day.
- Every sync publishes an `import.progress` event even with no new games, so the web can show
  "synced just now".
- A correspondence game created before the day's re-check window and finishing more than 30
  days later is still missed, as before. A full import catches it.

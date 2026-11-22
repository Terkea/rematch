# 0077. "Sync now" and the analysis-progress panel

Date: 2026-11-22. Status: accepted (M5b Task P2). Builds on ADR 0076 (the recurring sync),
ADR 0069 (the web enqueues into `jobs`), ADR 0056 (live refresh), ADR-WEB-8 (the event stream)
and ADR-R2-17 (the honest ETA).

## Context

The owner asked for "a way to poll the latest games and analyse the progress". Task S syncs every
linked account about every 30 minutes. The player also needs to ask for a sync now and to see
where import and analysis stand without guessing.

## Decision

1. **Sync now** is `POST /api/accounts/<accountId>/sync`. The segment is the linked account id;
   Next names the folder `[linkId]` only because it shares it with `DELETE /api/accounts/<linkId>`.
   It enqueues exactly the job of Rust `enqueue_incremental_import`: kind `import-account`,
   payload `{ accountId, incremental: true }`, dedupe key `import:<accountId>`, user key the
   account, priority 100, 3 attempts. `test/sync/contract.test.ts` pins every value against
   `queue::kinds` and `analysis::jobs::import`.
   - Authorisation is the linked-account check (`userAccountIds`): any link the user holds,
     verified or not, like the games they read. A foreign or malformed id is 404.
   - At most one sync per 2 minutes per account. The clock starts at the newest
     `import_runs.started_at` or the newest due `import-account` job's `created_at`, whichever is
     later, so a sync the worker started counts too. Inside it the answer is 429 with
     `availableAt`, and the panel says "You can sync again in 2 min." The comparison and
     `availableAt` use the database's `now()`, never the web host's clock. "Due" is R1's
     `due!()`: `run_at <= now() OR run_at <= created_at`.
   - Any live import answers `already`. That includes a queued import the worker held back on
     purpose (a retry backoff, an open breaker, `account_busy`). Its `run_at` is never moved
     (fix round 1: pulling it forward overrode the platform throttles and used up attempts).
     The panel shows it as "Waiting to retry. Next attempt at about 14:05 UTC." with "Sync now"
     unavailable. Requests for one account are serialised with
     `pg_advisory_xact_lock(30040, hashtext(account_id))`.
2. **Retry** is `POST /api/sync/retry { accountId }`. It enqueues `analyse-game` (the reconciler's
   priority, `coalesce(analysis_priority, 100)`) for the account's `failed` games. It skips games
   with a live job, or with any job created in the last 2 minutes, and takes at most 500. The web
   writes no `games` row: the worker's claim moves the game on.
3. **Status** is `GET /api/sync`, and it is also read on the server for the first paint. Per account, in one read-only snapshot:
   - the import phase: `checking` (an import job is due or running), `paused`, or `idle`;
   - the newest finished run's time and its new-game count ("Synced 3 min ago. 12 new games
     found.");
   - whether the newest run failed;
   - the next allowed sync;
   - game counts by `games.analysis_status`: waiting, analysing, done, failed, retrying (a failed
     game with a live job), not picked (`imported`) and skipped;
   - whether a `build-profile` job is queued or running ("Profile updating…").
4. **The ETA mirrors the scheduler.** `server/sync/eta.ts` runs `scheduler::fair::FAIR_ORDER_SQL`
   and `eta::position_sql`'s tail byte for byte (pinned by the contract test). It uses the same
   rule: at least 5 finished analyses in the last 10 minutes (`job_metrics`), and every queued
   analyse job of the account placed exactly in the first 400 places of the fair order. Otherwise
   there is no ETA, never a guess. The text states its basis: "About 14 min to go at the pace of
   the last 10 minutes (31 games analysed)."
5. **Live updates.** `applySyncEvent` (called first in `applyEvent`) invalidates the status on
   `import.progress` with `done: true`, `game.analysed`, `job.failed`, `profile.updated` and
   `weakness.changed`. It refetches at most once per 3 s, with a trailing refetch.
   - A finished import with new games also invalidates the account's `games` key, so
     `LiveRefresh` re-renders the list. A run with no new games leaves the page alone.
   - With the panel mounted on a page that refreshes itself, an analysed game is left to that
     refresh, so the status is read once.
   - The first progress event of a run the status doesn't show yet (a sync the worker started)
     refetches the status once. After that, the panel shows the stream's own count
     ("7 new so far").
6. **Where it shows.**
   - The overview: two lines in place of the status line. The first has the totals over every
     account, labelled "All time controls". The second has the most pressing sync state and a
     "Details" link to the games page. There is one "Sync now" or "Sync all" and no tally, so
     the coaching headline stays on the first screen.
   - An unavailable "Sync now" stays focusable with `aria-disabled`. It has a dashed, faint
     edge, and its reason shows in a tooltip and in its accessible description.
   - The games page: the full panel above the filters, with the key notes on games not picked
     or skipped.
   - The header `ActivityIndicator`: the streamed import first, then "Checking for new games",
     then "Analysing 86 / 214 games" (summed over the accounts, with the soak stroke), then
     "Updating your profile". It shows counts only and never says "live".
7. **The tally.** Progress is drawn as ticks, one per game (or per group past 48 or 72), each
   state with its own shape:
   - done: a full ink stroke, written top to bottom when it turns done;
   - analysing: a dashed pencil stroke;
   - failed: a broken stroke;
   - waiting: a short stub.

   The sentence beside it carries the exact counts, and the tally is named for assistive tech.
   Failed games stay visible with "Retry N games". Red is not used, because a failed analysis is
   not the player's mistake.
8. **No "next automatic check" time.** The sync's tick and its per-tick cap make the next check
   approximate, so the panel does not state one (the honesty rule).

## Consequences

- If Rust changes the import job, the dedupe key, the ETA rule or the fair order, the contract
  test fails. Both sides must then change together.
- The panel runs one status query per page load and per invalidation window. The ETA query runs
  only for accounts with games outstanding.

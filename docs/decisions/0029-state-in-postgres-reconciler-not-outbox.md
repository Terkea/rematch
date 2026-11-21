# 0029. State in Postgres, queue as delivery; reconciler instead of an outbox

Date: 2026-11-21
Status: Accepted

## Context

The owner requires an async pipeline that notices crashes and picks work back up, and a UI that receives results as they surface (spec §13a). Workers can be killed mid-job, Redis can lose jobs, a job can hang without its process dying, the chess platforms go down, and deploys send SIGTERM. The brief calls this "ADR 0026"; 0026 was already taken, so it is 0029.

## Decision

**Durable state lives in Postgres; BullMQ only delivers work.**
- `games` carries `analysis_status`, `analysis_started_at`, `heartbeat_at`, `analysis_attempts` and `analysis_error`. Each attempt sets `analysing`, the start time and the heartbeat, and increments the attempt count.
- `games.pending_since` records when a game last became `pending`: on insert, when a retryable attempt fails, and when the reconciler resets it.
- `import_runs` records each import attempt (`running → done | failed | paused`, `imported`, heartbeat, `last_error`, and `error_code`: `not_found`, `rate_limited`, `timeout`, `circuit_open` or `unknown`, taken from the typed platform errors).
- `linked_accounts.import_cursor` is the import checkpoint `{ oldestReached, newestSeenAt }`.
- The queue is fed only after the DB commit. Nothing reads Redis to decide what the state *is*.

**A reconciler instead of an outbox.** An outbox table would make "commit, then enqueue" atomic, but it needs its own relay and its own table. Because every job is idempotent and the state rows already say what should be queued, a sweep can rebuild the queue from the DB. The `reconcile` queue has a repeatable job every 5 minutes (`upsertJobScheduler`, one scheduler id, so restarts do not add more). Each sweep:
1. re-enqueues games that have been `pending` for more than 2 minutes and have no live job. It reads them in keyset pages of 1000 and checks each page's liveness in one pipelined round trip: `HMGET <prefix>:analyse-game:analyse-<id> timestamp finishedOn` (BullMQ 6.7.8's job hash). No `timestamp` means no job; no `finishedOn` means live, whether waiting, active or a delayed retry. `enqueueAnalyse` uses the same check;
2. resets `analysing` games whose heartbeat (or start time) is older than 10 minutes to `pending` and re-enqueues them. When the game's job is still `active`, it changes nothing and counts the game as `hungActive`: the worker holds the lock, and the job deadline will end it;
3. marks a game `failed` ("exceeded max attempts (stalled or crashed worker)", with `job.failed`) instead of re-enqueueing it once `analysis_attempts` reaches 6, in both sweeps above. Without this, a game that keeps crashing its worker would be retried for ever;
4. closes `running` import runs with a stale heartbeat as `failed` and enqueues a resume (job id `resume-import-<runId>`). The import then continues from the cursor;
5. prunes `user_events` older than 7 days and logs counts by state.

**Heartbeats come from inside the work.** `evaluateGame` takes an `onProgress` callback, called after each engine result is cached. The analyse job writes `heartbeat_at` from it (at most every 10 s), so a loop that hangs stops beating. A timer would keep beating and hide the hang. Imports beat at each checkpoint.

**Retries and their status.** BullMQ workers use `lockDuration: 60_000` and `maxStalledCount: 2`, and jobs get 3 attempts with exponential backoff. A failed attempt that will be retried puts the game back to `pending` with its error. Only the final attempt marks it `failed` and publishes `job.failed`. `EnginePoolClosedError` (shutdown) always goes back to `pending`. A missing game or account throws `UnrecoverableError`. When BullMQ gives up on a job, including by `maxStalledCount` exhaustion, the analyse worker's `failed` listener marks the game `failed` unless it is already terminal. In BullMQ 6.7.8 that exhaustion only sets a deferred failure, and the next worker fails the job without running the handler, so the handler's own catch never runs.

**Fencing.** The attempt's increment uses `RETURNING analysis_attempts`. The attempt's final writes (`done`, `failed`, `pending`, `skipped`) and its heartbeats apply only `WHERE analysis_attempts = <that value>`. The `done` transaction claims the row first and rolls back before touching `moves` if a newer attempt owns it, and the handler then returns `superseded`. A delivery of a game already `done` at the current protocol returns immediately.

**Hard deadlines.** An attempt can hang while its worker stays alive and keeps renewing the lock, so stalled-job detection never fires. Each analysis attempt therefore has a 10-minute deadline and each import a 30-minute no-progress deadline. Both use an `AbortSignal`, threaded into `evaluateGame` (raced against each engine call) and the import stream (raced against each `next()`), and they throw `JobDeadlineError`, so the normal retry path takes over. The worker's Postgres connections also set `statement_timeout = 30 s`.

**Job retention and re-analysis.** Default job options keep completed jobs for at most 24 h, up to 1000 of them (`removeOnComplete: { count: 1000, age: 86400 }`), and failed jobs for 7 days. The analyse job id stays `analyse-<gameId>` (ADR 0028). `enqueueAnalyse` checks the job state: if a live job exists it does nothing; if a retained `completed` or `failed` job holds the id, it removes that job first, then adds. We chose this over `analyse-<gameId>-<attempts>` ids because one id per game lets the reconciler ask "is there a live job for this game?" directly, and stops two generations of the same game running at once. Imports re-enqueue every upserted game that is still `pending`, not only new rows, so an import that died between insert and enqueue recovers on retry.

**Imports resume from a cursor.** Games stream newest first. A pass fetches `(since: newestSeenAt, until: oldestReached)`, both inclusive. Every 50 games (or 15 s), and on failure, the pass checkpoints `oldestReached` (the oldest game committed so far) and the run's `imported` count. When a pass reaches its lower bound, `newestSeenAt` becomes the newest stored game and `oldestReached` is cleared. A run first finishes any interrupted pass, then fetches games newer than everything stored. Overlaps are harmless because games upsert on `(platform, platformGameId)`. Both importers gained `until`.

**Per-platform circuit breakers** (5 consecutive failures, 60 s cooldown, then one half-open probe) wrap every platform request and streamed chunk in the import job. A 404 is not counted as an outage. When the breaker is open, the import throws `CircuitOpenError` (which carries `retryInMs`). The run is recorded as `paused` with `error_code = circuit_open` and no `job.failed`. The import worker then calls `job.moveToDelayed(now + retryInMs, token)` and throws `DelayedError`, so the job waits out the cooldown without using an attempt and cannot be dead-lettered by an outage. **Lichess look-back:** Lichess lists games by creation time, so a correspondence game that finishes after a newer game was stored would fall below the watermark for ever. Incremental Lichess passes therefore start at `newestSeenAt − 30 days` (`LICHESS_SINCE_LOOKBACK_DAYS`). Upserts make the overlap free, and the watermark itself still records the newest game. Analysis carries on unaffected.

**User events.** `publishEvent` inserts into `user_events` (a bigserial `id` that is also the SSE `Last-Event-ID`, indexed on `(account_id, id)`), then `PUBLISH user-events:<accountId>`. A failed PUBLISH is swallowed because the row is the source of truth and clients replay with `id > last`. **Commit-ordered ids.** A bigserial id is handed out before commit, so a lower id could commit after a higher one that a reader has already passed, and the reader would never see it. `insertUserEvent` (`@ci/db`) therefore takes `pg_advisory_xact_lock(7262011)` before drawing the id. The lock is released only after the transaction commits, so ids become visible in id order, and `listEventsAfter(db, accountIds, afterId, limit)` (`id > afterId ORDER BY id`) is exactly-once and in order for a reader that polls or reconnects. This is the simplest correct option: every writer goes through `insertUserEvent`, and events are low-volume and inserted in their own short transactions after the state commit. We rejected an `xmin`/snapshot horizon because xid order is not id order, and a time lag is not strictly correct. Jobs publish only after the commit they describe: `import.progress` at each checkpoint and at the end, `game.analysed` after the moves transaction, and `job.failed` on the final failure.

**Graceful shutdown.** `installGracefulShutdown` handles SIGTERM and SIGINT once (later signals are logged and ignored). Workers drain for up to 60% of the 25 s grace period. If they have not drained, the engine pool is closed, which aborts in-flight analyses back to `pending` with a retry scheduled. Workers still open at 90% are force-closed, and stalled-job detection hands their jobs on. Then the pool, queues, DB and Redis are closed. The process exits 1 if any close failed. If setup fails partway, `startWorkers` closes whatever it had already created.

## Alternatives considered

- A transactional outbox with a relay process. It is more moving parts for a gap the reconciler already covers.
- Attempt-generation job ids (`analyse-<gameId>-<n>`). They allow concurrent generations and make "is it queued?" a scan.
- Timer-based heartbeats. They hide hung loops.
- Pausing the whole import queue when a breaker opens. The queue is shared by both platforms, so this would stop the healthy one.

## Consequences

- Recovery latency is bounded by the sweep interval: a lost enqueue is repaired within 5 minutes, and a hung analysis within 10 to 15 minutes.
- A hung job that still holds its BullMQ lock is left alone (`hungActive`) until its deadline ends it, at most 10 minutes for an analysis.
- A sweep costs O(pending / 1000) queries and pipelined Redis round trips.
- Event inserts are serialised globally. That is fine at domain-event rates, but a caller must not insert events inside a long transaction, because the lock is held until it commits.
- A game is given up on after 6 attempts in total, across all paths.
- With `max` set, a finished pass still counts as complete. Deeper backfill (ADR 0025 tiers) must pass an explicit `until`.

# 0032. Postgres job queue

Date: 2026-11-21
Status: Accepted. Implements 0030; replaces the BullMQ delivery layer of 0028 and 0029.

## Context

ADR 0029 keeps durable state in Postgres and uses BullMQ (Redis) only to deliver work. The Rust backend (ADR 0030) drops Redis, so delivery moves into Postgres too. The queue is the heart of crash recovery: a worker can die mid-job, be SIGTERMed on its final attempt, lose its lease while still running, or face a platform outage. The TS reviews fixed a set of rules that the replacement must keep.

## Decision

**One `jobs` table** (`db/migrations/20261121000100_jobs.sql`), with `job_state` = `queued | running | done | failed | dead | paused`.

```text
enqueue ─▶ queued ──claim (attempts + 1)──▶ running ──complete──▶ done
             ▲                                │
             ├── Retryable, attempts left ────┤── Retryable, final attempt ─▶ dead
             ├── Release (attempts − 1) ──────┤── Unrecoverable ────────────▶ failed
             └── recurring: complete or any failure
```

`paused` is non-terminal and holds its dedupe key (used by later tasks; the queue itself never claims it). `done`, `failed` and `dead` are terminal and set `finished_at`; Task 14 prunes them (24 h and 1000 per kind for done, 7 days for failed and dead).

**Claiming uses `FOR UPDATE SKIP LOCKED`** in one `WITH next AS (SELECT … ORDER BY priority, run_at, id FOR UPDATE SKIP LOCKED LIMIT n) UPDATE … RETURNING` statement over `jobs_ready_idx`. Concurrent claimers skip each other's locked rows instead of blocking or double-claiming, so each job is claimed exactly once (tested with 20 concurrent claimers over 300 jobs) without advisory locks or a coordinator. Lower `priority` runs first, then the earliest `run_at`. Rows with `run_at` in the future are delayed jobs.

**Attempts are counted at claim.** The claim increments `attempts`, so a worker that crashes mid-job has used the attempt, exactly like a BullMQ delivery. The stall reaper (Task 14) gives the attempt back when it re-queues an expired lease and counts a stall instead (`max_stalls = 2`, then `dead` with `error_code = stalled`).

**Lease-token fencing.** Each claim writes a fresh `lease_token` (`gen_random_uuid()`), `lease_owner` and `lease_until`. `renew`, `complete` and `fail` all carry `WHERE id = $1 AND lease_token = $2 AND state = 'running'`. A worker whose lease expired and whose job was re-claimed by another worker gets `false` / `LeaseLost`, and its late writes change nothing.

**Failure outcomes** (`Queue::fail`):
- `Retryable`: `dead` once `attempts >= max_attempts` (default 3), otherwise queued at `now() + 5 s · 2^(attempts − 1)` (BullMQ's exponential 5000 ms).
- `Unrecoverable` (for example an import whose account is `not_found`): `failed` at once, with no retries.
- `Release { delay }`: queued at `now() + delay` **without using an attempt**: `attempts − 1`, even on the final attempt. Shutdown, a closed engine pool and an open circuit breaker use it, so none of them can dead-letter a job. This replaces BullMQ's `moveToDelayed` + `DelayedError` and fixes the TS behaviour where a SIGTERM on the final attempt dead-lettered the game.
- Every branch stores `last_error` (at most 2000 characters) and `error_code` (a release stores its reason, such as `shutdown` or `circuit_open`), and clears the lease.

**Dedupe among live jobs only.** `jobs_dedupe_live_uq` is a unique index on `(kind, dedupe_key)` restricted to `queued`, `running` and `paused`. Enqueue is `INSERT … ON CONFLICT … DO NOTHING`, which is race-free under concurrent producers and returns `None` when deduped. A finished job never blocks a new one, so BullMQ's "remove the retained completed job first, then add" dance (ADR 0029) is gone. `live_states(kind, keys)` answers "which of these 1000 games has a live job?" in one indexed query, replacing the pipelined `HMGET` liveness check.

**One live import per account.** Imports use the dedupe key `import:<account_id>` (`queue::import_key`) instead of BullMQ's per-run `resume-import-<runId>` ids, so a resume can never run beside the import it resumes. Analysis uses `analyse:<game_id>`.

**Recurring jobs instead of repeatable schedulers.** A job with `recur_every` goes back to `queued` at `now() + recur_every` with `attempts = 0` when it completes or fails, whatever the error (a release keeps its own delay). The reconciler is one recurring job with dedupe key `reconcile` every 300 s, so enqueueing it at every start is idempotent, replacing `upsertJobScheduler`.

**NOTIFY for wake-ups.** Enqueue runs `pg_notify('jobs', kind)` in the same statement, and a re-queue that is ready at once (a zero-delay release) does too. The notification is delivered on commit, and only when a row was inserted. Runners (Task 14) `LISTEN jobs` and fall back to polling every `idle_poll`, so a lost notification only costs latency.

**Every query has a timeout.** Each queue call runs in its own short transaction with `SET LOCAL statement_timeout` (default 30 s, the worker's setting), plus a client-side backstop of 5 s more for pool acquire and dead connections. A timed-out call returns `QueueError::Timeout` and is rolled back, so it never takes effect later: a bare client-side timeout would drop the future while the server finished the write.

**The `user_key` hook for R2.** Every job may carry `user_key` (the account id). R1 uses it to scope "queues idle" in the CLI to one account (`live_count(kinds, Some(account))`). `jobs_user_ready_idx (kind, user_key, priority, run_at)` and the `SelectStrategy` trait (one implementation, `PriorityFifo`) are the hook for spec §8d's per-account round robin in R2.

## Consequences

- Redis leaves the stack (Task 27); job state is transactional with the rest of the data and visible with plain SQL.
- Claims cost one indexed statement; at our volume (tens of jobs per second at most) Postgres is far from its limit. A busier deployment would partition by kind or batch claims.
- Terminal rows accumulate until retention runs (Task 14).
- Rejected: advisory-lock queues (no row-level state), `LISTEN`-only delivery (lost on disconnect), and keeping BullMQ next to a Rust worker (two sources of truth, and the final-attempt shutdown bug).

## Addendum (Task 14): runner, stall recovery and retention

**One runner per kind.** `Runner::spawn(queue, handler, cfg)` runs one tokio task that claims jobs of `handler.kind()` up to `cfg.concurrency` at once, so concurrency is per job kind. Defaults: lease 60 s, renew every 20 s, reap every 30 s, stall grace 25 s, idle poll 500 ms, backoff 5 s, no hard timeout (`fast_for_tests`: lease 4 s, renew 500 ms, reap 250 ms, stall grace 1 s).

**Wake-ups.** A listener task `LISTEN`s on `jobs` and wakes the loop only for its own kind (a `Notify` permit, so a notification that arrives while the loop is busy is not lost). The loop also polls every `idle_poll`, and the listener reconnects every second after an error, so a lost notification or a dropped listener costs latency only.

**`dequeue` is never raced.** It is not cancel-safe (dropping it after COMMIT orphans the claimed jobs until their leases expire), so it is never a `select!` branch; the stop flag is checked between calls.

**Leases and fencing.** A runner leases its jobs under its own owner, `<worker_id>/<kind>`, so runners of one process (one per kind, sharing the `worker_id`) never release each other's jobs. The `worker_id` is a prefix that must be unique per process: two replicas with the same `worker_id` would release each other's jobs on `force`. Each job renews its lease every `renew_every`. The renew runs as an in-flight future beside the handler, so the handler and its deadlines keep being polled, and it is bounded by the local lease deadline. That deadline starts from an instant taken *before* the claim and moves to "start of the last successful renew + lease", so it is never later than the database's `lease_until`; it runs on the monotonic clock, so a step of the database wall clock never shortens it. A forward step of the database clock does shorten the database lease by the step, which the two-pass reaper below absorbs. The job's `CancellationToken` fires when a renew returns `false` (another owner holds the lease), or when no renew has succeeded by `lease_until − CANCEL_GRACE` (1 s, at most half the lease; a hung renew is abandoned and rolls back). The handler then gets that grace and its result is dropped without a write, so a zombie handler never runs more than about the grace past its lease beside the next owner (Task 21 relies on this). The per-handler hard deadline cancels the token, gives 1 s, then fails the attempt as `Retryable { code: "timeout" }`. A panicking handler is a `Retryable { code: "panic" }` failure.

**Stall recovery.** Expiry is judged on the database clock, and a forward step of that clock makes a live lease look expired early. So every `reap_every` the runner lists its kind's leases that look expired (`Queue::expired_leases`: id, lease token, heartbeat) and recovers a lease only once it has looked expired, with the same token and heartbeat, for at least `stall_grace` on the monotonic clock (`Queue::recover_marked`). `stall_grace` must exceed `renew_every`, so a live owner always renews (changing its heartbeat) in between, whatever the step. `Queue::recover_expired(&[kind])` is the single-look primitive (tests, operations). Both are one statement: running rows with `lease_until < now()` (re-checked in the outer `WHERE`, so a renew that commits first wins; `FOR UPDATE SKIP LOCKED` in the inner scan) get `stalls + 1`. The re-queue keeps `run_at`, so the job is due at once in its old place: it neither falls behind a backlog nor, being immediate (`run_at <= created_at`), looks delayed after the clock steps back. With `stalls ≤ max_stalls` (2) they are queued at once with the attempt given back; beyond it they are `dead` with `error_code = 'stalled'` and `last_error = 'job stalled more than max_stalls times'`, keeping the attempt, and the reaping runner calls `Handler::on_dead` once. A job that keeps killing its worker therefore ends after three stalls. Recovered queued jobs are NOTIFYed.

**Shutdown.** `stop()` stops claiming and lets in-flight jobs finish (`wait_idle()`). `force()` cancels every handler, waits up to 1 s, aborts what is left and calls `Queue::release_owned("<worker_id>/<kind>")`, which re-queues this runner's own running jobs at once (keeping `run_at`) with `attempts − 1` and `error_code = 'shutdown'`. After `force`, any retryable failure (handler error, hard timeout or panic) is recorded as a release too, so a forced shutdown never uses an attempt; an `Unrecoverable` verdict stays `failed`, provided its write lands within the 1 s force grace (otherwise `release_owned` re-queues it without using an attempt). `shutdown(grace)` is `stop`, then `wait_idle` within `grace`, else `force`. All are re-entrant. `abort()` kills the tasks without touching the database (a simulated crash).

**Logging and metering.** Every failed attempt logs `job failed` at error level with `job_id`, `kind`, `attempt`, `max_attempts`, `error`, `error_code` and `outcome` (`retried`, `failed`, `dead`). Releases (for example an open breaker, unbounded by design) log a warning with the reason and delay and are counted, like stalls, lost leases and timeouts, in `RunnerHandle::stats()`. `on_dead` is called only for `dead` (final retryable attempt, or stalls), with the reason truncated to 2000 characters like `last_error`; an `Unrecoverable` failure is the handler's own decision and gets no callback.

**Retention.** `Queue::cleanup(&Retention)` deletes `done` rows older than 24 h, then those beyond the newest 1000 per kind, and `failed`/`dead` rows older than 7 days. Live rows are never touched. Each DELETE removes at most `batch_size` (1000) rows in its own transaction under the statement timeout, repeated until a batch comes back short. Scheduling it (for example from the reconciler or the worker binary) belongs to the worker (Task 22).

**Queue core follow-ups.** `enqueue` rejects `recur_every ≤ 1 s` (`QueueError::InvalidOptions`); a recurring job's failure resets `stalls` as its completion does; every `pg_notify` channel name is built from `JOBS_CHANNEL`.

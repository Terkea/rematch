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

**Claiming uses `FOR UPDATE SKIP LOCKED`** in one `WITH next AS (SELECT … ORDER BY priority, run_at, id FOR UPDATE SKIP LOCKED LIMIT n) UPDATE … RETURNING` statement. A claim for **one kind** (`kind = $1`) is an Index Scan on `jobs_ready_idx (kind, priority, run_at, id)` in claim order that stops after `n` rows, so it costs the same at any backlog (a unit test pins the plan, custom and generic, on 25 000 queued rows). A claim for several kinds (`kind = ANY($1)`) cannot be read in index order, and `LockRows` rules out a top-N sort, so it sorts every ready row of those kinds on each call: fine for a small backlog, but runners should claim one kind per call. `dequeue(&[&str], …)` picks the statement by the number of kinds. Concurrent claimers skip each other's locked rows instead of blocking or double-claiming, so each job is claimed exactly once (tested with 20 concurrent claimers over 300 jobs) without advisory locks or a coordinator. Lower `priority` runs first, then the earliest `run_at`. Rows with `run_at` in the future (beyond `created_at`) are delayed jobs.

**Attempts are counted at claim.** The claim increments `attempts`, so a worker that crashes mid-job has used the attempt, exactly like a BullMQ delivery. The stall reaper (Task 14) gives the attempt back when it re-queues an expired lease and counts a stall instead (`max_stalls = 2`, then `dead` with `error_code = stalled`).

**Lease-token fencing.** Each claim writes a fresh `lease_token` (`gen_random_uuid()`), `lease_owner` and `lease_until`. `renew`, `complete` and `fail` all carry `WHERE id = $1 AND lease_token = $2 AND state = 'running'`. A worker whose lease expired and whose job was re-claimed by another worker gets `false` / `LeaseLost`, and its late writes change nothing.

**Failure outcomes** (`Queue::fail`):
- `Retryable`: `dead` once `attempts >= max_attempts` (default 3), otherwise queued at `now() + 5 s · 2^(attempts − 1)` (BullMQ's exponential 5000 ms).
- `Unrecoverable` (for example an import whose account is `not_found`): `failed` at once, with no retries.
- `Release { delay }`: queued at `now() + delay`, or with its `run_at` unchanged when the delay is zero (so it keeps its place in the queue and an immediate job stays due across clock steps), **without using an attempt**: `attempts − 1`, even on the final attempt. Shutdown, a closed engine pool and an open circuit breaker use it, so none of them can dead-letter a job. This replaces BullMQ's `moveToDelayed` + `DelayedError` and fixes the TS behaviour where a SIGTERM on the final attempt dead-lettered the game.
- Every branch stores `last_error` (at most 2000 characters) and `error_code` (a release stores its reason, such as `shutdown` or `circuit_open`), and clears the lease.

**Dedupe among live jobs only.** `jobs_dedupe_live_uq` is a unique index on `(kind, dedupe_key)` restricted to `queued`, `running` and `paused`. Enqueue is `INSERT … ON CONFLICT … DO NOTHING`, which is race-free under concurrent producers and returns `None` when deduped. A finished job never blocks a new one, so BullMQ's "remove the retained completed job first, then add" dance (ADR 0029) is gone. `live_states(kind, keys)` answers "which of these 1000 games has a live job?" in one indexed query, replacing the pipelined `HMGET` liveness check.

**One live import per account.** Imports use the dedupe key `import:<account_id>` (`queue::import_key`) instead of BullMQ's per-run `resume-import-<runId>` ids, so a resume can never run beside the import it resumes. Analysis uses `analyse:<game_id>`.

**Recurring jobs instead of repeatable schedulers.** A job with `recur_every` goes back to `queued` at `now() + recur_every` with `attempts = 0` when it completes or fails, whatever the error (a release keeps its own delay). The reconciler is one recurring job with dedupe key `reconcile` every 300 s, so enqueueing it at every start is idempotent, replacing `upsertJobScheduler`.

**NOTIFY for wake-ups.** Enqueue runs `pg_notify('jobs', kind)` in the same statement, and a re-queue that is ready at once (a zero-delay release) does too. The notification is delivered on commit, and only when a row was inserted. Runners (Task 14) `LISTEN jobs` and fall back to polling every `idle_poll`, so a lost notification only costs latency.

**An empty claim does not mean "no work".** `dequeue` returning nothing means only that this statement found no row that was queued, unlocked and due by the database clock. Two things make that differ from "the queue is empty":
- `SKIP LOCKED` skips rows that other in-flight claims hold, so under contention a claim can come back empty while those rows are still `queued` (another claimer gets them a moment later).
- Being due is judged on the database's wall clock, which can step backwards (NTP stepping, VM or host resync). The Docker Desktop VM on Windows, Postgres's `clock_timestamp()` included, was measured stepping back about 0.9 s plus 1.8 s every 27 s (once 3.2 s). With a bare `run_at <= now()`, every job enqueued in the last 2.7 s looked delayed after such a step. That was the flaky 20-claimer test: all claimers got an empty claim at once and stopped with up to 216 of 300 jobs still queued.

The claim therefore treats a job as due when `run_at <= now() OR run_at <= created_at`. An immediate enqueue writes `run_at = created_at` (one transaction's `now()`), so new imports and analyses are claimable whatever the clock does, while an explicitly delayed job (`run_at > created_at`) still waits for `now()`. A zero-delay release keeps `run_at` (and so the `created_at` rule). Re-queued rows with a delay (retry backoff, a delayed release, a recurring run) are written with `now() + delay` and stay clock-governed: a backward step of `d` delays them by up to `d`, a bounded delay, never a lost job. The reverse also holds: such a row becomes due *early*, at once, when the clock stepped back by more than the job's age plus its delay before the re-queue, because `now() + delay` then lands at or before `created_at`. That needs a step larger than any we measured, and costs one early run.

The `created_at` rule also covers a race that needs no clock step. `now()` is the transaction's start time, so a claim whose transaction began before an enqueue's, but whose statement ran after that enqueue committed, saw the new row with `run_at` later than its own `now()` and skipped it (one possible cause of the "just-enqueued job is not due" seen in Task 21, besides a clock step). With `run_at <= created_at` the row is due.

So callers must never treat an empty claim as done or idle-forever. The runner (Task 14) goes back to waiting for the next `jobs` notification **or** the `idle_poll` timer, whichever comes first, and always re-claims on that timer even without a notification: a notification that arrived while the jobs looked delayed has already been used. "Is the queue drained?" is answered by `live_count`, never by an empty claim. The concurrency test stops only when the database shows no queued or running job, and retries an empty claim in the meantime.

**Every query has a timeout.** Each queue call runs in its own short transaction with `SET LOCAL statement_timeout` (default 30 s, the worker's setting), plus a client-side backstop of 5 s more for pool acquire and dead connections. A timed-out call returns `QueueError::Timeout` and is rolled back, so it never takes effect later: a bare client-side timeout would drop the future while the server finished the write.

**The `user_key` hook for R2.** Every job may carry `user_key` (the account id). R1 uses it to scope "queues idle" in the CLI to one account (`live_count(kinds, Some(account))`). `jobs_user_ready_idx (kind, user_key, priority, run_at)` and the `SelectStrategy` trait (one implementation, `PriorityFifo`) are the hook for spec §8d's per-account round robin in R2.

## Consequences

- Redis leaves the stack (Task 27); job state is transactional with the rest of the data and visible with plain SQL.
- A single-kind claim costs one index-ordered statement at any backlog; a multi-kind claim sorts the ready rows of its kinds. At our volume (tens of jobs per second at most) Postgres is far from its limit. A busier deployment would partition by kind or batch claims.
- Terminal rows accumulate until retention runs (Task 14).
- Rejected: advisory-lock queues (no row-level state), `LISTEN`-only delivery (lost on disconnect), and keeping BullMQ next to a Rust worker (two sources of truth, and the final-attempt shutdown bug).

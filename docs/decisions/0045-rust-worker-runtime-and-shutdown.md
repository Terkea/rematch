# 0045. Rust worker runtime image and graceful shutdown

Date: 2026-11-21. Status: accepted.

## Context

R1 Task 22 turns the Rust crates into the deployable worker. The worker must survive a deploy
(SIGTERM) without losing or burning work, refuse to analyse with the wrong engine, and ship as a
small, locked-down image.

## Decision

- **One process, one runner per job kind:** import (concurrency 2, no hard timeout: the handler
  owns its 30-minute idle deadline), analyse (concurrency = engine pool size, hard timeout 11 min
  over the handler's own 10-minute deadline) and reconcile (concurrency 1, hard timeout 5 min).
  The pool size is `WORKER_POOL_SIZE` (passed through compose; `.env.example` sets 8 for a
  desktop), else `min(cores - 2, 8)` and at least 1, so a backfill cannot saturate the
  machine. The database pool has `pool_size + 8` connections (budget in `worker/src/start.rs`),
  and every connection asks the server for TCP keepalives (60/10/3 s).
- **Boot order:** check that `STOCKFISH_PATH` is Stockfish 19.x (the `sf19-n300k-v1` protocol is
  pinned to it; anything else is a typed `StartError` and exit 1), adopt a Drizzle database and
  migrate it, connect, start the engines and the runners, schedule the reconcile. A failed step
  closes what the earlier ones created.
- **Shutdown** (25 s grace; compose `stop_grace_period: 30s`): stop claiming, drain for 60 % of the
  grace, then close the engine pool (in-flight analyses put their game back to `pending` and
  release their job without using an attempt, even on the final attempt), drain until 90 %, then
  force the runners: the 90 %-100 % window bounds the wait for their handlers (cancel, then
  abort, even a handler that ignores cancellation), and then `release_owned` always runs to
  completion, bounded only by the queue's query timeout and the hard deadline below, not by the
  window. (Amended 2026-11-22: the release used to get only what was left of the window, at
  least 50 ms. On a loaded host that write took longer, was dropped, and left the job leased
  for a whole lease and charged a stall.) Then close the engines and the database pool.
  Every step is bounded by the remaining budget (at least 100 ms); exit code 1 only if a close
  failed or timed out. That 100 ms floor lets the steps overrun the grace slightly, so the whole
  shutdown also has a **hard deadline** at `grace + hard_after` (25 s + 3 s = 28 s, before the
  30 s SIGKILL): whatever is still running is dropped, the code is 1 and the process exits; jobs
  not yet released are reclaimed when their lease expires. SIGTERM and SIGINT start it once;
  later signals are logged and ignored.
- **Config is safe to log:** `WorkerEnv` has a hand-written `Debug` that shows the database URL
  with its password replaced by `***` and the Lichess token only as `Some("***")`.
- **Batched enqueue:** `Queue::enqueue_many` inserts many jobs of one kind in one statement with
  the same columns and live-only dedupe rule as `enqueue`, under the queue's statement timeout,
  with one NOTIFY per batch. The reconciler uses it with the analyse job's own options.
- **`on_dead` generation fence:** a dead-lettered analyse job does not mark its game failed while
  the game is `analysing` with a heartbeat younger than three heartbeat periods (30 s): that is a
  live run, possibly of a newer generation. A stalled run is always older than that (lease plus
  stall grace, 85 s).
- **Import runs are status-fenced:** `finish` and `fail` only touch a run that is still `running`,
  so a run the reconciler closed for a stale heartbeat stays closed.
- **Runtime image** (`backend/Dockerfile` target `worker`): cargo-chef planner and builder stages,
  then `debian:bookworm-slim` pinned by digest with `ca-certificates`, the checksum-pinned
  Stockfish 19 and the `worker` and `cli` binaries; `USER 10001`, no ports, no `CONTACT_EMAIL`
  (ADR 0043). Logs are JSON lines on stdout with `"service":"worker"`; `LOG_LEVEL=silent` turns
  them off, otherwise `RUST_LOG` filters.

## Consequences

- A deploy never burns a *job* attempt: a job interrupted by shutdown is released and re-run by
  the next worker. It does still count against the game's `games.analysis_attempts`, which every
  claim increments (the attempt fence, ADR 0035): an interrupted run is one of the game's
  `MAX_ANALYSIS_ATTEMPTS` (6) after which the reconciler fails it. Frequent restarts mid-analysis
  can therefore exhaust a game; deferred to T23/T24.
- A hard-timed-out analysis whose handler never winds down leaves its game `analysing`; the
  reconciler's stale sweep (10 min) resets or fails it.
- Writes of an import are still not routed through its lock connection: if that connection dies
  mid-import, a re-claimed job can overlap the old handler (T21 limitation, deferred to T23/T24).
- Bumping the Debian base means updating the digest by hand.

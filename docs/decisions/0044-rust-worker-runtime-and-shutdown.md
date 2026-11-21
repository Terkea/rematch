# 0044. Rust worker runtime image and graceful shutdown

Date: 2026-11-21. Status: accepted.

## Context

R1 Task 22 turns the Rust crates into the deployable worker. The worker must survive a deploy
(SIGTERM) without losing or burning work, refuse to analyse with the wrong engine, and ship as a
small, locked-down image.

## Decision

- **One process, one runner per job kind:** import (concurrency 2, no hard timeout: the handler
  owns its 30-minute idle deadline), analyse (concurrency = engine pool size, hard timeout 11 min
  over the handler's own 10-minute deadline) and reconcile (concurrency 1, hard timeout 5 min).
  The pool size defaults to `max(1, cores - 1)` (`WORKER_POOL_SIZE`). The database pool has
  `pool_size + 8` connections (budget in `worker/src/start.rs`), and every connection asks the
  server for TCP keepalives (60/10/3 s).
- **Boot order:** check that `STOCKFISH_PATH` is Stockfish 19.x (the `sf19-n300k-v1` protocol is
  pinned to it; anything else is a typed `StartError` and exit 1), adopt a Drizzle database and
  migrate it, connect, start the engines and the runners, schedule the reconcile. A failed step
  closes what the earlier ones created.
- **Shutdown** (25 s grace; compose `stop_grace_period: 30s`): stop claiming, drain for 60 % of the
  grace, then close the engine pool (in-flight analyses put their game back to `pending` and
  release their job without using an attempt, even on the final attempt), drain until 90 %, then
  force the runners (cancel and `release_owned`), then close the engines and the database pool.
  Every step is bounded by the remaining budget (at least 100 ms); exit code 1 only if a close
  failed or timed out. SIGTERM and SIGINT start it once; later signals are logged and ignored.
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

- A deploy never burns an attempt: a job interrupted by shutdown is re-run by the next worker.
- A hard-timed-out analysis whose handler never winds down leaves its game `analysing`; the
  reconciler's stale sweep (10 min) resets or fails it.
- Writes of an import are still not routed through its lock connection: if that connection dies
  mid-import, a re-claimed job can overlap the old handler (T21 limitation, still open).
- Bumping the Debian base means updating the digest by hand.

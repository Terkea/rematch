# 0035. Analysis generations and job outcome rules

Date: 2026-11-21
Status: Accepted. Implements 0030 and 0032; changes the job semantics of 0028.

## Context

The TS worker analysed a game once per protocol: `analysis_status = 'done' AND analysis_protocol = ANALYSIS_PROTOCOL` returned early, and `analysis_attempts` only ever grew. A user who asked for a re-analysis (a "force") could not bypass "already done", and a game that had used up its attempts stayed capped for ever. On shutdown, a BullMQ job on its final attempt was dead-lettered, so a deploy could fail games that were never at fault. BullMQ ids (`resume-import-<runId>`) allowed several live imports per account, and the circuit breaker created a new `import_runs` row on every re-delay. R1 replaces BullMQ with the Postgres queue (ADR 0032), so these rules are restated here.

## Decision

- **Generations.** `games.analysis_generation integer NOT NULL DEFAULT 0` counts re-analysis requests. `games.analysed_generation integer` is the generation the stored analysis belongs to. The migration sets `analysed_generation = 0` for games already `done`. A game is finished only when `analysis_status = 'done' AND analysis_protocol = ANALYSIS_PROTOCOL AND analysed_generation = analysis_generation`. Only then does the handler return `done` without writes.
- **`request_reanalysis(pool, queue, game_id)`** sets `analysis_generation + 1`, `analysis_attempts = 0`, status `pending`, `analysis_error = NULL` and `pending_since = now()` where the game is not `analysing`, then enqueues it (`analyse:<game_id>`, so a live job is reused). It returns `false` for a missing game or one mid-analysis. Resetting the attempts per generation means a capped game (`analysis_attempts >= 6`, indexed by `games_attempt_cap_idx` for the reconciler) can be retried by the user.
- **The fence.** Each attempt claims the game (`analysis_attempts + 1`, `RETURNING analysis_attempts, analysis_generation`). Every later write (heartbeat, skipped, done, pending, failed) carries `AND analysis_attempts = <claimed> AND analysis_generation = <claimed>`. The generation is part of the fence because `request_reanalysis` resets the attempt counter. Without it, a stale run of generation 0 holding attempt 1 could complete generation 1 after the reset.
- **Job outcomes of the analyse handler.**
  - Game row missing: `Unrecoverable` (the only unrecoverable case).
  - Unsupported variant or fewer than 2 plies: a fenced `skipped`. Any other PGN error is the game's own (poison) and is retried like an engine error.
  - The fence misses: `Ok(superseded)`, nothing written.
  - **Shutdown or a closed engine pool releases the job** (`Release { delay: 0, reason: "shutdown" }`) on any attempt, after a fenced write of `pending` with `pending_since = now()` and the error message. A cancelled job token (shutdown, lost lease, hard timeout) counts as a shutdown. TS dead-lettered such a job on its final attempt.
  - Deadline overrun (10 minutes): `Retryable { code: "timeout" }`, handled like any other error below.
  - Any other error: a non-final attempt writes `pending` (with `pending_since`). The final attempt writes `failed` (error at most 2000 characters) and publishes `job.failed {kind: "analyse-game", gameId, reason}` only if the fenced write hit the row. Both return `Retryable`.
  - `on_dead` (the runner dead-letters the job, including a stalled one) writes `failed` only where the status is not already `done`, `failed` or `skipped`, and publishes `job.failed` only if a row came back. It therefore never duplicates the handler's own final-attempt event.
- **Rules owned by Task 21 (import), recorded here.** A user-level `not_found` (the chess.com archive list, the Lichess user stream) is unrecoverable: the run fails at once with `error_code = not_found`, and `job.failed` is published. An open or half-open breaker re-delays the job without using an attempt, floored at `max(5 s, probe timeout)` plus jitter, and **reuses the account's `paused` run row** (found through `import_runs_paused_idx`) instead of inserting a new one. `paused` is non-terminal.
- **One live import per account** (dedupe key `import:<account_id>`, ADR 0032), and one live analyse job per game (`analyse:<game_id>`, `user_key = account_id`, `max_attempts = 3`).

## Consequences

- A deploy never fails a game: its job is released and a fresh worker completes it (Task 22 tests this end to end).
- A re-analysis is cheap to request and safe against stale runs. The eval cache is keyed by `(fen_key, protocol)`, so a re-analysis at the same protocol is served mostly from the cache.
- A job that is released on every attempt (a pool that is always closed) is re-queued without bound. The worker's shutdown order (stop claiming before closing the pool, Task 22) keeps that from looping.

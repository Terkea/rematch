# 0036. Per-stage timing metrics

Date: 2026-11-21
Status: Accepted. Implements the efficiency requirement of 0030.

## Context

The owner wants the backend to be measurably efficient. The TS worker logged only the outcome of a job, so nobody could tell whether a slow analysis waited in the queue, in the cache, in Stockfish or in the persist transaction. R1 needs numbers per job that the benchmark (Task 26), the R2 ETA estimate and an operator can query with SQL.

## Decision

- **One `job_metrics` row per analyse attempt that claimed the game**, written after the commit. It is best effort: an error is logged with `tracing::warn!` and never raised. The same fields, plus a few that have no column yet, go into one `tracing::info!(target: "metrics", ...)` event. Failed, released and superseded attempts are recorded too, with the partial statistics `evaluate_game` returns on failure. A duplicate delivery of finished work (the early return) records nothing.
- **Columns and their exact meaning.** All durations are milliseconds (`double precision`). `NULL` means the stage did not run.

| Column | Meaning |
|---|---|
| `job_id`, `kind`, `game_id`, `account_id` | The queue job, `analyse-game`, the game and its account. |
| `queue_wait_ms` | `job.started_at - max(job.run_at, job.created_at)`: how long the job was ready but not running. |
| `parse_ms` | `parse_pgn` (SAN replay, FENs, clocks). |
| `cache_ms` | Sum of the time spent inside eval-cache calls (one `get_many`, the Lichess `put_many`, and one `put_many` per engine result). Workers overlap, so it can exceed the elapsed time. |
| `engine_wall_ms` | **Sum** over completed searches of the engine-reported wall time. With 4 parallel requests it is about 4 times the elapsed engine stage. |
| `engine_cpu_ms` | **Sum** over completed searches of Stockfish's CPU time: the change of `utime + stime` in `/proc/<pid>/stat` (100 ticks per second) around each search. `NULL` when the engine reports none (not Linux, or a fake engine). |
| `persist_ms` | The persist transaction: the fenced `done` claim, `DELETE FROM moves`, the `UNNEST` inserts in chunks of 500, and the commit. |
| `total_ms` | The whole handler, from entry to just before the metrics are written. It includes the `game.analysed` publish. |
| `positions` | Positions in the game, `plies + 1`, counted per occurrence. |
| `engine_positions` | Distinct positions whose engine search completed. |
| `cache_hits`, `lichess_used` | Positions served by the eval cache and by the game's Lichess evals. |
| `created_at` | When the row was written. |

- **Logged but not yet persisted:** the real elapsed span of the engine stage (`engine_stage_ms`, first worker start to last worker end), the real span of the whole `evaluate_game` call (`evaluate_ms`), and the attempt's `outcome`. This migration fixes the `job_metrics` columns because R2 builds on it. R2 Task 3 adds `job_metrics.stages jsonb`, which then carries these spans and the stages of every other job kind.
- **Retention: 30 days.** The reconciler prunes `job_metrics` rows older than 30 days, using `job_metrics_created_idx`.
- **Benchmark method (Task 26).** Analyse the same recorded game set with a cold and with a warm cache, then read the run's `job_metrics`. Report per-game medians and p95 of every stage, engine CPU per position (`engine_cpu_ms / engine_positions`), parallel efficiency (`engine_wall_ms / (engine_stage_ms * concurrency)`) and throughput in games per second, compared with the TS worker on the same machine.

## Consequences

- One query attributes any slow analysis to a stage, and the same rows feed R2's ETA (`games_per_sec` over the last 10 minutes).
- One extra insert per attempt, which is small next to the moves inserts of the same job.

# ADR-R2-24. The analyse-game hook: per-game features at analysis time

Date: 2026-11-22
Status: Accepted. Placeholder number; the controller renumbers R2 ADRs before merging. Task 31 (`backend/crates/analysis/src/hook.rs`, `backend/crates/profile/src/{analyse_features,hook_impl,load,requeue}.rs`). Related: ADR 0035 (analyse job fence), ADR-R2-13 (Maia-only difficulty), ADR-R2-14 (enrichment protocol), ADR-R2-15 (preview pass).

## Context

A player's profile is built from per-game rows: enriched `move_features`, `game_metrics`, `game_openings` and `weakness_series_point`. They must be written when a game's analysis finishes, by code the R1 `analysis` crate cannot depend on (`profile` depends on `analysis`, not the reverse). The analysis itself must never be lost to a failure of this extra work, re-analysis must replace the rows, and R1's fence, release and generation rules must still hold.

## Decision

### The seam

- `analysis::hook::{AnalyseHook, PreparedHook}`. `AnalyseGameDeps.hook = None` (the default) keeps R1's behaviour exactly; R1 code never touches the R2 columns or tables then.
- The job calls `prepare` after `build_move_records`, while it still owns the game. Heartbeats keep running during `prepare` (fenced, every `heartbeat_every`): enrichment runs engines, and a silent run would look stale to the reconciler. A missed heartbeat cancels the run's token and the attempt ends `Superseded`.
- `persist` runs in the moves transaction, after the fenced `done` update and the moves insert, **inside a savepoint**. `after_commit` runs after the `game.analysed` publish.

### Failure rules

| Outcome | Game | Hook rows |
|---|---|---|
| `HookError::Cancelled` (shutdown, lost lease) | `pending`, job released (`shutdown`), as R1 | none |
| `HookError::Cancelled` after the deadline | R1's retryable `timeout` | none |
| `HookError::Failed` | `done` with its moves | none: every earlier hook row is deleted (`clear_hook_rows`), `enrichment_protocol = NULL`, preview deleted |
| `persist` error | `done` with its moves (savepoint rolled back) | none: as for `Failed` |
| superseded | nothing written (fence) | none, no `after_commit` |

`enrichment_protocol = NULL` is what `requeue_unenriched` looks for, so the hook is always redone later. The earlier analysis's `move_features`, `game_metrics`, `game_openings` and `weakness_series_point` rows are deleted in the same transaction (fix round 1, I1): they hang off `games`, not `moves`, so nothing else removes them, and the loaders would serve them as current. Whenever the game reaches `done` with a hook configured, its `game_previews` row is deleted in the same transaction as the done transition (R-13, T28 review). The final `game.analysed` is R1's payload, `{gameId, accuracy}`, never `preview: true`.

### What `ProfileHook` writes

- `enrich_game` for the player's colour only (Task 25), then `compute_game_features`:
  - book exit from the explorer of the player's band (with T18's band fallback); a read error or no theory evidence means no book exit, never a failure;
  - phases recomputed with `detect_phases(fens, book_exit_ply.unwrap_or(20))`;
  - the opening from the CC0 classification (`classify_line`). `game_openings.line_name`/`eco` fall back to the platform's values only when nothing is classified; `line_key`/`line_kind` stay NULL then (no series is keyed on a platform name);
  - the counted variants of assembly, observation, strengths and metrics; every skipped-input count goes to `job_metrics.stages` of the `enrich-game` row (T6, T8 and T9 hand-offs), next to Task 25's stage timings and `features_ms`.
- Rows are replaced, never appended: `move_features` (delete + insert), `game_metrics` and `game_openings` (upsert), the game's `weakness_series_point` rows (delete + insert). A re-analysis gives the same row counts.
- `after_commit` enqueues `build-profile` deduped on `profile:<account>:<tc>` with `run_at = now() + 30 s`, so a batch of games builds once.

### Requeue and the pending-Maia sweep

- `requeue_unenriched` sets `done` games with `enrichment_protocol IS DISTINCT FROM 'enr-v1'` to `pending` at tier 2 (scheduler priority 20/21, beta plan) with `analysis_attempts = 0`, and enqueues them. It pages by keyset on `games_pkey` with an unfiltered window (as R1's reconciler), pinned by a plan guard. Main evals come from the cache.
- Difficulty is Maia only. With Maia down the moves stay `pending_maia`. `run_difficulty_sweep` runs Task 25's sweep and recomputes the metrics, opening and series rows of every game it changed, from the stored analysis (cached evals) and stored enrichment, without rewriting `move_features`. The write happens under `SELECT … FOR UPDATE` of the game and only while it is still `done` at the same generation and protocols; a game being re-analysed is skipped (its re-analysis recomputes it). - **Durable recompute (fix round 1, I3).** Migration `20261121020400_games_profile_stale.sql` adds `games.profile_stale_at` and a statement-level trigger on `move_features`: any row going to `difficulty_status = 'ok'` marks its game in the same statement, so in the same transaction as the difficulty write. `run_difficulty_sweep` recomputes the marked games (oldest first, 500 per run, partial index `games_profile_stale_idx`) and clears each mark in the recompute's transaction, only if it was not set again meanwhile. A cancel, a DB error or a crash between the difficulty write and the recompute leaves the mark, and the next sweep recomputes the game. The hook clears the mark (it recomputes everything); so does a recompute that finds the game no longer current.
- A recompute that cannot evaluate or decode the game deletes its derived rows and clears `enrichment_protocol` (no stale rows served), so `requeue_unenriched` redoes it; a DB error keeps the mark.
- The sweep runs as the recurring job `difficulty-sweep` (every 10 minutes, deduped; a sweep with Maia down costs one `/healthz` probe). The worker registers its runner and calls `ensure_difficulty_sweep_scheduled` and `requeue_unenriched` at start (Task 35).

## Consequences

- An enrichment outage degrades to "analysed, not yet profiled", never to a lost or failed analysis.
- A done game can be briefly `pending` again after a requeue; its moves and evals are rewritten identically from the cache.
- `build-profile` is deduped onto a running job too: a game that finishes while its account's build is running is only included by a later build (the next analysed game or sweep recompute enqueues one). Task 33 owns closing that gap (for example a re-enqueue when the build sees newer rows).
- `moves.phase` keeps R1's phases (opening until ply 20). The features use phases recomputed from the book exit, so an exemplar's `moves.phase` (`load_exemplar_moves`) can differ from the phase its features used. Readers that need the feature phase recompute it with `detect_phases`.

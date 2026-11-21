# 0028. BullMQ job ids and analysis persistence

Date: 2026-11-21
Status: Accepted

## Context

Task 12 adds the `import-account` and `analyse-game` queues. Importing must be idempotent, an analysis must be retryable without duplicate rows, and a game must never be enqueued before its row is committed.

## Decision

- Analyse jobs use the deterministic job id `analyse-<gameId>`, so duplicate enqueues are no-ops. The planned `analyse:<gameId>` is not possible: BullMQ rejects custom ids that contain ":" (unless they have exactly three parts).
- `handleImportAccount` upserts each game with `analysisStatus: "pending"` and calls `enqueueAnalyse` only after that write has committed, and only for newly inserted rows. Games whose account username matches neither player are skipped and counted.
- `handleAnalyseGame` sets `analysing` on every attempt, replaces the game's `moves` and marks it `done` (protocol, both accuracies) in one transaction, and on any error stores a `failed` status with the message (at most 2000 characters) and rethrows so BullMQ retries. Unsupported variants and games under 2 plies become `skipped` without calling the engine.
- Queue defaults: 3 attempts, exponential backoff starting at 5000 ms. Workers share one ioredis connection created with `maxRetriesPerRequest: null`.
- `msgpackr-extract` (an optional native accelerator pulled in by BullMQ) is set to `false` in `allowBuilds`, so the frozen install passes.

## Alternatives considered

Encoding the job id with another separator and parsing it back (not needed, the payload carries the id); enqueueing inside the DB transaction (a worker could pick the job up before commit).

## Consequences

A game inserted but not enqueued (crash between the two steps) stays `pending` until the reconciler of Task 12b re-enqueues it. Finished jobs keep their id until removed, so a later re-enqueue of the same game is a no-op while the job is retained.

/**
 * The worker jobs "Sync now" and "Retry" enqueue, with the values Rust uses
 * (`backend/crates/queue/src/kinds.rs`, `analysis/src/jobs/{import,analyse}.rs`).
 * `test/sync/contract.test.ts` pins each of them against that source.
 */
export const IMPORT_JOB = {
  kind: "import-account",
  /** `EnqueueOptions::default()`: `enqueue_import` sets no priority. */
  priority: 100,
  maxAttempts: 3,
  key: (accountId: string) => `import:${accountId}`,
  /**
   * Rust `enqueue_incremental_import` (Task S, ADR 0076): only games newer than the last import,
   * no `max`. The recurring sync enqueues the same job, so one dedupe key covers both.
   */
  payload: (accountId: string) => ({ accountId, incremental: true }),
} as const;

export const ANALYSE_JOB = {
  kind: "analyse-game",
  /** The reconciler re-enqueues at `coalesce(analysis_priority, 100)`. */
  defaultPriority: 100,
  maxAttempts: 3,
  key: (gameId: string) => `analyse:${gameId}`,
  payload: (gameId: string) => ({ gameId }),
} as const;

export const PROFILE_KIND = "build-profile";

/** "Sync now" is allowed once per this long per account (coordinator ruling, Task P2). */
export const SYNC_COOLDOWN_MS = 2 * 60_000;

/** A failed game is retried at most once per this long. */
export const RETRY_COOLDOWN_MS = 2 * 60_000;

/** The most failed games one "Retry" re-enqueues. */
export const RETRY_LIMIT = 500;

/** `pg_advisory_xact_lock` class for sync requests (web uses 30030–30032, R3 30001–30020). */
export const SYNC_LOCK_CLASS = 30040;

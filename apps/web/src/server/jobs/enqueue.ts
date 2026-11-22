import type { Db } from "@ci/db";
import { sql } from "drizzle-orm";

/** One job for R1's Postgres queue (`jobs`, ADR 0032). `userKey` is the account id (fairness). */
export type NewJob = {
  kind: string;
  payload: unknown;
  dedupeKey: string;
  userKey: string;
  priority: number;
  maxAttempts: number;
};

/**
 * The job kinds the web enqueues, with the values of `backend/crates/ai/src/jobs/kinds.rs`
 * (`KIND_*`, `*_PRIORITY`, `*_MAX_ATTEMPTS`, `report_key`, `chat_key` and the camelCase payload
 * structs). `test/coaching/enqueue.test.ts` pins them against that file.
 */
export const JOB = {
  report: {
    kind: "generate-report",
    priority: 50,
    maxAttempts: 3,
    key: (runId: string) => `report:${runId}`,
    payload: (runId: string) => ({ reportRunId: runId }),
  },
  chat: {
    kind: "coach-chat",
    priority: 10,
    maxAttempts: 2,
    key: (messageId: string) => `chat:${messageId}`,
    payload: (messageId: string) => ({ messageId }),
  },
} as const;

/**
 * R1's `ENQUEUE_SQL` (`backend/crates/queue/src/queue.rs`) with `run_at = now()` and no
 * recurrence: the same columns, the live-only dedupe conflict rule and `pg_notify('jobs', kind)`.
 * Runs on `tx`, so a caller's transaction commits the job with its rows (and the notify is
 * delivered at commit). Null when a live (`queued`, `running`, `paused`) job holds the key.
 */
export async function enqueueJob(tx: Db, job: NewJob): Promise<number | null> {
  const rows = (await tx.execute(sql`
    WITH ins AS (
      INSERT INTO jobs (kind, payload, dedupe_key, user_key, priority, run_at, max_attempts, recur_every)
      VALUES (${job.kind}, ${JSON.stringify(job.payload)}::jsonb, ${job.dedupeKey}, ${job.userKey}::uuid,
        ${job.priority}::int2, now(), ${job.maxAttempts}::int4, NULL)
      ON CONFLICT (kind, dedupe_key) WHERE dedupe_key IS NOT NULL AND state IN ('queued','running','paused')
      DO NOTHING RETURNING id)
    SELECT id::text AS id, pg_notify('jobs', ${job.kind}) IS NULL AS not_notified FROM ins
  `)) as unknown as { id: string }[];
  const id = rows[0]?.id;
  return id == null ? null : Number(id);
}

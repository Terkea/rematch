import { type Db, games, importRuns, userEvents } from "@ci/db";
import { and, eq, inArray, lt, sql } from "drizzle-orm";
import { log as defaultLog, type Logger } from "../log.js";
import type { Queues } from "../queue/queues.js";

export interface ReconcileDeps {
  db: Db;
  queues: Pick<Queues, "enqueueAnalyse" | "hasLiveAnalyseJob" | "enqueueImport">;
  now?: () => Date;
  log?: Logger;
}

export interface ReconcileResult {
  requeuedPending: number;
  resetStale: number;
  resumedImports: number;
}

const EVENT_RETENTION_MS = 7 * 24 * 3600 * 1000;

/** BullMQ job id for resuming one stale import run, so repeated sweeps cannot double-enqueue. */
export const resumeImportJobId = (runId: string): string => `resume-import-${runId}`;

/**
 * Idempotent repair sweep (spec §13a). Postgres holds the state; this rebuilds what the queue
 * should contain:
 * 1. `pending` games with no live BullMQ job are re-enqueued (lost enqueue, Redis data loss).
 * 2. `analysing` games whose heartbeat is older than `staleAfterMs` go back to `pending` and are
 *    re-enqueued (a job that stopped making progress without its worker dying).
 * 3. `running` import runs with a stale heartbeat are closed as failed and their account is
 *    re-imported; the import resumes from the account's cursor.
 * Also prunes user events older than 7 days.
 */
export async function reconcile(
  deps: ReconcileDeps,
  opts: { staleAfterMs?: number } = {},
): Promise<ReconcileResult> {
  const { db, queues } = deps;
  const log = deps.log ?? defaultLog;
  const now = (deps.now ?? (() => new Date()))();
  const cutoff = new Date(now.getTime() - (opts.staleAfterMs ?? 600_000));

  // 1. Lost pending games. Runs first, so games reset in step 2 are counted only there.
  let requeuedPending = 0;
  const pending = await db
    .select({ id: games.id })
    .from(games)
    .where(eq(games.analysisStatus, "pending"));
  for (const { id } of pending) {
    if (await queues.hasLiveAnalyseJob(id)) continue;
    if (await queues.enqueueAnalyse(id)) requeuedPending++;
  }

  // 2. Stale analyses. A null heartbeat falls back to the start time (or counts as stale).
  const stale = await db
    .update(games)
    .set({ analysisStatus: "pending" })
    .where(
      and(
        eq(games.analysisStatus, "analysing"),
        sql`coalesce(${games.heartbeatAt}, ${games.analysisStartedAt}, 'epoch'::timestamptz) < ${cutoff.toISOString()}::timestamptz`,
      ),
    )
    .returning({ id: games.id });
  for (const { id } of stale) {
    // If the hung job is still active in BullMQ this is a no-op; the game stays pending and
    // the next sweep enqueues it once that job ends.
    await queues.enqueueAnalyse(id);
  }

  // 3. Stale import runs.
  const staleRuns = await db
    .update(importRuns)
    .set({
      status: "failed",
      finishedAt: now,
      lastError: "stale heartbeat: resumed by the reconciler",
    })
    .where(and(eq(importRuns.status, "running"), lt(importRuns.heartbeatAt, cutoff)))
    .returning({ id: importRuns.id, accountId: importRuns.accountId });
  for (const run of staleRuns) {
    await queues.enqueueImport({ accountId: run.accountId }, { jobId: resumeImportJobId(run.id) });
  }

  const pruned = await db
    .delete(userEvents)
    .where(lt(userEvents.createdAt, new Date(now.getTime() - EVENT_RETENTION_MS)))
    .returning({ id: userEvents.id });

  const result = {
    requeuedPending,
    resetStale: stale.length,
    resumedImports: staleRuns.length,
  };
  const counts = await db
    .select({ status: games.analysisStatus, n: sql<number>`count(*)::int` })
    .from(games)
    .where(inArray(games.analysisStatus, ["pending", "analysing", "failed"]))
    .groupBy(games.analysisStatus);
  log.info(
    {
      ...result,
      prunedEvents: pruned.length,
      games: Object.fromEntries(counts.map((c) => [c.status, c.n])),
    },
    "reconcile sweep",
  );
  return result;
}

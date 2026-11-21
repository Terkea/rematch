import { type Db, games, importRuns, userEvents } from "@ci/db";
import { and, asc, eq, gt, gte, inArray, lt, sql } from "drizzle-orm";
import type { PublishFn } from "../events/publish.js";
import { log as defaultLog, errorMessage, type Logger } from "../log.js";
import type { Queues } from "../queue/queues.js";

export interface ReconcileDeps {
  db: Db;
  queues: Pick<Queues, "enqueueAnalyse" | "analyseLiveness" | "analyseJobState" | "enqueueImport">;
  /** `job.failed` for games given up on. */
  publish?: PublishFn;
  now?: () => Date;
  log?: Logger;
}

export interface ReconcileOptions {
  /** Heartbeat age after which `analysing` games and `running` imports count as stale (10 min). */
  staleAfterMs?: number;
  /** Games pending for less than this are left alone: their enqueue may be in flight (2 min). */
  pendingGraceMs?: number;
  /** Keyset page size of the pending sweep (1000). */
  pageSize?: number;
}

export interface ReconcileResult {
  requeuedPending: number;
  resetStale: number;
  resumedImports: number;
  /** Stale heartbeat but the BullMQ job is still active: left alone (its deadline ends it). */
  hungActive: number;
  /** Games marked failed because they used up MAX_ANALYSIS_ATTEMPTS. */
  exhausted: number;
}

/**
 * Attempt counter cap across every path (BullMQ retries, stalled pickups, reconciler re-enqueues).
 * Twice BullMQ's 3 attempts, so a game is only given up on after two full rounds.
 */
export const MAX_ANALYSIS_ATTEMPTS = 6;
export const EXHAUSTED_REASON = "exceeded max attempts (stalled or crashed worker)";

const EVENT_RETENTION_MS = 7 * 24 * 3600 * 1000;

/** BullMQ job id for resuming one stale import run, so repeated sweeps cannot double-enqueue. */
export const resumeImportJobId = (runId: string): string => `resume-import-${runId}`;

/**
 * Idempotent repair sweep (spec §13a). Postgres holds the state; this rebuilds what the queue
 * should contain:
 * 1. Games pending for over `pendingGraceMs` with no live BullMQ job are re-enqueued, in keyset
 *    pages with one pipelined liveness check per page.
 * 2. `analysing` games whose heartbeat is older than `staleAfterMs` go back to `pending` and are
 *    re-enqueued, unless their job is still active (counted as `hungActive`, left alone).
 * 3. In both sweeps a game at MAX_ANALYSIS_ATTEMPTS is marked failed instead (poison games).
 * 4. `running` import runs with a stale heartbeat are closed as failed and resumed.
 * Also prunes user events older than 7 days.
 */
export async function reconcile(
  deps: ReconcileDeps,
  opts: ReconcileOptions = {},
): Promise<ReconcileResult> {
  const { db, queues } = deps;
  const log = deps.log ?? defaultLog;
  const now = (deps.now ?? (() => new Date()))();
  const staleCutoff = new Date(now.getTime() - (opts.staleAfterMs ?? 600_000));
  const pendingCutoff = new Date(now.getTime() - (opts.pendingGraceMs ?? 120_000));
  const pageSize = opts.pageSize ?? 1000;
  const result: ReconcileResult = {
    requeuedPending: 0,
    resetStale: 0,
    resumedImports: 0,
    hungActive: 0,
    exhausted: 0,
  };

  const giveUp = async (ids: string[], from: "pending" | "analysing") => {
    if (ids.length === 0) return;
    const failed = await db
      .update(games)
      .set({ analysisStatus: "failed", analysisError: EXHAUSTED_REASON })
      .where(and(inArray(games.id, ids), eq(games.analysisStatus, from)))
      .returning({ id: games.id, accountId: games.accountId });
    result.exhausted += failed.length;
    for (const g of failed) {
      await deps
        .publish?.(g.accountId, "job.failed", {
          kind: "analyse-game",
          gameId: g.id,
          reason: EXHAUSTED_REASON,
        })
        .catch((e) => log.warn({ gameId: g.id, err: errorMessage(e) }, "publish failed"));
    }
  };

  // 1. Lost pending games. Runs first, so games reset in step 2 are counted only there.
  const pendingOld = sql`coalesce(${games.pendingSince}, 'epoch'::timestamptz) < ${pendingCutoff.toISOString()}::timestamptz`;
  let afterId: string | null = null;
  for (;;) {
    const page: { id: string; attempts: number }[] = await db
      .select({ id: games.id, attempts: games.analysisAttempts })
      .from(games)
      .where(
        and(
          eq(games.analysisStatus, "pending"),
          pendingOld,
          afterId === null ? undefined : gt(games.id, afterId),
        ),
      )
      .orderBy(asc(games.id))
      .limit(pageSize);
    if (page.length === 0) break;
    afterId = page[page.length - 1]?.id ?? null;
    const liveness = await queues.analyseLiveness(page.map((g) => g.id));
    const exhausted: string[] = [];
    for (const [i, g] of page.entries()) {
      if (liveness[i] === "live") continue;
      if (g.attempts >= MAX_ANALYSIS_ATTEMPTS) {
        exhausted.push(g.id);
        continue;
      }
      if (await queues.enqueueAnalyse(g.id)) result.requeuedPending++;
    }
    await giveUp(exhausted, "pending");
    if (page.length < pageSize) break;
  }

  // 2. Stale analyses. A null heartbeat falls back to the start time (or counts as stale).
  const stale = await db
    .select({ id: games.id, attempts: games.analysisAttempts })
    .from(games)
    .where(
      and(
        eq(games.analysisStatus, "analysing"),
        sql`coalesce(${games.heartbeatAt}, ${games.analysisStartedAt}, 'epoch'::timestamptz) < ${staleCutoff.toISOString()}::timestamptz`,
      ),
    );
  const exhausted: string[] = [];
  for (const g of stale) {
    if ((await queues.analyseJobState(g.id)) === "active") {
      // Its worker holds the lock, so re-enqueueing would be a no-op and resetting the row
      // would be a lie. The job deadline ends it; the next sweep sees the outcome.
      result.hungActive++;
      log.warn({ gameId: g.id }, "stale heartbeat but job still active");
      continue;
    }
    if (g.attempts >= MAX_ANALYSIS_ATTEMPTS) {
      exhausted.push(g.id);
      continue;
    }
    const [reset] = await db
      .update(games)
      .set({ analysisStatus: "pending", pendingSince: now })
      .where(
        and(
          eq(games.id, g.id),
          eq(games.analysisStatus, "analysing"),
          eq(games.analysisAttempts, g.attempts),
        ),
      )
      .returning({ id: games.id });
    if (!reset) continue; // it moved on meanwhile
    result.resetStale++;
    await queues.enqueueAnalyse(g.id);
  }
  await giveUp(exhausted, "analysing");

  // 3. Stale import runs.
  const staleRuns = await db
    .update(importRuns)
    .set({
      status: "failed",
      finishedAt: now,
      lastError: "stale heartbeat: resumed by the reconciler",
      errorCode: "timeout",
    })
    .where(and(eq(importRuns.status, "running"), lt(importRuns.heartbeatAt, staleCutoff)))
    .returning({ id: importRuns.id, accountId: importRuns.accountId });
  for (const run of staleRuns) {
    await queues.enqueueImport({ accountId: run.accountId }, { jobId: resumeImportJobId(run.id) });
  }
  result.resumedImports = staleRuns.length;

  const pruned = await db
    .delete(userEvents)
    .where(lt(userEvents.createdAt, new Date(now.getTime() - EVENT_RETENTION_MS)))
    .returning({ id: userEvents.id });

  const counts = await db
    .select({ status: games.analysisStatus, n: sql<number>`count(*)::int` })
    .from(games)
    .where(inArray(games.analysisStatus, ["pending", "analysing", "failed"]))
    .groupBy(games.analysisStatus);
  const [overAttempts] = await db
    .select({ n: sql<number>`count(*)::int` })
    .from(games)
    .where(gte(games.analysisAttempts, MAX_ANALYSIS_ATTEMPTS));
  log.info(
    {
      ...result,
      prunedEvents: pruned.length,
      games: Object.fromEntries(counts.map((c) => [c.status, c.n])),
      atAttemptCap: overAttempts?.n ?? 0,
    },
    "reconcile sweep",
  );
  return result;
}

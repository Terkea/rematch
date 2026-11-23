import type { Db } from "@ci/db";
import { sql } from "drizzle-orm";
import { demoAccountIds } from "../demo";
import { userAccountIds } from "../events/user-accounts";
import { enqueueJob } from "../jobs";
import { ANALYSE_JOB, IMPORT_JOB, RETRY_COOLDOWN_MS, RETRY_LIMIT, SYNC_LOCK_CLASS } from "./jobs";
import { availableAtSql } from "./status";

/**
 * `queued`: an import was enqueued. `already`: a live import exists (due, running, paused, or
 * held back by the worker; one live import per account, `import:<account>`). `throttled`: an import
 * started less than `SYNC_COOLDOWN_MS` ago; `availableAt` says when the next is allowed.
 */
export type SyncOutcome =
  | { accountId: string; outcome: "queued" | "already" }
  | { accountId: string; outcome: "throttled"; availableAt: Date };

export type SyncRequest = { kind: "ok"; accounts: SyncOutcome[] } | { kind: "not_found" };

async function syncOne(tx: Db, accountId: string): Promise<SyncOutcome> {
  await tx.execute(
    sql`select pg_advisory_xact_lock(${SYNC_LOCK_CLASS}::int4, hashtext(${accountId}::text))`,
  );
  // Any live import answers `already`, including one the worker held back (a retry backoff, an
  // open breaker, a busy account): its `run_at` is deliberate and is never moved from here.
  const [live] = (await tx.execute(sql`
    SELECT j.id FROM jobs j
    WHERE j.kind = ${IMPORT_JOB.kind} AND j.dedupe_key = ${IMPORT_JOB.key(accountId)}
      AND j.state IN ('queued', 'running', 'paused')
    LIMIT 1
  `)) as unknown as { id: string }[];
  if (live) return { accountId, outcome: "already" };

  // The 2-minute limit, entirely in database time.
  const [limit] = (await tx.execute(
    sql`SELECT a.at, a.at > now() AS blocked FROM (SELECT ${availableAtSql(sql`${accountId}::uuid`)} AS at) a`,
  )) as unknown as { at: Date | string | null; blocked: boolean | null }[];
  if (limit?.at && limit.blocked) {
    return { accountId, outcome: "throttled", availableAt: new Date(limit.at) };
  }

  const id = await enqueueJob(tx, {
    kind: IMPORT_JOB.kind,
    payload: IMPORT_JOB.payload(accountId),
    dedupeKey: IMPORT_JOB.key(accountId),
    userKey: accountId,
    priority: IMPORT_JOB.priority,
    maxAttempts: IMPORT_JOB.maxAttempts,
  });
  return { accountId, outcome: id === null ? "already" : "queued" };
}

/**
 * "Sync now": an incremental import of one linked account, or of every one when `accountId` is
 * null. Only accounts the user holds a link to (`userAccountIds`); anything else is not found,
 * whatever the reason, so a foreign id leaks nothing. Each account is serialised by
 * `pg_advisory_xact_lock(30040, hashtext(account_id))`, so concurrent clicks enqueue once.
 */
export async function requestSync(
  db: Db,
  userId: string,
  accountId: string | null,
): Promise<SyncRequest> {
  const ids = await userAccountIds(db, userId);
  const targets = accountId === null ? ids : ids.filter((id) => id === accountId);
  if (targets.length === 0) return { kind: "not_found" };
  // The demo player (ADR 0097) is fictional: there is nothing on a platform to fetch.
  const demo = await demoAccountIds(db, targets);
  const accounts: SyncOutcome[] = [];
  for (const id of targets.filter((t) => !demo.has(t))) {
    accounts.push(await db.transaction((tx) => syncOne(tx as unknown as Db, id)));
  }
  return { kind: "ok", accounts };
}

export type RetryRequest = { kind: "ok"; queued: number } | { kind: "not_found" };

/**
 * "Retry" for games whose analysis failed: one `analyse-game` job each (the reconciler's
 * priority), skipping games with a live job or one created in the last `RETRY_COOLDOWN_MS`.
 * The worker's claim moves the game on from `failed`; the web writes no game row.
 */
export async function requestRetry(
  db: Db,
  userId: string,
  accountId: string,
): Promise<RetryRequest> {
  const ids = await userAccountIds(db, userId);
  if (!ids.includes(accountId)) return { kind: "not_found" };
  return db.transaction(async (raw) => {
    const tx = raw as unknown as Db;
    await tx.execute(
      sql`select pg_advisory_xact_lock(${SYNC_LOCK_CLASS}::int4, hashtext(${`retry:${accountId}`}))`,
    );
    const games = (await tx.execute(sql`
      SELECT g.id::text AS id, coalesce(g.analysis_priority, ${ANALYSE_JOB.defaultPriority})::int AS priority
      FROM games g
      WHERE g.account_id = ${accountId}::uuid AND g.analysis_status = 'failed'
        AND NOT EXISTS (
          SELECT 1 FROM jobs j WHERE j.kind = ${ANALYSE_JOB.kind}
            AND j.dedupe_key = ${ANALYSE_JOB.key("")} || g.id::text
            AND (j.state IN ('queued', 'running', 'paused')
              OR j.created_at > now() - ${RETRY_COOLDOWN_MS} * interval '1 millisecond'))
      ORDER BY g.played_at DESC, g.id
      LIMIT ${RETRY_LIMIT}
    `)) as unknown as { id: string; priority: number }[];
    let queued = 0;
    for (const g of games) {
      const id = await enqueueJob(tx, {
        kind: ANALYSE_JOB.kind,
        payload: ANALYSE_JOB.payload(g.id),
        dedupeKey: ANALYSE_JOB.key(g.id),
        userKey: accountId,
        priority: Number(g.priority),
        maxAttempts: ANALYSE_JOB.maxAttempts,
      });
      if (id !== null) queued++;
    }
    return { kind: "ok", queued };
  });
}

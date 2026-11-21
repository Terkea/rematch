import { type Db, games, importRuns, userEvents } from "@ci/db";
import { Worker } from "bullmq";
import { eq, ne } from "drizzle-orm";
import type { Redis } from "ioredis";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { MAX_ANALYSIS_ATTEMPTS, reconcile } from "../../src/jobs/reconcile.js";
import {
  ANALYSE_GAME_QUEUE,
  analyseJobId,
  createQueues,
  createRedisConnection,
  type Queues,
} from "../../src/queue/queues.js";
import { getWorkerTestDb } from "../db.js";
import { insertAccount } from "../jobs/helpers.js";
import {
  addGame,
  getGame,
  redisUrl,
  silentLog,
  stackConfigured,
  uniquePrefix,
  waitFor,
} from "./helpers.js";

const MIN = 60_000;
/** Pending games are only swept once they have been pending for 2 minutes; look from 3 ahead. */
const later = () => new Date(Date.now() + 3 * MIN);
const ZERO = { requeuedPending: 0, resetStale: 0, resumedImports: 0, hungActive: 0, exhausted: 0 };

describe.skipIf(!stackConfigured())("reconciler", () => {
  let db: Db;
  let connection: Redis;
  let queues: Queues;
  const prefix = uniquePrefix();
  const published: { accountId: string; type: string; payload: object }[] = [];
  const publish = async (accountId: string, type: string, payload: object) => {
    published.push({ accountId, type, payload });
    return published.length;
  };

  beforeAll(async () => {
    db = (await getWorkerTestDb()) as Db;
    connection = createRedisConnection(redisUrl);
    queues = createQueues(connection, { prefix });
  });

  afterAll(async () => {
    await queues.obliterate();
    await queues.close();
    connection.disconnect();
    await db.$client.end();
  });

  const deps = (now: Date) => ({ db, queues, now: () => now, publish, log: silentLog });

  it("reconciler requeues lost pending", async () => {
    const { id } = await addGame(db, "1. e4 e5 2. Nf3 *", "lost");
    expect(await queues.analyseGame.getJob(analyseJobId(id))).toBeUndefined();

    // Freshly pending games are left alone: their enqueue may simply not have happened yet.
    expect(await reconcile(deps(new Date()))).toEqual(ZERO);

    expect(await reconcile(deps(later()))).toEqual({ ...ZERO, requeuedPending: 1 });
    const job = await queues.analyseGame.getJob(analyseJobId(id));
    expect(job?.data).toEqual({ gameId: id });

    // Idempotent: the job is now live, so a second sweep does nothing.
    expect((await reconcile(deps(later()))).requeuedPending).toBe(0);
  });

  it("reconciler resets stale analysing", async () => {
    const now = later();
    const stale = await addGame(db, "1. d4 d5 *", "stale");
    const fresh = await addGame(db, "1. c4 c5 *", "fresh");
    await db
      .update(games)
      .set({ analysisStatus: "analysing", heartbeatAt: new Date(now.getTime() - 11 * MIN) })
      .where(eq(games.id, stale.id));
    await db
      .update(games)
      .set({ analysisStatus: "analysing", heartbeatAt: new Date(now.getTime() - 1 * MIN) })
      .where(eq(games.id, fresh.id));

    expect(await reconcile(deps(now))).toEqual({ ...ZERO, resetStale: 1 });
    expect((await getGame(db, stale.id))?.analysisStatus).toBe("pending");
    expect(await queues.analyseGame.getJob(analyseJobId(stale.id))).toBeDefined();
    expect((await getGame(db, fresh.id))?.analysisStatus).toBe("analysing");
    expect(await queues.analyseGame.getJob(analyseJobId(fresh.id))).toBeUndefined();
  });

  it("honours staleAfterMs", async () => {
    const now = later();
    const g = await addGame(db, "1. Nf3 Nf6 *", "custom");
    await db
      .update(games)
      .set({ analysisStatus: "analysing", heartbeatAt: new Date(now.getTime() - 2 * MIN) })
      .where(eq(games.id, g.id));
    const res = await reconcile(deps(now), { staleAfterMs: 1.5 * MIN });
    expect(res.resetStale).toBe(1);
  });

  it("fails a game that exhausted its attempts instead of re-enqueueing it forever", async () => {
    expect(MAX_ANALYSIS_ATTEMPTS).toBe(6);
    const now = later();
    const stuck = await addGame(db, "1. b3 e5 *", "exhausted");
    await db
      .update(games)
      .set({
        analysisStatus: "analysing",
        analysisAttempts: 6,
        heartbeatAt: new Date(now.getTime() - 11 * MIN),
      })
      .where(eq(games.id, stuck.id));
    const lost = await addGame(db, "1. b4 e5 *", "exhausted-pending");
    await db.update(games).set({ analysisAttempts: 7 }).where(eq(games.id, lost.id));
    published.length = 0;

    expect(await reconcile(deps(now))).toEqual({ ...ZERO, exhausted: 2 });
    for (const g of [stuck, lost]) {
      const row = await getGame(db, g.id);
      expect(row?.analysisStatus).toBe("failed");
      expect(row?.analysisError).toBe("exceeded max attempts (stalled or crashed worker)");
      expect(await queues.analyseGame.getJob(analyseJobId(g.id))).toBeUndefined();
    }
    expect(published.map((p) => [p.type, (p.payload as { gameId: string }).gameId]).sort()).toEqual(
      [
        ["job.failed", lost.id],
        ["job.failed", stuck.id],
      ].sort(),
    );
  });

  it("leaves a stale game whose job is still active alone, and counts it as hungActive", async () => {
    const now = later();
    const hung = await addGame(db, "1. g3 d5 *", "hung");
    await db
      .update(games)
      .set({ analysisStatus: "analysing", heartbeatAt: new Date(now.getTime() - 11 * MIN) })
      .where(eq(games.id, hung.id));
    // Only this game's job may be picked up by the hanging worker below.
    await db.update(games).set({ analysisStatus: "done" }).where(ne(games.id, hung.id));
    await queues.analyseGame.drain(true);
    await queues.enqueueAnalyse(hung.id);
    const worker = new Worker(ANALYSE_GAME_QUEUE, () => new Promise(() => {}), {
      connection,
      prefix,
    });
    try {
      await waitFor(async () => (await queues.analyseJobState(hung.id)) === "active", {
        what: "the job to become active",
      });
      expect(await reconcile(deps(now))).toEqual({ ...ZERO, hungActive: 1 });
      expect((await getGame(db, hung.id))?.analysisStatus).toBe("analysing");
      expect(await queues.analyseJobState(hung.id)).toBe("active");
    } finally {
      await worker.close(true);
      await db.update(games).set({ analysisStatus: "done" }).where(eq(games.id, hung.id));
    }
  });

  it("resumes import runs whose heartbeat went stale", async () => {
    const now = later();
    const acct = await insertAccount(db, "importer");
    const [staleRun] = await db
      .insert(importRuns)
      .values({
        accountId: acct.id,
        status: "running",
        heartbeatAt: new Date(now.getTime() - 11 * MIN),
      })
      .returning();
    const [liveRun] = await db
      .insert(importRuns)
      .values({ accountId: acct.id, status: "running", heartbeatAt: now })
      .returning();

    expect((await reconcile(deps(now))).resumedImports).toBe(1);
    const jobs = await queues.importAccount.getJobs(["waiting", "delayed"]);
    expect(jobs.map((j) => j.data)).toEqual([{ accountId: acct.id }]);

    const runs = await db.select().from(importRuns).where(eq(importRuns.accountId, acct.id));
    const byId = new Map(runs.map((r) => [r.id, r]));
    expect(byId.get(staleRun?.id as string)?.status).toBe("failed");
    expect(byId.get(staleRun?.id as string)?.lastError).toMatch(/stale/);
    expect(byId.get(liveRun?.id as string)?.status).toBe("running");

    // The stale run is closed, so it is not resumed twice.
    expect((await reconcile(deps(now))).resumedImports).toBe(0);
  });

  it("prunes user events older than 7 days", async () => {
    const now = later();
    const acct = await insertAccount(db, "pruned");
    await db.insert(userEvents).values([
      {
        accountId: acct.id,
        type: "x",
        payload: {},
        createdAt: new Date(now.getTime() - 8 * 24 * 60 * MIN),
      },
      { accountId: acct.id, type: "y", payload: {} },
    ]);
    await reconcile(deps(now));
    const left = await db.select().from(userEvents).where(eq(userEvents.accountId, acct.id));
    expect(left.map((e) => e.type)).toEqual(["y"]);
  });
});

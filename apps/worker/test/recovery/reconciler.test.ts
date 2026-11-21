import { type Db, games, importRuns } from "@ci/db";
import { eq } from "drizzle-orm";
import type { Redis } from "ioredis";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { reconcile } from "../../src/jobs/reconcile.js";
import {
  analyseJobId,
  createQueues,
  createRedisConnection,
  type Queues,
} from "../../src/queue/queues.js";
import { getWorkerTestDb } from "../db.js";
import { insertAccount } from "../jobs/helpers.js";
import { addGame, getGame, redisUrl, stackConfigured, uniquePrefix } from "./helpers.js";

const MIN = 60_000;

describe.skipIf(!stackConfigured())("reconciler", () => {
  let db: Db;
  let connection: Redis;
  let queues: Queues;

  beforeAll(async () => {
    db = (await getWorkerTestDb()) as Db;
    connection = createRedisConnection(redisUrl);
    queues = createQueues(connection, { prefix: uniquePrefix() });
  });

  afterAll(async () => {
    await queues.obliterate();
    await queues.close();
    connection.disconnect();
    await db.$client.end();
  });

  it("reconciler requeues lost pending", async () => {
    const { id } = await addGame(db, "1. e4 e5 2. Nf3 *", "lost");
    expect(await queues.analyseGame.getJob(analyseJobId(id))).toBeUndefined();

    expect(await reconcile({ db, queues })).toEqual({
      requeuedPending: 1,
      resetStale: 0,
      resumedImports: 0,
    });
    const job = await queues.analyseGame.getJob(analyseJobId(id));
    expect(job?.data).toEqual({ gameId: id });

    // Idempotent: the job is now live, so a second sweep does nothing.
    expect((await reconcile({ db, queues })).requeuedPending).toBe(0);
  });

  it("reconciler resets stale analysing", async () => {
    const now = new Date();
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

    expect(await reconcile({ db, queues, now: () => now })).toEqual({
      requeuedPending: 0,
      resetStale: 1,
      resumedImports: 0,
    });
    expect((await getGame(db, stale.id))?.analysisStatus).toBe("pending");
    expect(await queues.analyseGame.getJob(analyseJobId(stale.id))).toBeDefined();
    expect((await getGame(db, fresh.id))?.analysisStatus).toBe("analysing");
    expect(await queues.analyseGame.getJob(analyseJobId(fresh.id))).toBeUndefined();
  });

  it("honours staleAfterMs", async () => {
    const now = new Date();
    const g = await addGame(db, "1. Nf3 Nf6 *", "custom");
    await db
      .update(games)
      .set({ analysisStatus: "analysing", heartbeatAt: new Date(now.getTime() - 2 * MIN) })
      .where(eq(games.id, g.id));
    const res = await reconcile({ db, queues, now: () => now }, { staleAfterMs: 1.5 * MIN });
    expect(res.resetStale).toBe(1);
  });

  it("resumes import runs whose heartbeat went stale", async () => {
    const now = new Date();
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

    expect((await reconcile({ db, queues, now: () => now })).resumedImports).toBe(1);
    const jobs = await queues.importAccount.getJobs(["waiting", "delayed"]);
    expect(jobs.map((j) => j.data)).toEqual([{ accountId: acct.id }]);

    const runs = await db.select().from(importRuns).where(eq(importRuns.accountId, acct.id));
    const byId = new Map(runs.map((r) => [r.id, r]));
    expect(byId.get(staleRun?.id as string)?.status).toBe("failed");
    expect(byId.get(staleRun?.id as string)?.lastError).toMatch(/stale/);
    expect(byId.get(liveRun?.id as string)?.status).toBe("running");

    // The stale run is closed, so it is not resumed twice.
    expect((await reconcile({ db, queues, now: () => now })).resumedImports).toBe(0);
  });
});

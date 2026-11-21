import { type Db, userEvents } from "@ci/db";
import type { Worker } from "bullmq";
import { eq } from "drizzle-orm";
import type { Redis } from "ioredis";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { createDbEvalCache } from "../../src/analysis/eval-cache.js";
import { publishEvent } from "../../src/events/publish.js";
import {
  analyseJobId,
  createQueues,
  createRedisConnection,
  type Queues,
} from "../../src/queue/queues.js";
import { createAnalyseWorker } from "../../src/start-workers.js";
import { getWorkerTestDb } from "../db.js";
import {
  addGame,
  FAST_RETRY,
  FAST_WORKER,
  fakeLine,
  getGame,
  redisUrl,
  silentLog,
  stackConfigured,
  uniquePrefix,
  waitFor,
} from "./helpers.js";

describe.skipIf(!stackConfigured())("poison jobs", () => {
  let db: Db;
  let connection: Redis;
  let queues: Queues;
  let worker: Worker | undefined;
  const prefix = uniquePrefix();

  beforeAll(async () => {
    db = (await getWorkerTestDb()) as Db;
    connection = createRedisConnection(redisUrl);
    queues = createQueues(connection, { prefix, jobOptions: FAST_RETRY });
  });

  afterAll(async () => {
    await worker?.close(true);
    await queues.obliterate();
    await queues.close();
    connection.disconnect();
    await db.$client.end();
  });

  it("poison game fails after 3 attempts", async () => {
    // An illegal move: chess.js throws a parse error that is not UnsupportedVariantError.
    const { id, accountId } = await addGame(db, "1. e4 e5 2. Ke9 *", "poison");
    const statusesSeen = new Set<string>();
    const pool = {
      analyse: async () => fakeLine,
    };
    worker = createAnalyseWorker(
      { connection, prefix, workerOptions: FAST_WORKER, log: silentLog },
      {
        db,
        pool,
        cache: createDbEvalCache(db),
        publish: (acct, type, payload) =>
          publishEvent({ db, redis: connection }, acct, type, payload),
      },
      1,
    );
    await queues.enqueueAnalyse(id);

    const failed = await waitFor(
      async () => {
        const g = await getGame(db, id);
        if (g) statusesSeen.add(g.analysisStatus);
        const state = await (await queues.analyseGame.getJob(analyseJobId(id)))?.getState();
        return g?.analysisStatus === "failed" && state === "failed" ? g : null;
      },
      { timeoutMs: 15_000, intervalMs: 10, what: "the job to fail for good" },
    );
    expect(failed.analysisAttempts).toBe(3);
    expect(failed.analysisError).toBeTruthy();
    // Between attempts the game is pending again (not failed), so nothing looks final too early.
    expect(statusesSeen.has("done")).toBe(false);

    const events = await db.select().from(userEvents).where(eq(userEvents.accountId, accountId));
    expect(events).toHaveLength(1);
    expect(events[0]?.type).toBe("job.failed");
    expect(events[0]?.payload).toMatchObject({ kind: "analyse-game", gameId: id });
  }, 20_000);
});

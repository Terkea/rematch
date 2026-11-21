import { type Db, moves } from "@ci/db";
import type { Worker } from "bullmq";
import { eq } from "drizzle-orm";
import type { Redis } from "ioredis";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { createDbEvalCache } from "../../src/analysis/eval-cache.js";
import { createQueues, createRedisConnection, type Queues } from "../../src/queue/queues.js";
import { createAnalyseWorker } from "../../src/start-workers.js";
import { getWorkerTestDb } from "../db.js";
import {
  addGame,
  FAST_WORKER,
  fakeLine,
  getGame,
  redisUrl,
  silentLog,
  stackConfigured,
  uniquePrefix,
  waitFor,
} from "./helpers.js";

// 10 plies, 11 distinct positions, not terminal.
const RUY = "1. e4 e5 2. Nf3 Nc6 3. Bb5 a6 4. Ba4 Nf6 5. O-O Be7 *";

describe.skipIf(!stackConfigured())("crash recovery: worker killed mid-analysis", () => {
  let db: Db;
  let connection: Redis;
  let queues: Queues;
  const prefix = uniquePrefix();
  const workers: Worker[] = [];

  beforeAll(async () => {
    db = (await getWorkerTestDb()) as Db;
    connection = createRedisConnection(redisUrl);
    queues = createQueues(connection, { prefix });
  });

  afterAll(async () => {
    await Promise.all(workers.map((w) => w.close(true)));
    await queues.obliterate();
    await queues.close();
    connection.disconnect();
    await db.$client.end();
  });

  it("kill mid-analysis still completes", async () => {
    const { id } = await addGame(db, RUY, "killed");
    const rt = { connection, prefix, workerOptions: FAST_WORKER, log: silentLog };

    const firstCalls: string[] = [];
    const hanging = {
      analyse: (fen: string) => {
        firstCalls.push(fen);
        // Position 4 never returns: the worker "dies" while it is in flight.
        if (firstCalls.length === 4) return new Promise<never>(() => {});
        return Promise.resolve(fakeLine);
      },
    };
    const crashed = createAnalyseWorker(
      rt,
      { db, pool: hanging, cache: createDbEvalCache(db), concurrency: 1 },
      1,
    );
    workers.push(crashed);
    await queues.enqueueAnalyse(id);
    await waitFor(() => firstCalls.length === 4, { what: "the 4th engine call" });
    expect((await getGame(db, id))?.analysisStatus).toBe("analysing");

    // Simulated crash: stop without finishing or releasing the job.
    await crashed.close(true);

    const secondCalls: string[] = [];
    const healthy = {
      analyse: async (fen: string) => {
        secondCalls.push(fen);
        return fakeLine;
      },
    };
    workers.push(
      createAnalyseWorker(
        rt,
        { db, pool: healthy, cache: createDbEvalCache(db), concurrency: 1 },
        1,
      ),
    );

    const done = await waitFor(
      async () => {
        const g = await getGame(db, id);
        return g?.analysisStatus === "done" ? g : null;
      },
      { timeoutMs: 15_000, what: "the game to be done" },
    );
    expect(done.analysisAttempts).toBe(2);
    expect(done.analysisStartedAt).toBeInstanceOf(Date);

    const rows = await db.select().from(moves).where(eq(moves.gameId, id));
    expect(rows.map((r) => r.ply).sort((a, b) => a - b)).toEqual([1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);

    // The 3 positions finished before the crash came from the cache; the rest went to the engine.
    for (const fen of firstCalls.slice(0, 3)) expect(secondCalls).not.toContain(fen);
    expect(secondCalls).toContain(firstCalls[3]);
    expect(firstCalls.length + secondCalls.length).toBe(11 + 1);
  }, 25_000);
});

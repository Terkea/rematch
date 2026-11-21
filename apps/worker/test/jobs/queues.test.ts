import { randomUUID } from "node:crypto";
import { Worker } from "bullmq";
import type { Redis } from "ioredis";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import {
  analyseJobId,
  createQueues,
  createRedisConnection,
  DEFAULT_JOB_OPTIONS,
  type Queues,
  RECONCILE_EVERY_MS,
  RECONCILE_QUEUE,
} from "../../src/queue/queues.js";

const url = process.env.REDIS_URL;

describe.skipIf(!url && process.env.IN_DOCKER !== "1")("queues", () => {
  let connection: Redis;
  let queues: Queues;
  const prefix = `test-${randomUUID()}`;

  beforeAll(() => {
    connection = createRedisConnection(url as string);
    queues = createQueues(connection, { prefix });
  });

  afterAll(async () => {
    await queues.obliterate();
    await queues.close();
    connection.disconnect();
  });

  it("uses the brief's default job options, with bounded retention", () => {
    expect(DEFAULT_JOB_OPTIONS).toEqual({
      attempts: 3,
      backoff: { type: "exponential", delay: 5000 },
      removeOnComplete: { count: 1000, age: 24 * 3600 },
      removeOnFail: { age: 7 * 24 * 3600 },
    });
    expect(queues.analyseGame.name).toBe("analyse-game");
    expect(queues.importAccount.name).toBe("import-account");
    expect(queues.reconcile.name).toBe(RECONCILE_QUEUE);
    expect(RECONCILE_QUEUE).toBe("reconcile");
  });

  it("enqueueing the same game twice creates one job", async () => {
    await queues.enqueueAnalyse("g1");
    await queues.enqueueAnalyse("g1");
    await queues.enqueueAnalyse("g2");
    const counts = await queues.analyseGame.getJobCounts("waiting", "delayed", "active");
    expect((counts.waiting ?? 0) + (counts.delayed ?? 0) + (counts.active ?? 0)).toBe(2);
    const job = await queues.analyseGame.getJob(analyseJobId("g1"));
    expect(job?.data).toEqual({ gameId: "g1" });
    expect(job?.opts.attempts).toBe(3);
  });

  it("re-enqueues a game whose previous job is retained as completed", async () => {
    await queues.enqueueAnalyse("again");
    const worker = new Worker(
      queues.analyseGame.name,
      async (job) => {
        if (job.data.gameId !== "again") throw new Error("not this one");
      },
      { connection, prefix },
    );
    try {
      const finished = async () =>
        (await queues.analyseGame.getJob(analyseJobId("again")))?.getState();
      for (let i = 0; i < 200 && (await finished()) !== "completed"; i++) {
        await new Promise((r) => setTimeout(r, 25));
      }
      expect(await finished()).toBe("completed");
    } finally {
      await worker.close();
    }
    // Otherwise the retained id would silently swallow the re-analysis.
    expect(await queues.enqueueAnalyse("again")).toBe(true);
    const state = await (await queues.analyseGame.getJob(analyseJobId("again")))?.getState();
    expect(state).toBe("waiting");
    // A live job is not duplicated.
    expect(await queues.enqueueAnalyse("again")).toBe(false);
  });

  it("schedules one repeatable reconcile job every 5 minutes", async () => {
    await queues.scheduleReconcile();
    await queues.scheduleReconcile();
    expect(RECONCILE_EVERY_MS).toBe(5 * 60_000);
    const schedulers = await queues.reconcile.getJobSchedulers();
    expect(schedulers).toHaveLength(1);
    expect(schedulers[0]?.every).toBe(RECONCILE_EVERY_MS);
  });

  it("checks job liveness for many games in one pipelined round trip", async () => {
    await queues.enqueueAnalyse("fin");
    const worker = new Worker(queues.analyseGame.name, async () => {}, { connection, prefix });
    try {
      for (let i = 0; i < 200 && (await queues.analyseJobState("fin")) !== "completed"; i++) {
        await new Promise((r) => setTimeout(r, 25));
      }
    } finally {
      await worker.close();
    }
    await queues.enqueueAnalyse("still-waiting");
    expect(await queues.analyseLiveness(["fin", "still-waiting", "never-enqueued"])).toEqual([
      "finished",
      "live",
      "none",
    ]);
    expect(await queues.analyseLiveness([])).toEqual([]);
  });
});

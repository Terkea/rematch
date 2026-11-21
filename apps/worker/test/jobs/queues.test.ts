import { randomUUID } from "node:crypto";
import { afterAll, describe, expect, it } from "vitest";
import {
  analyseJobId,
  createQueues,
  createRedisConnection,
  DEFAULT_JOB_OPTIONS,
} from "../../src/queue/queues.js";

const url = process.env.REDIS_URL;

describe.skipIf(!url && process.env.IN_DOCKER !== "1")("queues", () => {
  const connection = createRedisConnection(url as string);
  const queues = createQueues(connection, `test-${randomUUID()}`);

  afterAll(async () => {
    await queues.analyseGame.obliterate({ force: true });
    await queues.importAccount.obliterate({ force: true });
    await queues.close();
    connection.disconnect();
  });

  it("uses the brief's default job options", () => {
    expect(DEFAULT_JOB_OPTIONS).toEqual({
      attempts: 3,
      backoff: { type: "exponential", delay: 5000 },
    });
    expect(queues.analyseGame.name).toBe("analyse-game");
    expect(queues.importAccount.name).toBe("import-account");
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
});

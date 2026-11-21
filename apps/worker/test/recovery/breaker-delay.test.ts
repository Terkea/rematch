import { type Db, games, importRuns } from "@ci/db";
import type { Job, Worker } from "bullmq";
import { eq } from "drizzle-orm";
import type { Redis } from "ioredis";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { CircuitOpenError, createCircuitBreaker } from "../../src/http/circuit-breaker.js";
import type { PlatformClient } from "../../src/http/platform-client.js";
import { createQueues, createRedisConnection, type Queues } from "../../src/queue/queues.js";
import { createImportWorker } from "../../src/start-workers.js";
import { getWorkerTestDb } from "../db.js";
import { insertAccount, makeGame } from "../jobs/helpers.js";
import {
  FAST_WORKER,
  redisUrl,
  silentLog,
  stackConfigured,
  uniquePrefix,
  waitFor,
} from "./helpers.js";

describe.skipIf(!stackConfigured())("open circuit breaker delays imports", () => {
  let db: Db;
  let connection: Redis;
  let queues: Queues;
  let worker: Worker | undefined;
  const prefix = uniquePrefix();

  beforeAll(async () => {
    db = (await getWorkerTestDb()) as Db;
    connection = createRedisConnection(redisUrl);
    queues = createQueues(connection, { prefix });
  });

  afterAll(async () => {
    await worker?.close(true);
    await queues.obliterate();
    await queues.close();
    connection.disconnect();
    await db.$client.end();
  });

  it("delays the job without using an attempt, then resumes once the breaker closes", async () => {
    expect(new CircuitOpenError(1234).retryInMs).toBe(1234);
    const acct = await insertAccount(db, "patient");
    const breaker = createCircuitBreaker({ failureThreshold: 1, cooldownMs: 700 });
    await breaker.exec(() => Promise.reject(new Error("HTTP 503"))).catch(() => {});
    expect(breaker.state()).toBe("open");

    const client: PlatformClient = {
      getJson: async <T>() => ({}) as T,
      getNdjson: async function* <T>() {
        yield* [] as T[];
      },
    };
    const game = makeGame({ white: { name: "Patient", rating: 1 } });
    const importer = async function* (c: PlatformClient) {
      await c.getJson("https://example.invalid/probe");
      yield game;
    };
    const failedEvents: string[] = [];
    worker = createImportWorker(
      { connection, prefix, workerOptions: FAST_WORKER, log: silentLog },
      {
        db,
        importers: { lichess: importer, chesscom: importer } as never,
        clients: { lichess: client, chesscom: client },
        breakers: { lichess: breaker, chesscom: breaker },
        enqueueAnalyse: async () => {},
        publish: async (_a, type) => {
          if (type === "job.failed") failedEvents.push(type);
          return 1;
        },
      },
      1,
    );
    await queues.enqueueImport({ accountId: acct.id }, { jobId: "patient-import" });

    const delayed = await waitFor(
      async () => {
        const job = (await queues.importAccount.getJob("patient-import")) as Job | undefined;
        return job && (await job.getState()) === "delayed" ? job : null;
      },
      { intervalMs: 10, what: "the job to be delayed" },
    );
    expect(delayed.attemptsMade).toBe(0);
    const [paused] = await db.select().from(importRuns).where(eq(importRuns.accountId, acct.id));
    expect(paused).toMatchObject({ status: "paused", errorCode: "circuit_open" });

    await waitFor(
      async () => (await queues.importAccount.getJobState("patient-import")) === "completed",
      { what: "the job to complete after the cooldown" },
    );
    const done = await queues.importAccount.getJob("patient-import");
    expect(done?.attemptsMade).toBeLessThanOrEqual(1);
    expect(breaker.state()).toBe("closed");
    expect(await db.select().from(games).where(eq(games.accountId, acct.id))).toHaveLength(1);
    const runs = await db.select().from(importRuns).where(eq(importRuns.accountId, acct.id));
    expect(runs.map((r) => r.status).sort()).toEqual(["done", "paused"]);
    expect(failedEvents).toEqual([]);
  }, 20_000);
});

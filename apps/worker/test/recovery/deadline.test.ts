import { type Db, importRuns } from "@ci/db";
import type { Worker } from "bullmq";
import { eq } from "drizzle-orm";
import type { Redis } from "ioredis";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { createDbEvalCache } from "../../src/analysis/eval-cache.js";
import { handleAnalyseGame } from "../../src/jobs/analyse-game.js";
import {
  ANALYSE_DEADLINE_MS,
  IMPORT_IDLE_DEADLINE_MS,
  JobDeadlineError,
} from "../../src/jobs/deadline.js";
import { handleImportAccount } from "../../src/jobs/import-account.js";
import { createQueues, createRedisConnection, type Queues } from "../../src/queue/queues.js";
import { createAnalyseWorker } from "../../src/start-workers.js";
import { getWorkerTestDb } from "../db.js";
import { insertAccount, makeGame } from "../jobs/helpers.js";
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

describe.skipIf(!stackConfigured())("hard deadlines for hung jobs", () => {
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

  it("uses the brief's deadlines: 10 min per analysis, 30 min without import progress", () => {
    expect(ANALYSE_DEADLINE_MS).toBe(10 * 60_000);
    expect(IMPORT_IDLE_DEADLINE_MS).toBe(30 * 60_000);
  });

  it("an analysis hung on a never-resolving await ends with JobDeadlineError", async () => {
    const { id } = await addGame(db, "1. e4 d5 2. exd5 Qxd5 *", "deadline-unit");
    const hanging = { analyse: () => new Promise<never>(() => {}) };
    const started = Date.now();
    await expect(
      handleAnalyseGame(
        { db, pool: hanging, cache: createDbEvalCache(db), deadlineMs: 300, log: silentLog },
        { gameId: id },
        { attemptsMade: 0, attempts: 3 },
      ),
    ).rejects.toBeInstanceOf(JobDeadlineError);
    expect(Date.now() - started).toBeLessThan(1500);
    const g = await getGame(db, id);
    expect(g?.analysisStatus).toBe("pending");
    expect(g?.analysisError).toMatch(/deadline/);
  });

  it("a worker whose lock stays alive is still recovered: the deadline ends the job", async () => {
    const { id } = await addGame(db, "1. c4 e5 2. Nc3 Nf6 *", "deadline-worker");
    let calls = 0;
    const flaky = {
      analyse: (): Promise<typeof fakeLine> => {
        calls++;
        // The first engine call hangs forever; the lock keeps being renewed meanwhile.
        return calls === 1 ? new Promise<never>(() => {}) : Promise.resolve(fakeLine);
      },
    };
    worker = createAnalyseWorker(
      { connection, prefix, workerOptions: FAST_WORKER, log: silentLog },
      { db, pool: flaky, cache: createDbEvalCache(db), deadlineMs: 800, concurrency: 1 },
      1,
    );
    const stalled: string[] = [];
    const failures: string[] = [];
    worker.on("stalled", (jobId) => stalled.push(jobId));
    worker.on("failed", (_job, err) => failures.push(err.name));
    await queues.enqueueAnalyse(id);
    const done = await waitFor(
      async () => {
        const g = await getGame(db, id);
        return g?.analysisStatus === "done" ? g : null;
      },
      { timeoutMs: 10_000, what: "the retried analysis to finish" },
    );
    expect(done.analysisAttempts).toBe(2);
    // Ended by the deadline, not by stalled-job detection (the lock never expired).
    expect(failures).toEqual(["JobDeadlineError"]);
    expect(stalled).toEqual([]);
  }, 20_000);

  it("an import with no progress for the idle deadline fails as a timeout", async () => {
    const acct = await insertAccount(db, "idle");
    const first = makeGame({ white: { name: "Idle", rating: 1 } });
    const importer = async function* () {
      yield first;
      await new Promise<never>(() => {}); // the stream goes silent
    };
    await expect(
      handleImportAccount(
        {
          db,
          importers: { lichess: importer, chesscom: importer } as never,
          clients: { lichess: {}, chesscom: {} } as never,
          enqueueAnalyse: async () => {},
          idleDeadlineMs: 300,
          log: silentLog,
        },
        { accountId: acct.id },
      ),
    ).rejects.toBeInstanceOf(JobDeadlineError);
    const [run] = await db.select().from(importRuns).where(eq(importRuns.accountId, acct.id));
    expect(run).toMatchObject({ status: "failed", errorCode: "timeout", imported: 1 });
  });
});

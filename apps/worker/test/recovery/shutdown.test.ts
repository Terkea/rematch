import { EventEmitter } from "node:events";
import type { Db } from "@ci/db";
import type { Worker } from "bullmq";
import type { Redis } from "ioredis";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { createDbEvalCache } from "../../src/analysis/eval-cache.js";
import { EnginePool } from "../../src/engine/engine-pool.js";
import { installGracefulShutdown } from "../../src/lifecycle/shutdown.js";
import {
  analyseJobId,
  createQueues,
  createRedisConnection,
  type Queues,
} from "../../src/queue/queues.js";
import { createAnalyseWorker } from "../../src/start-workers.js";
import { getWorkerTestDb } from "../db.js";
import { skipEngineTests, stockfishPath } from "../engine/stockfish.js";
import {
  addGame,
  FAST_WORKER,
  getGame,
  isDead,
  redisUrl,
  silentLog,
  stackConfigured,
  uniquePrefix,
  waitFor,
} from "./helpers.js";

function harness() {
  const order: string[] = [];
  const proc = new EventEmitter();
  let exitCode: number | undefined;
  const exited = new Promise<number>((resolve) => {
    proc.once("test-exit", resolve);
  });
  const exit = (code: number) => {
    exitCode = code;
    proc.emit("test-exit", code);
  };
  const closer = (name: string, fn: () => Promise<void> = async () => {}) => ({
    calls: 0,
    async close(force?: boolean) {
      this.calls++;
      order.push(force ? `${name}(force)` : name);
      await fn();
    },
  });
  return { order, proc, exit, exited, closer, code: () => exitCode };
}

describe("installGracefulShutdown (fakes)", () => {
  it("closes workers, then the pool, db and redis, and exits 0", async () => {
    const h = harness();
    const worker = h.closer("worker");
    const pool = h.closer("pool");
    const db = { $client: { end: async () => void h.order.push("db") } };
    const redis = { quit: async () => void h.order.push("redis"), disconnect() {} };
    installGracefulShutdown(
      { workers: [worker], pool, db, redis },
      { process: h.proc, exit: h.exit, log: silentLog },
    );
    h.proc.emit("SIGTERM");
    expect(await h.exited).toBe(0);
    expect(h.order).toEqual(["worker", "pool", "db", "redis"]);
  });

  it("a second SIGTERM does not close anything twice", async () => {
    const h = harness();
    let release!: () => void;
    const worker = h.closer("worker", () => new Promise<void>((r) => (release = r)));
    const pool = h.closer("pool");
    const db = { $client: { end: async () => {} } };
    const redis = { quit: async () => "OK", disconnect() {} };
    installGracefulShutdown(
      { workers: [worker], pool, db, redis },
      { process: h.proc, exit: h.exit, log: silentLog },
    );
    h.proc.emit("SIGTERM");
    h.proc.emit("SIGTERM");
    h.proc.emit("SIGINT");
    release();
    expect(await h.exited).toBe(0);
    expect(worker.calls).toBe(1);
    expect(pool.calls).toBe(1);
  });

  it("exits non-zero when a close fails, and still closes the rest", async () => {
    const h = harness();
    const worker = h.closer("worker");
    const pool = h.closer("pool", async () => {
      throw new Error("boom");
    });
    const db = { $client: { end: async () => void h.order.push("db") } };
    const redis = { quit: async () => void h.order.push("redis"), disconnect() {} };
    installGracefulShutdown(
      { workers: [worker], pool, db, redis },
      { process: h.proc, exit: h.exit, log: silentLog },
    );
    h.proc.emit("SIGTERM");
    expect(await h.exited).toBe(1);
    expect(h.order).toEqual(["worker", "pool", "db", "redis"]);
  });

  it("does not wait past graceMs for a stuck worker", async () => {
    const h = harness();
    const worker = h.closer(
      "worker",
      (() => {
        let first = true;
        return () => {
          if (first) {
            first = false;
            return new Promise<void>(() => {}); // drain never finishes
          }
          return Promise.resolve();
        };
      })(),
    );
    const pool = h.closer("pool");
    const db = { $client: { end: async () => {} } };
    const redis = { quit: async () => "OK", disconnect() {} };
    const started = Date.now();
    installGracefulShutdown(
      { workers: [worker], pool, db, redis },
      { graceMs: 300, process: h.proc, exit: h.exit, log: silentLog },
    );
    h.proc.emit("SIGTERM");
    await h.exited;
    expect(Date.now() - started).toBeLessThan(300 + 100);
    // The pool is closed early to abort in-flight work, then the worker is force-closed.
    expect(h.order).toEqual(["worker", "pool", "worker(force)"]);
  });
});

describe.skipIf(!stackConfigured() || skipEngineTests)("SIGTERM during analysis", () => {
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

  it("SIGTERM loses nothing", async () => {
    // A long game: 300k nodes per position keeps the engine busy well past the grace period.
    const pgn =
      "1. e4 c5 2. Nf3 d6 3. d4 cxd4 4. Nxd4 Nf6 5. Nc3 a6 6. Be3 e5 7. Nb3 Be6 8. f3 Be7 " +
      "9. Qd2 O-O 10. O-O-O Nbd7 11. g4 b5 12. g5 b4 13. Ne2 Ne8 14. f4 a5 15. f5 a4 *";
    const { id } = await addGame(db, pgn, "sigterm");
    const pool = await EnginePool.create({ size: 1, path: stockfishPath });
    const pids = pool.pids();
    expect(pids).toHaveLength(1);
    worker = createAnalyseWorker(
      { connection, prefix, workerOptions: FAST_WORKER, log: silentLog },
      { db, pool, cache: createDbEvalCache(db) },
      1,
    );
    await queues.enqueueAnalyse(id);
    await waitFor(async () => (await getGame(db, id))?.analysisStatus === "analysing", {
      what: "analysis to start",
    });

    const proc = new EventEmitter();
    const exited = new Promise<number>((resolve) => proc.once("exit-called", resolve));
    // A separate Redis connection for the handles, so the test's queue connection stays open.
    const redis = createRedisConnection(redisUrl);
    const graceMs = 2000;
    const started = Date.now();
    installGracefulShutdown(
      // The test keeps the db open to inspect it; closing a real postgres client is covered above.
      { workers: [worker], pool, db: { $client: { end: async () => {} } }, redis },
      { graceMs, process: proc, exit: (c) => proc.emit("exit-called", c), log: silentLog },
    );
    proc.emit("SIGTERM");
    const code = await exited;
    expect(Date.now() - started).toBeLessThan(graceMs + 500);
    expect(code).toBe(0);
    worker = undefined;

    const g = await getGame(db, id);
    expect(["done", "pending"]).toContain(g?.analysisStatus);
    if (g?.analysisStatus === "pending") {
      // Left for the retry path: the job is still live in BullMQ (delayed retry or waiting).
      const state = await (await queues.analyseGame.getJob(analyseJobId(id)))?.getState();
      expect(["delayed", "waiting", "active"]).toContain(state);
    }
    for (const pid of pids) expect(isDead(pid)).toBe(true);
  }, 20_000);
});

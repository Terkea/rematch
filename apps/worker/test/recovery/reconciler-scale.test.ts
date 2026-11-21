import { type Db, games, linkedAccounts } from "@ci/db";
import type { Redis } from "ioredis";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { reconcile } from "../../src/jobs/reconcile.js";
import { createQueues, createRedisConnection, type Queues } from "../../src/queue/queues.js";
import { getWorkerTestDb } from "../db.js";
import { redisUrl, silentLog, stackConfigured, uniquePrefix } from "./helpers.js";

const N = 2500;

describe.skipIf(!stackConfigured())("reconciler at scale", () => {
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

  it("sweeps 2,500 pending games in pages and leaves exactly one job per game", async () => {
    const [acct] = await db
      .insert(linkedAccounts)
      .values({ platform: "lichess", username: "bulk", displayName: "bulk" })
      .returning();
    const old = new Date(Date.now() - 10 * 60_000);
    const rows = Array.from({ length: N }, (_, i) => ({
      accountId: acct?.id as string,
      platform: "lichess" as const,
      platformGameId: `bulk-${i}`,
      pgn: "1. e4 *",
      playedAt: new Date(Date.UTC(2026, 0, 1, 0, 0, i)),
      timeClass: "blitz" as const,
      rated: true,
      playerColor: "white" as const,
      whiteName: "bulk",
      blackName: "x",
      result: "*" as const,
      pendingSince: old,
    }));
    for (let i = 0; i < rows.length; i += 500)
      await db.insert(games).values(rows.slice(i, i + 500));
    const ids = (await db.select({ id: games.id }).from(games)).map((r) => r.id);
    expect(ids).toHaveLength(N);

    // A few already have a live job: they must not get a second one.
    for (const id of ids.slice(0, 10)) await queues.enqueueAnalyse(id);

    const started = Date.now();
    const res = await reconcile({ db, queues, log: silentLog }, { pageSize: 1000 });
    expect(res.requeuedPending).toBe(N - 10);
    expect(Date.now() - started).toBeLessThan(15_000);

    expect(await queues.analyseGame.getWaitingCount()).toBe(N);
    const liveness = await queues.analyseLiveness(ids);
    expect(liveness.every((l) => l === "live")).toBe(true);

    expect((await reconcile({ db, queues, log: silentLog })).requeuedPending).toBe(0);
    expect(await queues.analyseGame.getWaitingCount()).toBe(N);
  }, 30_000);
});

import { type Db, userEvents } from "@ci/db";
import { and, asc, eq, gt } from "drizzle-orm";
import type { Redis } from "ioredis";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { publishEvent, userEventsChannel } from "../../src/events/publish.js";
import { createRedisConnection } from "../../src/queue/queues.js";
import { getWorkerTestDb } from "../db.js";
import { insertAccount } from "../jobs/helpers.js";
import { redisUrl, stackConfigured, waitFor } from "./helpers.js";

describe.skipIf(!stackConfigured())("user events", () => {
  let db: Db;
  let redis: Redis;
  let subscriber: Redis;

  beforeAll(async () => {
    db = (await getWorkerTestDb()) as Db;
    redis = createRedisConnection(redisUrl);
    subscriber = createRedisConnection(redisUrl);
  });

  afterAll(async () => {
    redis.disconnect();
    subscriber.disconnect();
    await db.$client.end();
  });

  it("events are ordered and replayable", async () => {
    const acct = await insertAccount(db, "eventful");
    expect(userEventsChannel(acct.id)).toBe(`user-events:${acct.id}`);

    const received: { id: number; type: string; payload: unknown }[] = [];
    subscriber.on("message", (_channel, message) => {
      received.push(JSON.parse(message) as { id: number; type: string; payload: unknown });
    });
    await subscriber.subscribe(userEventsChannel(acct.id));

    const deps = { db, redis };
    const ids = [
      await publishEvent(deps, acct.id, "import.progress", { imported: 50 }),
      await publishEvent(deps, acct.id, "game.analysed", { gameId: "g1", accuracy: 81.5 }),
      await publishEvent(deps, acct.id, "job.failed", { kind: "analyse-game", reason: "x" }),
    ];
    expect(ids[0]).toBeLessThan(ids[1] as number);
    expect(ids[1]).toBeLessThan(ids[2] as number);

    await waitFor(() => received.length === 3, { what: "3 pub/sub messages" });
    expect(received.map((r) => r.id)).toEqual(ids);
    expect(received.map((r) => r.type)).toEqual(["import.progress", "game.analysed", "job.failed"]);
    expect(received[1]?.payload).toEqual({ gameId: "g1", accuracy: 81.5 });

    // Every published id is already durable, so a reconnecting client can replay after it.
    const replay = await db
      .select()
      .from(userEvents)
      .where(and(eq(userEvents.accountId, acct.id), gt(userEvents.id, ids[0] as number)))
      .orderBy(asc(userEvents.id));
    expect(replay.map((r) => r.id)).toEqual(ids.slice(1));
    expect(replay.map((r) => r.type)).toEqual(["game.analysed", "job.failed"]);
  });

  it("keeps the row when Redis publish fails", async () => {
    const acct = await insertAccount(db, "offline");
    const broken = {
      publish: async () => {
        throw new Error("redis down");
      },
    };
    const id = await publishEvent({ db, redis: broken }, acct.id, "import.progress", {
      imported: 1,
    });
    const [row] = await db.select().from(userEvents).where(eq(userEvents.id, id));
    expect(row?.accountId).toBe(acct.id);
  });
});

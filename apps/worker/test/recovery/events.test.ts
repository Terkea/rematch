import { type Db, insertUserEvent, listEventsAfter, userEvents } from "@ci/db";
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

  it("replay never skips a lower id that commits after a higher one was read", async () => {
    const acct = await insertAccount(db, "ordered");
    let releaseA!: () => void;
    const gateA = new Promise<void>((r) => (releaseA = r));
    let idA = 0;
    let aInserted!: () => void;
    const aHasId = new Promise<void>((r) => (aInserted = r));
    // Transaction A takes an id, then stays open.
    const txA = db.transaction(async (tx) => {
      idA = await insertUserEvent(tx, { accountId: acct.id, type: "a", payload: {} });
      aInserted();
      await gateA;
    });
    await aHasId;
    // Transaction B starts later and would commit first.
    let idB = 0;
    const txB = (async () => {
      idB = await insertUserEvent(db, { accountId: acct.id, type: "b", payload: {} });
    })();

    // A reader polls repeatedly, advancing its cursor like an SSE client.
    const seen: number[] = [];
    let cursor = 0;
    const poll = async () => {
      for (const e of await listEventsAfter(db, [acct.id], cursor, 100)) {
        seen.push(e.id);
        cursor = e.id;
      }
    };
    for (let i = 0; i < 5; i++) {
      await poll();
      await new Promise((r) => setTimeout(r, 20));
    }
    // Nothing is visible while A is open: B is ordered behind A rather than slipping past it.
    expect(seen).toEqual([]);
    releaseA();
    await Promise.all([txA, txB]);
    await poll();
    await poll();
    expect(idA).toBeLessThan(idB);
    expect(seen).toEqual([idA, idB]);
  });

  it("listEventsAfter filters by account, orders by id and honours the limit", async () => {
    const one = await insertAccount(db, "list-one");
    const two = await insertAccount(db, "list-two");
    const ids: number[] = [];
    for (const a of [one, two, one, two, one]) {
      ids.push(await insertUserEvent(db, { accountId: a.id, type: "t", payload: { a: a.id } }));
    }
    const mine = await listEventsAfter(db, [one.id], 0, 10);
    expect(mine.map((e) => e.id)).toEqual([ids[0], ids[2], ids[4]]);
    expect(
      (await listEventsAfter(db, [one.id, two.id], ids[1] as number, 2)).map((e) => e.id),
    ).toEqual([ids[2], ids[3]]);
    expect(await listEventsAfter(db, [], 0, 10)).toEqual([]);
  });
});

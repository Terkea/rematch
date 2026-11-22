import { sql } from "drizzle-orm";
import postgres from "postgres";
import { afterAll, describe, expect, it } from "vitest";
import { createDb } from "../src/client.js";
import {
  insertUserEvent,
  listEventsAfter,
  MAX_NOTIFY_BYTES,
  notificationJson,
  oldestEventId,
  USER_EVENTS_CHANNEL,
  userEventsChannel,
} from "../src/events.js";
import { linkedAccounts, userEvents } from "../src/schema.js";
import { assertSafeTestDatabase } from "../src/test-guard.js";

const url = assertSafeTestDatabase(process.env.DATABASE_URL_TEST, process.env.DATABASE_URL);
const db = createDb(url);
const listener = postgres(url, { onnotice: () => {} });

afterAll(async () => {
  await listener.end();
  await db.$client.end();
});

async function makeAccount(username: string) {
  const [row] = await db
    .insert(linkedAccounts)
    .values({ platform: "lichess", username, displayName: username })
    .returning();
  if (!row) throw new Error("no account");
  return row;
}

describe("user events (Task 2 helpers, ADR 0034 delivery)", () => {
  it("userEventsChannel keeps the worker's Redis naming", () => {
    expect(userEventsChannel("x")).toBe("user-events:x");
    expect(USER_EVENTS_CHANNEL).toBe("user_events");
  });

  it("listEventsAfter filters by account, excludes the cursor and defaults to 500", async () => {
    const a1 = await makeAccount("one");
    const a2 = await makeAccount("two");
    const first = await insertUserEvent(db, { accountId: a1.id, type: "t", payload: { n: 1 } });
    const second = await insertUserEvent(db, { accountId: a2.id, type: "t", payload: { n: 2 } });
    const third = await insertUserEvent(db, { accountId: a1.id, type: "t", payload: { n: 3 } });
    expect((await listEventsAfter(db, [a1.id], 0)).map((e) => e.id)).toEqual([first, third]);
    expect((await listEventsAfter(db, [a1.id, a2.id], first)).map((e) => e.id)).toEqual([
      second,
      third,
    ]);
    const many = Array.from({ length: 501 }, () => ({ accountId: a2.id, type: "t", payload: {} }));
    await db.insert(userEvents).values(many);
    expect(await listEventsAfter(db, [a2.id], second)).toHaveLength(500);
  });

  it("oldestEventId is the smallest id for the accounts, or null", async () => {
    const a1 = await makeAccount("old1");
    const a2 = await makeAccount("old2");
    expect(await oldestEventId(db, [a1.id])).toBeNull();
    expect(await oldestEventId(db, [])).toBeNull();
    const first = await insertUserEvent(db, { accountId: a2.id, type: "t", payload: {} });
    const second = await insertUserEvent(db, { accountId: a1.id, type: "t", payload: {} });
    expect(await oldestEventId(db, [a1.id])).toBe(second);
    expect(await oldestEventId(db, [a1.id, a2.id])).toBe(first);
  });

  it("insertUserEvent sends the Rust NOTIFY body on commit (ADR 0034)", async () => {
    const acc = await makeAccount("notify");
    const received: string[] = [];
    const { unlisten } = await listener.listen(USER_EVENTS_CHANNEL, (p) => {
      if (JSON.parse(p).accountId === acc.id) received.push(p);
    });
    try {
      const id = await insertUserEvent(db, {
        accountId: acc.id,
        type: "game.analysed",
        payload: { gameId: "g1", accuracy: 81.5 },
      });
      await expect.poll(() => received.length).toBe(1);
      const body = received[0] as string;
      const [row] = await db.execute<{ created: string }>(
        sql`select to_char(created_at at time zone 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.US"Z"') as created
            from user_events where id = ${id}`,
      );
      expect(body).toBe(
        `{"id":${id},"accountId":"${acc.id}","type":"game.analysed","createdAt":"${row?.created}","payload":{"gameId":"g1","accuracy":81.5}}`,
      );
      expect(row?.created).toMatch(/^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d\.\d{6}Z$/);
    } finally {
      await unlisten();
    }
  });

  it("a rolled-back insert notifies nobody", async () => {
    const acc = await makeAccount("rollback");
    const received: string[] = [];
    const { unlisten } = await listener.listen(USER_EVENTS_CHANNEL, (p) => {
      if (JSON.parse(p).accountId === acc.id) received.push(p);
    });
    try {
      await db
        .transaction(async (tx) => {
          await insertUserEvent(tx, { accountId: acc.id, type: "t", payload: {} });
          throw new Error("roll back");
        })
        .catch(() => {});
      await insertUserEvent(db, { accountId: acc.id, type: "after", payload: {} });
      await expect.poll(() => received.length).toBe(1);
      expect(JSON.parse(received[0] as string).type).toBe("after");
    } finally {
      await unlisten();
    }
  });

  it("drops an oversized payload and marks it truncated, like Rust", () => {
    const stored = { id: 7, createdAt: "2026-11-21T10:00:05.123456Z" };
    const small = notificationJson(stored, "a", "t", { n: 1 });
    expect(small).toBe(
      '{"id":7,"accountId":"a","type":"t","createdAt":"2026-11-21T10:00:05.123456Z","payload":{"n":1}}',
    );
    // A user-owned event carries userId right after accountId (serde field order in Rust).
    expect(notificationJson(stored, "a", "t", { n: 1 }, "u")).toBe(
      '{"id":7,"accountId":"a","userId":"u","type":"t","createdAt":"2026-11-21T10:00:05.123456Z","payload":{"n":1}}',
    );
    const big = notificationJson(stored, "a", "t", { s: "é".repeat(MAX_NOTIFY_BYTES) });
    expect(big).toBe(
      '{"id":7,"accountId":"a","type":"t","createdAt":"2026-11-21T10:00:05.123456Z","truncated":true}',
    );
  });
});

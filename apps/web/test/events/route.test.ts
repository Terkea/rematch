import { sql } from "drizzle-orm";
import { afterAll, afterEach, beforeEach, describe, expect, it } from "vitest";
import { createTestSession, resetAuthTables, webTestDb } from "../helpers/session";
import {
  type Frame,
  frameReader,
  insertAndPublish,
  linkAccount,
  pruneThrough,
  resetEventTables,
} from "./helpers";

const db = webTestDb();
const route = await import("../../src/app/api/events/route");
const { getEventHub, resetEventHub } = await import("../../src/server/events/hub");
const sse = await import("../../src/server/events/sse");
const { userAccountIds } = await import("../../src/server/events/user-accounts");

const ORIGIN = "http://localhost:3100";
const defaults = { ...sse.eventStreamLimits };

let me: { cookie: string; userId: string; accountId: string };
let other: { cookie: string; userId: string; accountId: string };
const open: AbortController[] = [];

async function signedIn(birthMonth: string | null = "1990-01") {
  const { user, cookie } = await createTestSession(db, { birthMonth });
  const accountId = await linkAccount(db, user.id);
  return { cookie, userId: user.id, accountId };
}

function request(
  query = "",
  opts: { cookie?: string | null; lastEventId?: string; origin?: string; fetchSite?: string } = {},
) {
  const controller = new AbortController();
  open.push(controller);
  const headers = new Headers();
  const cookie = opts.cookie === undefined ? me.cookie : opts.cookie;
  if (cookie) headers.set("cookie", cookie);
  if (opts.lastEventId !== undefined) headers.set("last-event-id", opts.lastEventId);
  if (opts.origin) headers.set("origin", opts.origin);
  if (opts.fetchSite) headers.set("sec-fetch-site", opts.fetchSite);
  const req = new Request(`${ORIGIN}/api/events${query}`, { headers, signal: controller.signal });
  return { req, controller };
}

async function stream(opts: Parameters<typeof request>[1] = {}, query = "") {
  const { req, controller } = request(query, opts);
  const res = await route.GET(req);
  // Only a stream is read frame by frame; an error response keeps its JSON body readable.
  const frames = res.status === 200 ? frameReader(res) : (undefined as never);
  return { res, frames, controller };
}

/** Event frames until one with `untilId` arrives (or the stream ends). */
async function readUntil(frames: ReturnType<typeof frameReader>, untilId: number) {
  const seen: Frame[] = [];
  for (;;) {
    const frame = await frames.nextEvent();
    if (!frame) return seen;
    seen.push(frame);
    if (frame.id === String(untilId)) return seen;
  }
}

beforeEach(async () => {
  await resetAuthTables(db);
  await resetEventTables(db);
  me = await signedIn();
  other = await signedIn();
});

afterEach(async () => {
  sse.setBeforeReplayHook(undefined);
  Object.assign(sse.eventStreamLimits, defaults);
  for (const c of open.splice(0)) c.abort();
  await expect.poll(() => getEventHub().channelCount()).toBe(0);
});

afterAll(async () => {
  await resetEventHub();
});

describe("GET /api/events (SSE)", () => {
  it("opens with the SSE headers and retry: 3000", async () => {
    const { res, frames } = await stream();
    expect(res.status).toBe(200);
    expect(res.headers.get("content-type")).toBe("text/event-stream; charset=utf-8");
    expect(res.headers.get("cache-control")).toBe("no-cache, no-transform");
    expect(res.headers.get("x-accel-buffering")).toBe("no");
    expect(await frames.next()).toEqual({ retry: "3000" });
  });

  it("replay plus live race delivers each id once in order", async () => {
    const ids: number[] = [];
    for (let i = 1; i <= 5; i++) ids.push(await insertAndPublish(db, me.accountId, "t", { i }));
    let e6 = 0;
    sse.setBeforeReplayHook(async () => {
      e6 = await insertAndPublish(db, me.accountId, "t", { i: 6 });
    });
    const { frames } = await stream({ lastEventId: String(ids[1]) });
    const seen = await readUntil(frames, (ids[4] as number) + 1);
    expect(e6).toBe((ids[4] as number) + 1);
    expect(seen.map((f) => Number(f.id))).toEqual([ids[2], ids[3], ids[4], e6]);
    // The buffered live copy of e6 is dropped, so nothing else follows.
    expect(await frames.nextWithin(200)).toBe("timeout");
  });

  it("delivers live events after replay", async () => {
    const e1 = await insertAndPublish(db, me.accountId, "t");
    const { frames } = await stream({ lastEventId: "0" });
    expect((await frames.nextEvent())?.id).toBe(String(e1));
    const e7 = await insertAndPublish(db, me.accountId, "game.analysed", { gameId: "g7" });
    const frame = await frames.nextEvent();
    expect(frame).toEqual({
      id: String(e7),
      event: "game.analysed",
      data: JSON.stringify({ accountId: me.accountId, payload: { gameId: "g7" } }),
    });
  });

  it("does not leak other users' events, live or replayed", async () => {
    const theirs = await insertAndPublish(db, other.accountId, "t", { secret: 1 });
    const mine = await insertAndPublish(db, me.accountId, "t");
    const { frames } = await stream({ lastEventId: "0" });
    expect((await frames.nextEvent())?.id).toBe(String(mine));
    await insertAndPublish(db, other.accountId, "t", { secret: 2 });
    expect(await frames.nextWithin(300)).toBe("timeout");
    // And the other user's own stream does get them.
    const theirStream = await stream({ cookie: other.cookie, lastEventId: "0" });
    expect((await theirStream.frames.nextEvent())?.id).toBe(String(theirs));
    expect(theirs).toBeLessThan(mine);
  });

  it("an account linked by another user only reaches that user", async () => {
    // Two users may each hold an unverified link to the same account (ADR-WEB-3): both see it.
    await db.execute(sql`insert into account_links (user_id, linked_account_id, method)
                         values (${other.userId}, ${me.accountId}, 'location_code')`);
    expect(await userAccountIds(db, other.userId)).toHaveLength(2);
    expect(await userAccountIds(db, me.userId)).toEqual([me.accountId]);
  });

  it("a user-owned event of a shared account reaches only its user, live and on replay", async () => {
    // Two users linked to one account (ADR-WEB-3); chat events carry user_id (migration
    // 20261121030400, ADR-R3-8): they are for that user only.
    await db.execute(sql`insert into account_links (user_id, linked_account_id, method)
                         values (${other.userId}, ${me.accountId}, 'location_code')`);
    const shared = await insertAndPublish(db, me.accountId, "import.progress", { n: 1 });
    const secret = await insertAndPublish(
      db,
      me.accountId,
      "chat.chunk",
      { text: "s1" },
      me.userId,
    );
    const theirsOwn = await insertAndPublish(db, me.accountId, "chat.chunk", {}, other.userId);

    // Replay: user 2 gets the account event and their own, never user 1's.
    const theirReplay = await stream({ cookie: other.cookie, lastEventId: "0" });
    expect((await theirReplay.frames.nextEvent())?.id).toBe(String(shared));
    expect((await theirReplay.frames.nextEvent())?.id).toBe(String(theirsOwn));
    // User 1's replay has theirs, not user 2's.
    const myReplay = await stream({ lastEventId: "0" });
    expect((await myReplay.frames.nextEvent())?.id).toBe(String(shared));
    expect((await myReplay.frames.nextEvent())?.id).toBe(String(secret));

    // Live: a new user-1 event reaches user 1 only; the next account event reaches both.
    const live = await insertAndPublish(db, me.accountId, "chat.chunk", { text: "s2" }, me.userId);
    const after = await insertAndPublish(db, me.accountId, "import.progress", { n: 2 });
    expect((await myReplay.frames.nextEvent())?.id).toBe(String(live));
    expect((await myReplay.frames.nextEvent())?.id).toBe(String(after));
    expect((await theirReplay.frames.nextEvent())?.id).toBe(String(after));
    expect(await theirReplay.frames.nextWithin(300)).toBe("timeout");

    // A truncated NOTIFY (payload fetched by id) is filtered the same way.
    const big = await insertAndPublish(
      db,
      me.accountId,
      "chat.chunk",
      { text: "x".repeat(9000) },
      me.userId,
    );
    expect((await myReplay.frames.nextEvent())?.id).toBe(String(big));
    expect(await theirReplay.frames.nextWithin(300)).toBe("timeout");

    // hello: user 1's event does not move user 2's head.
    const hello = await stream({ cookie: other.cookie });
    expect(await hello.frames.nextEvent()).toEqual({
      id: String(after),
      event: "hello",
      data: "{}",
    });
  });

  it("stale cursor emits reset", async () => {
    const ids: number[] = [];
    for (let i = 1; i <= 5; i++) ids.push(await insertAndPublish(db, me.accountId, "t"));
    await pruneThrough(db, ids[2] as number);
    const { frames } = await stream({ lastEventId: String(ids[0]) });
    const first = await frames.nextEvent();
    expect(first?.event).toBe("reset");
    expect(first?.data).toBe("{}");
    // The stream continues from the newest id, so a reconnect does not reset again.
    expect(first?.id).toBe(String(ids[4]));
    const live = await insertAndPublish(db, me.accountId, "t");
    expect((await frames.nextEvent())?.id).toBe(String(live));
  });

  it("a cursor at the watermark is not stale", async () => {
    const ids: number[] = [];
    for (let i = 1; i <= 4; i++) ids.push(await insertAndPublish(db, me.accountId, "t"));
    await pruneThrough(db, ids[1] as number);
    const { frames } = await stream({ lastEventId: String(ids[1]) });
    expect((await frames.nextEvent())?.id).toBe(String(ids[2]));
  });

  it("no cursor sends hello with the current max id", async () => {
    await insertAndPublish(db, me.accountId, "t");
    const mine = await insertAndPublish(db, me.accountId, "t");
    // A later event of another user does not move (or reveal) this user's head.
    await insertAndPublish(db, other.accountId, "t");
    const { frames } = await stream();
    expect(await frames.nextEvent()).toEqual({ id: String(mine), event: "hello", data: "{}" });
    const live = await insertAndPublish(db, me.accountId, "t");
    expect((await frames.nextEvent())?.id).toBe(String(live));
  });

  it("hello is 0 for a user without events, ?since= is a cursor", async () => {
    const { frames } = await stream();
    expect((await frames.nextEvent())?.id).toBe("0");
    const e1 = await insertAndPublish(db, me.accountId, "t");
    const e2 = await insertAndPublish(db, me.accountId, "t");
    const since = await stream({}, `?since=${e1}`);
    expect((await since.frames.nextEvent())?.id).toBe(String(e2));
  });

  it("replays past one page", async () => {
    sse.eventStreamLimits.replayPageSize = 3;
    const ids: number[] = [];
    for (let i = 0; i < 7; i++) ids.push(await insertAndPublish(db, me.accountId, "t"));
    const { frames } = await stream({ lastEventId: "0" });
    const seen = await readUntil(frames, ids[6] as number);
    expect(seen.map((f) => Number(f.id))).toEqual(ids);
  });

  it("sends a ': ping' heartbeat", async () => {
    sse.eventStreamLimits.heartbeatMs = 30;
    const { frames } = await stream();
    for (;;) {
      const frame = await frames.next();
      if (frame?.comment === "ping") break;
      expect(frame).not.toBeNull();
    }
  });

  it("closes a stream at its maximum age", async () => {
    sse.eventStreamLimits.maxAgeMs = 100;
    const { frames } = await stream();
    for (;;) if ((await frames.next()) === null) break;
    await expect.poll(() => getEventHub().channelCount()).toBe(0);
  });

  it("closes a stream whose client stops reading", async () => {
    sse.eventStreamLimits.maxQueuedBytes = 2000;
    const { frames } = await stream({ lastEventId: "0" });
    for (let i = 0; i < 40; i++)
      await insertAndPublish(db, me.accountId, "t", { pad: "x".repeat(80) });
    await expect.poll(() => getEventHub().channelCount()).toBe(0);
    let count = 0;
    while ((await frames.nextEvent()) !== null) count++;
    expect(count).toBeLessThan(40);
  });

  it("caps concurrent streams per user", async () => {
    sse.eventStreamLimits.maxStreamsPerUser = 2;
    const a = await stream();
    await stream();
    const third = await stream();
    expect(third.res.status).toBe(429);
    expect(await third.res.json()).toEqual({ error: "too_many_streams" });
    // Another user is not affected.
    expect((await stream({ cookie: other.cookie })).res.status).toBe(200);
    a.controller.abort();
    await expect.poll(async () => (await stream()).res.status).toBe(200);
  });

  it("replays from its cursor after the LISTEN connection drops", async () => {
    const { frames } = await stream({ lastEventId: "0" });
    const e1 = await insertAndPublish(db, me.accountId, "t");
    expect((await frames.nextEvent())?.id).toBe(String(e1));
    await db.execute(sql`select pg_terminate_backend(pid) from pg_stat_activity
                         where query ilike 'listen%' and pid <> pg_backend_pid()`);
    const e2 = await insertAndPublish(db, me.accountId, "t");
    const e3 = await insertAndPublish(db, me.accountId, "t");
    const seen = await readUntil(frames, e3);
    expect(seen.map((f) => Number(f.id))).toEqual([e2, e3]);
  });

  it("abort unsubscribes", async () => {
    const { controller, frames } = await stream();
    await frames.nextEvent();
    expect(getEventHub().channelCount()).toBe(1);
    controller.abort();
    await expect.poll(() => getEventHub().channelCount()).toBe(0);
  });

  it("401 without session", async () => {
    const { res } = await stream({ cookie: null });
    expect(res.status).toBe(401);
  });

  it("403 while the age is unknown", async () => {
    const pending = await signedIn(null);
    const { res } = await stream({ cookie: pending.cookie });
    expect(res.status).toBe(403);
    expect(await res.json()).toEqual({ error: "age_required" });
  });

  it("403 for a cross-site navigation without an Origin header", async () => {
    expect((await stream({ fetchSite: "cross-site" })).res.status).toBe(403);
    expect((await stream({ fetchSite: "same-origin" })).res.status).toBe(200);
  });

  it("403 for a foreign origin, 400 for a malformed cursor", async () => {
    expect((await stream({ origin: "https://evil.example" })).res.status).toBe(403);
    const bad = await stream({ lastEventId: "-1" });
    expect(bad.res.status).toBe(400);
    expect(await bad.res.json()).toEqual({ error: "invalid_cursor" });
    expect(getEventHub().channelCount()).toBe(0);
  });
});

describe("GET /api/events?after= (polling)", () => {
  async function poll(after: string, cookie = me.cookie) {
    const { req } = request(`?after=${after}`, { cookie });
    return route.GET(req);
  }

  it("returns the events after the cursor", async () => {
    const ids: number[] = [];
    for (let i = 1; i <= 5; i++) ids.push(await insertAndPublish(db, me.accountId, "t", { i }));
    await insertAndPublish(db, other.accountId, "t");
    const res = await poll(String(ids[2]));
    expect(res.status).toBe(200);
    expect(res.headers.get("cache-control")).toBe("no-store");
    const body = await res.json();
    expect(body).toEqual({
      events: [
        { id: ids[3], accountId: me.accountId, type: "t", payload: { i: 4 } },
        { id: ids[4], accountId: me.accountId, type: "t", payload: { i: 5 } },
      ],
      lastId: ids[4],
      reset: false,
    });
  });

  it("keeps the cursor when nothing is new, caps a page at 500", async () => {
    expect(await (await poll("0")).json()).toEqual({ events: [], lastId: 0, reset: false });
    await db.execute(sql`insert into user_events (account_id, type, payload)
                         select ${me.accountId}, 't', '{}' from generate_series(1, 501)`);
    const body = await (await poll("0")).json();
    expect(body.events).toHaveLength(500);
    expect(body.lastId).toBe(500);
  });

  it("reports reset for a stale cursor and moves to the head", async () => {
    const ids: number[] = [];
    for (let i = 1; i <= 5; i++) ids.push(await insertAndPublish(db, me.accountId, "t"));
    await pruneThrough(db, ids[2] as number);
    expect(await (await poll(String(ids[0]))).json()).toEqual({
      events: [],
      lastId: ids[4],
      reset: true,
    });
  });

  it("never returns another user's user-owned events of a shared account", async () => {
    await db.execute(sql`insert into account_links (user_id, linked_account_id, method)
                         values (${other.userId}, ${me.accountId}, 'location_code')`);
    const shared = await insertAndPublish(db, me.accountId, "t", { n: 1 });
    const secret = await insertAndPublish(db, me.accountId, "chat.chunk", { text: "s" }, me.userId);
    const theirs = (await (await poll("0", other.cookie)).json()) as {
      events: { id: number }[];
      lastId: number;
    };
    expect(theirs.events.map((e) => e.id)).toEqual([shared]);
    expect(theirs.lastId).toBe(shared);
    const mine = (await (await poll("0")).json()) as { events: { id: number }[] };
    expect(mine.events.map((e) => e.id)).toEqual([shared, secret]);
  });

  it("refuses unauthenticated and malformed requests", async () => {
    expect((await poll("0", "")).status).toBe(401);
    expect((await poll("abc")).status).toBe(400);
  });
});

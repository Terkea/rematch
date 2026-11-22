import { sql } from "drizzle-orm";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// The stream's replay reads go through this wrapper, so a test can act between a replay page
// being read and the sync flushing its buffer: the window of the T22 re-review race.
let afterPage: (() => Promise<void>) | undefined;
vi.mock("@ci/db", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@ci/db")>();
  return {
    ...actual,
    listEventsPage: async (...args: Parameters<typeof actual.listEventsPage>) => {
      const page = await actual.listEventsPage(...args);
      const hook = afterPage;
      afterPage = undefined;
      if (hook) await hook();
      return page;
    },
  };
});

const { createTestSession, resetAuthTables, webTestDb } = await import("../helpers/session");
const { frameReader, insertAndPublish, linkAccount, resetEventTables } = await import("./helpers");
const { createEventHub } = await import("../../src/server/events/hub");
const { openEventStream } = await import("../../src/server/events/sse");

const db = webTestDb();
let accountId: string;
let userId: string;
const aborts: AbortController[] = [];

beforeEach(async () => {
  await resetAuthTables(db);
  await resetEventTables(db);
  const { user } = await createTestSession(db, { birthMonth: "1990-01" });
  userId = user.id;
  accountId = await linkAccount(db, user.id);
});

afterEach(() => {
  afterPage = undefined;
  for (const a of aborts.splice(0)) a.abort();
});

describe("LISTEN reconnect during a running sync", () => {
  it("never sends a buffered live event ahead of a gap the queued resync must close", async () => {
    let notify: ((p: string) => void) | undefined;
    let relisten: (() => void) | undefined;
    const listen = vi.fn(async (_c: string, n: (p: string) => void, listened: () => void) => {
      notify = n;
      relisten = listened;
      listened();
      return { unlisten: async () => {} };
    });
    const hub = createEventHub({ listen, fetchEvent: async () => null });
    const e1 = await insertAndPublish(db, accountId, "t");

    let gap = 0;
    let live = 0;
    afterPage = async () => {
      // The first sync has read its page (e1). Now LISTEN drops: `gap` is only stored...
      gap = await insertAndPublish(db, accountId, "t");
      // ...LISTEN is rebuilt (a resync is queued), and `live` arrives over the new LISTEN.
      relisten?.();
      live = await insertAndPublish(db, accountId, "t");
      notify?.(JSON.stringify({ id: live, accountId, type: "t", createdAt: "", payload: {} }));
      await new Promise((r) => setTimeout(r, 20));
    };

    const controller = new AbortController();
    aborts.push(controller);
    const res = await openEventStream({
      db,
      hub,
      userId,
      accountIds: [accountId],
      cursor: 0,
      signal: controller.signal,
    });
    const frames = frameReader(res);
    const ids: number[] = [];
    for (;;) {
      const f = await frames.nextWithin(2000);
      if (f === "timeout" || f === null) break;
      if (!f.event) continue;
      ids.push(Number(f.id));
      if (ids.length === 3) break;
    }
    expect(ids).toEqual([e1, gap, live]);
    expect(await frames.nextWithin(200)).toBe("timeout");
  });

  it("a LISTEN rebuild landing as a sync finishes still runs the resync (no stall)", async () => {
    // Review Minor 1: `syncing` used to clear one microtask after the sync set replaying=false.
    // A rebuild in that gap left the stream buffering with no sync to flush it. Fire the rebuild
    // at every microtask offset around the sync's end; each stream must still deliver live.
    const deferred = (k: number, fn: () => void) => {
      if (k === 0) fn();
      else queueMicrotask(() => deferred(k - 1, fn));
    };
    for (let k = 0; k < 16; k++) {
      let notify: ((p: string) => void) | undefined;
      let relisten: (() => void) | undefined;
      const listen = vi.fn(async (_c: string, n: (p: string) => void, listened: () => void) => {
        notify = n;
        relisten = listened;
        listened();
        return { unlisten: async () => {} };
      });
      const hub = createEventHub({ listen, fetchEvent: async () => null });
      const e1 = await insertAndPublish(db, accountId, "t");
      afterPage = async () => {
        deferred(k, () => relisten?.());
      };
      const controller = new AbortController();
      aborts.push(controller);
      const res = await openEventStream({
        db,
        hub,
        userId,
        accountIds: [accountId],
        cursor: e1 - 1,
        signal: controller.signal,
      });
      const frames = frameReader(res);
      expect((await frames.nextEvent())?.id).toBe(String(e1));
      await new Promise((r) => setTimeout(r, 30));
      const live = await insertAndPublish(db, accountId, "t");
      notify?.(JSON.stringify({ id: live, accountId, type: "t", createdAt: "", payload: {} }));
      const next = await frames.nextWithin(1500);
      expect(next === "timeout" ? `stalled at offset ${k}` : next?.id).toBe(String(live));
      controller.abort();
      await hub.close();
    }
  });

  it("user 2 opening with since= never replays user 1's own events, in either replay", async () => {
    // Two users on one account; a user-owned event (user_events.user_id) is for that user only.
    const { user: other } = await createTestSession(db, { birthMonth: "1990-01" });
    await db.execute(sql`insert into account_links (user_id, linked_account_id, method)
                         values (${other.id}, ${accountId}, 'location_code')`);
    let relisten: (() => void) | undefined;
    const listen = vi.fn(async (_c: string, _n: (p: string) => void, listened: () => void) => {
      relisten = listened;
      listened();
      return { unlisten: async () => {} };
    });
    const hub = createEventHub({ listen, fetchEvent: async () => null });
    const before = await insertAndPublish(db, accountId, "t");
    const secret1 = await insertAndPublish(db, accountId, "chat.chunk", { s: 1 }, userId);

    let gap = 0;
    let secret2 = 0;
    afterPage = async () => {
      // During user 2's first replay: user 1's event and an account event are stored while
      // LISTEN is down, then LISTEN is rebuilt, so the pending replay must read them.
      secret2 = await insertAndPublish(db, accountId, "chat.chunk", { s: 2 }, userId);
      gap = await insertAndPublish(db, accountId, "t");
      relisten?.();
    };

    const controller = new AbortController();
    aborts.push(controller);
    const res = await openEventStream({
      db,
      hub,
      userId: other.id,
      accountIds: [accountId],
      cursor: 0,
      signal: controller.signal,
    });
    const frames = frameReader(res);
    const ids: number[] = [];
    for (;;) {
      const f = await frames.nextWithin(1000);
      if (f === "timeout" || f === null) break;
      if (f.event) ids.push(Number(f.id));
    }
    expect(ids).toEqual([before, gap]);
    expect(ids).not.toContain(secret1);
    expect(ids).not.toContain(secret2);
  });
});

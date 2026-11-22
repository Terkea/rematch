import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createTestSession, resetAuthTables, webTestDb } from "../helpers/session";
import { frameReader, insertAndPublish, linkAccount, resetEventTables } from "./helpers";

const db = webTestDb();
const { createEventHub } = await import("../../src/server/events/hub");
const { openEventStream } = await import("../../src/server/events/sse");

/** A fake postgres.js `listen` that never delivers: only the stored rows exist for the stream. */
function silentListen() {
  let onListen: (() => void) | undefined;
  return {
    listen: vi.fn(async (_c: string, _n: (p: string) => void, listened: () => void) => {
      onListen = listened;
      listened();
      return { unlisten: async () => {} };
    }),
    relisten: () => onListen?.(),
  };
}

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
  for (const a of aborts.splice(0)) a.abort();
});

async function open(hub: ReturnType<typeof createEventHub>, cursor: number | null) {
  const controller = new AbortController();
  aborts.push(controller);
  const res = await openEventStream({
    db,
    hub,
    userId,
    accountIds: [accountId],
    cursor,
    signal: controller.signal,
  });
  return frameReader(res);
}

describe("LISTEN reconnect", () => {
  it("replays the events stored while LISTEN was down, once and in order", async () => {
    const pg = silentListen();
    const hub = createEventHub({ listen: pg.listen, fetchEvent: async () => null });
    const frames = await open(hub, 0);
    const e1 = await insertAndPublish(db, accountId, "t");
    // The fake LISTEN delivered nothing, so these were only stored: a gap the stream must close.
    const e2 = await insertAndPublish(db, accountId, "t");
    const e3 = await insertAndPublish(db, accountId, "t");
    pg.relisten();
    const ids: number[] = [];
    for (;;) {
      const f = await frames.nextEvent();
      if (!f) break;
      ids.push(Number(f.id));
      if (f.id === String(e3)) break;
    }
    expect(ids).toEqual([e1, e2, e3]);
    expect(await frames.nextWithin(200)).toBe("timeout");
  });

  it("recovers a live event whose truncated payload could not be fetched", async () => {
    const pg = silentListen();
    let notify: ((p: string) => void) | undefined;
    const listen = vi.fn(async (c: string, n: (p: string) => void, l: () => void) => {
      notify = n;
      return pg.listen(c, n, l);
    });
    const hub = createEventHub({
      listen,
      fetchEvent: async () => {
        throw new Error("db down");
      },
      fetchRetryDelaysMs: [1, 1],
      log: () => {},
    });
    const frames = await open(hub, 0);
    const e1 = await insertAndPublish(db, accountId, "t");
    // Announce e1 as truncated; the fetch fails every time.
    notify?.(JSON.stringify({ id: e1, accountId, type: "t", truncated: true }));
    const f = await frames.nextEvent();
    expect(f?.id).toBe(String(e1));
    expect(await frames.nextWithin(200)).toBe("timeout");
  });
});

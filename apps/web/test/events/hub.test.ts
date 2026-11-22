import type { StreamEvent } from "@ci/db";
import { describe, expect, it, vi } from "vitest";
import { createEventHub, type ListenFn } from "../../src/server/events/hub";

const A = "11111111-1111-4111-8111-111111111111";
const B = "22222222-2222-4222-8222-222222222222";
const U1 = "33333333-3333-4333-8333-333333333333";
const U2 = "44444444-4444-4444-8444-444444444444";

/** A fake postgres.js `listen`: `notify` delivers a body, `relisten` simulates a reconnect. */
function fakeListen(opts: { failFirst?: boolean } = {}) {
  let onNotify: ((p: string) => void) | undefined;
  let onListen: (() => void) | undefined;
  let calls = 0;
  const listen: ListenFn = vi.fn(async (_channel, notify, listened) => {
    calls++;
    if (opts.failFirst && calls === 1) throw new Error("connection refused");
    onNotify = notify;
    onListen = listened;
    listened();
    return { unlisten: async () => {} };
  });
  return {
    listen,
    notify: (body: object | string) =>
      onNotify?.(typeof body === "string" ? body : JSON.stringify(body)),
    relisten: () => onListen?.(),
  };
}

const note = (id: number, accountId: string, payload: unknown = { n: id }) => ({
  id,
  accountId,
  type: "import.progress",
  createdAt: "2026-11-21T10:00:00.000000Z",
  payload,
});

describe("event hub", () => {
  it("delivers each notification only to listeners of its account", async () => {
    const pg = fakeListen();
    const hub = createEventHub({ listen: pg.listen, fetchEvent: async () => null });
    const gotA: StreamEvent[] = [];
    const gotB: StreamEvent[] = [];
    hub.subscribe([A], (e) => gotA.push(e));
    hub.subscribe([B], (e) => gotB.push(e));
    await hub.ready();
    pg.notify(note(1, A));
    pg.notify(note(2, B));
    await vi.waitFor(() => expect(gotB).toHaveLength(1));
    expect(gotA).toEqual([{ id: 1, accountId: A, type: "import.progress", payload: { n: 1 } }]);
    expect(gotB.map((e) => e.id)).toEqual([2]);
    expect(pg.listen).toHaveBeenCalledTimes(1);
    expect(vi.mocked(pg.listen).mock.calls[0]?.[0]).toBe("user_events");
  });

  it("delivers a user-owned notification only to that user's subscribers", async () => {
    const pg = fakeListen();
    const fetchEvent = vi.fn(
      async (id: number): Promise<StreamEvent | null> => ({
        id,
        accountId: A,
        userId: U1,
        type: "chat.chunk",
        payload: {},
      }),
    );
    const hub = createEventHub({ listen: pg.listen, fetchEvent });
    const one: StreamEvent[] = [];
    const two: StreamEvent[] = [];
    const nobody: StreamEvent[] = [];
    hub.subscribe([A], (e) => one.push(e), undefined, U1);
    hub.subscribe([A], (e) => two.push(e), undefined, U2);
    hub.subscribe([A], (e) => nobody.push(e));
    await hub.ready();
    pg.notify({ ...note(1, A), userId: U1 });
    pg.notify({
      id: 2,
      accountId: A,
      userId: U1,
      type: "chat.chunk",
      createdAt: "x",
      truncated: true,
    });
    pg.notify(note(3, A));
    await vi.waitFor(() => expect(two).toHaveLength(1));
    expect(one.map((e) => e.id)).toEqual([1, 2, 3]);
    expect(two.map((e) => e.id)).toEqual([3]);
    expect(nobody.map((e) => e.id)).toEqual([3]);
  });

  it("refcounts accounts and an unsubscribe is idempotent", () => {
    const pg = fakeListen();
    const hub = createEventHub({ listen: pg.listen, fetchEvent: async () => null });
    const one = hub.subscribe([A, B], () => {});
    const two = hub.subscribe([A], () => {});
    expect(hub.channelCount()).toBe(2);
    one();
    one();
    expect(hub.channelCount()).toBe(1);
    two();
    expect(hub.channelCount()).toBe(0);
  });

  it("fetches a truncated payload by id and keeps id order", async () => {
    const pg = fakeListen();
    let release: () => void = () => {};
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    const fetchEvent = vi.fn(async (id: number) => {
      await gate;
      return { id, accountId: A, type: "import.progress", payload: { big: true } };
    });
    const hub = createEventHub({ listen: pg.listen, fetchEvent });
    const got: StreamEvent[] = [];
    hub.subscribe([A], (e) => got.push(e));
    await hub.ready();
    const { payload: _drop, ...head } = note(1, A);
    pg.notify({ ...head, truncated: true });
    pg.notify(note(2, A));
    await new Promise((r) => setTimeout(r, 20));
    expect(got).toEqual([]);
    release();
    await vi.waitFor(() => expect(got).toHaveLength(2));
    expect(got.map((e) => [e.id, e.payload])).toEqual([
      [1, { big: true }],
      [2, { n: 2 }],
    ]);
    expect(fetchEvent).toHaveBeenCalledWith(1);
  });

  it("does not fetch a truncated payload nobody listens to", async () => {
    const pg = fakeListen();
    const fetchEvent = vi.fn(async () => null);
    const hub = createEventHub({ listen: pg.listen, fetchEvent });
    hub.subscribe([A], () => {});
    await hub.ready();
    const { payload: _drop, ...head } = note(1, B);
    pg.notify({ ...head, truncated: true });
    await new Promise((r) => setTimeout(r, 10));
    expect(fetchEvent).not.toHaveBeenCalled();
  });

  it("skips malformed notifications and keeps going", async () => {
    const pg = fakeListen();
    const hub = createEventHub({
      listen: pg.listen,
      fetchEvent: async () => null,
      log: () => {},
    });
    const got: StreamEvent[] = [];
    hub.subscribe([A], (e) => got.push(e));
    await hub.ready();
    pg.notify("not json");
    pg.notify({ id: "x", accountId: A, type: "t" });
    pg.notify({ id: 3, accountId: A, type: "t\nevent: x", payload: {} });
    pg.notify(note(4, A));
    await vi.waitFor(() => expect(got.map((e) => e.id)).toEqual([4]));
  });

  it("tells subscribers to replay after the LISTEN connection is rebuilt", async () => {
    const pg = fakeListen();
    const hub = createEventHub({ listen: pg.listen, fetchEvent: async () => null });
    const onReconnect = vi.fn();
    hub.subscribe([A], () => {}, onReconnect);
    await hub.ready();
    expect(onReconnect).not.toHaveBeenCalled();
    pg.relisten();
    await vi.waitFor(() => expect(onReconnect).toHaveBeenCalledTimes(1));
  });

  it("retries LISTEN after a failed start", async () => {
    const pg = fakeListen({ failFirst: true });
    const hub = createEventHub({ listen: pg.listen, fetchEvent: async () => null, log: () => {} });
    hub.subscribe([A], () => {});
    await expect(hub.ready()).rejects.toThrow(/connection refused/);
    await expect(hub.ready()).resolves.toBeUndefined();
    expect(pg.listen).toHaveBeenCalledTimes(2);
  });

  it("retries a failed truncated fetch, then asks the subscribers to replay", async () => {
    const pg = fakeListen();
    const fetchEvent = vi.fn(async (): Promise<StreamEvent | null> => {
      throw new Error("db down");
    });
    const hub = createEventHub({
      listen: pg.listen,
      fetchEvent,
      fetchRetryDelaysMs: [1, 1],
      log: () => {},
    });
    const got: StreamEvent[] = [];
    const resync = vi.fn();
    hub.subscribe([A], (e) => got.push(e), resync);
    await hub.ready();
    pg.notify({ id: 1, accountId: A, type: "import.progress", truncated: true });
    await vi.waitFor(() => expect(resync).toHaveBeenCalledTimes(1));
    expect(fetchEvent).toHaveBeenCalledTimes(3);
    expect(got).toEqual([]);
  });

  it("delivers a truncated event whose fetch succeeds on a retry", async () => {
    const pg = fakeListen();
    const ev: StreamEvent = { id: 1, accountId: A, type: "import.progress", payload: { big: 1 } };
    const fetchEvent = vi
      .fn<() => Promise<StreamEvent | null>>()
      .mockRejectedValueOnce(new Error("blip"))
      .mockResolvedValue(ev);
    const hub = createEventHub({
      listen: pg.listen,
      fetchEvent,
      fetchRetryDelaysMs: [1, 1],
      log: () => {},
    });
    const got: StreamEvent[] = [];
    const resync = vi.fn();
    hub.subscribe([A], (e) => got.push(e), resync);
    await hub.ready();
    pg.notify({ id: 1, accountId: A, type: "import.progress", truncated: true });
    await vi.waitFor(() => expect(got).toEqual([ev]));
    expect(resync).not.toHaveBeenCalled();
  });
});

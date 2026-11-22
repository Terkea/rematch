import { describe, expect, it } from "vitest";
import { ClientGone, Overloaded } from "../src/errors.js";
import { Limiter } from "../src/limiter.js";

function deferred() {
  let resolve!: () => void;
  const promise = new Promise<void>((r) => {
    resolve = r;
  });
  return { promise, resolve };
}
const tick = () => new Promise((r) => setTimeout(r, 0));

describe("Limiter", () => {
  it("caps_in_flight_and_runs_fifo", async () => {
    const l = new Limiter(2, 10, 10000);
    const gates = [deferred(), deferred(), deferred(), deferred()];
    const started: number[] = [];
    const runs = gates.map((g, i) =>
      l.run(async () => {
        started.push(i);
        await g.promise;
        return i;
      }),
    );
    await tick();
    expect(started).toEqual([0, 1]);
    expect(l.stats()).toEqual({ inFlight: 2, queued: 2 });
    gates[0]?.resolve();
    await tick();
    expect(started).toEqual([0, 1, 2]);
    gates[1]?.resolve();
    gates[2]?.resolve();
    gates[3]?.resolve();
    expect(await Promise.all(runs)).toEqual([0, 1, 2, 3]);
    expect(l.stats()).toEqual({ inFlight: 0, queued: 0 });
  });

  it("releases_the_slot_when_fn_throws", async () => {
    const l = new Limiter(1, 1, 1000);
    await expect(l.run(async () => Promise.reject(new Error("boom")))).rejects.toThrow("boom");
    expect(await l.run(async () => 7)).toBe(7);
  });

  it("full_queue_rejects_with_overloaded", async () => {
    const l = new Limiter(1, 1, 10000);
    const g = deferred();
    const a = l.run(() => g.promise);
    const b = l.run(async () => {});
    const c = l.run(async () => {});
    await expect(c).rejects.toBeInstanceOf(Overloaded);
    const e = await c.catch((x) => x);
    expect([e.status, e.type, e.retryAfter]).toEqual([529, "overloaded_error", "5"]);
    g.resolve();
    await Promise.all([a, b]);
  });

  it("queue_wait_timeout_rejects", async () => {
    const l = new Limiter(1, 5, 20);
    const g = deferred();
    const a = l.run(() => g.promise);
    await expect(l.run(async () => {})).rejects.toBeInstanceOf(Overloaded);
    expect(l.stats().queued).toBe(0);
    g.resolve();
    await a;
  });

  it("queued_request_leaves_on_disconnect", async () => {
    const l = new Limiter(1, 5, 10000);
    const g = deferred();
    const a = l.run(() => g.promise);
    const client = new AbortController();
    let ran = false;
    const b = l.run(
      async () => {
        ran = true;
      },
      { signal: client.signal },
    );
    await tick();
    expect(l.stats().queued).toBe(1);
    client.abort();
    await expect(b).rejects.toBeInstanceOf(ClientGone);
    expect(l.stats().queued).toBe(0);
    g.resolve();
    await a;
    await tick();
    expect(ran).toBe(false);
  });
});

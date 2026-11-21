// The turn lock's connection handling, against a fake pg pool (the real Postgres lock is covered in
// session-store.test.ts). From the final Task 10 review: lockLost counts only while the lock is
// held, a no-op 'error' listener stays on the connection after release, and the not-acquired path
// does not fail open when its RESET fails.
import { EventEmitter } from "node:events";
import type pg from "pg";
import { describe, expect, it } from "vitest";
import { PgSessionStore } from "../src/session-store.js";

class FakeClient extends EventEmitter {
  released: boolean[] = [];
  constructor(
    private readonly acquired: boolean,
    private readonly failReset = false,
  ) {
    super();
  }
  async query(sql: string) {
    if (sql.startsWith("RESET") && this.failReset) throw new Error("connection reset");
    if (sql.includes("pg_try_advisory_lock")) return { rows: [{ ok: this.acquired }] };
    return { rows: [] };
  }
  release(destroy?: boolean) {
    this.released.push(destroy === true);
  }
}

function store(client: FakeClient) {
  let lost = 0;
  const pool = { connect: async () => client } as unknown as pg.Pool;
  const s = new PgSessionStore(pool, { onLockLost: () => (lost += 1) });
  return { s, lost: () => lost };
}

describe("tryLockConversation", () => {
  it("lock_lost_is_counted_only_while_the_lock_is_held", async () => {
    const c = new FakeClient(true);
    const { s, lost } = store(c);
    const unlock = await s.tryLockConversation("conv");
    expect(unlock).not.toBeNull();
    c.emit("error", new Error("57P01"));
    expect(lost()).toBe(1);
    await unlock?.();
    // A later error on the pooled connection is not this lock's loss, and does not throw.
    expect(() => c.emit("error", new Error("idle drop"))).not.toThrow();
    expect(lost()).toBe(1);
  });

  it("a_noop_error_listener_stays_after_release", async () => {
    const c = new FakeClient(true);
    const { s, lost } = store(c);
    const unlock = await s.tryLockConversation("conv");
    await unlock?.();
    expect(c.released).toEqual([false]);
    expect(c.listenerCount("error")).toBe(1);
    expect(() => c.emit("error", new Error("late"))).not.toThrow();
    expect(lost()).toBe(0);
    // Reusing the pooled connection for more turns does not pile up listeners.
    for (let i = 0; i < 20; i++) await (await s.tryLockConversation("conv"))?.();
    expect(c.listenerCount("error")).toBe(1);
  });

  it("not_acquired_with_a_failing_reset_is_still_not_acquired", async () => {
    const c = new FakeClient(false, true);
    const { s, lost } = store(c);
    // Never throws (the caller would proceed without a lock): it reports "held elsewhere".
    await expect(s.tryLockConversation("conv")).resolves.toBeNull();
    expect(c.released).toEqual([true]);
    expect(lost()).toBe(0);
  });

  it("not_acquired_returns_the_connection_after_a_clean_reset", async () => {
    const c = new FakeClient(false);
    const { s } = store(c);
    await expect(s.tryLockConversation("conv")).resolves.toBeNull();
    expect(c.released).toEqual([false]);
  });
});

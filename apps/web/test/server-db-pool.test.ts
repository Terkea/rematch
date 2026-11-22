import { afterEach, describe, expect, it, vi } from "vitest";

// Next dev builds each route into its own module graph, so a pool cached in a module variable is
// created once per graph. The pool must live on globalThis: one per process, whatever imports it.
const URL_A = "postgres://postgres:postgres@localhost:1/chess_pool_test";

async function freshModules() {
  vi.resetModules();
  vi.stubEnv("DATABASE_URL", URL_A);
  vi.stubEnv("BETTER_AUTH_SECRET", "x".repeat(40));
  const db = await import("../src/server/db");
  const auth = await import("../src/server/auth");
  const health = await import("../src/server/health");
  const hub = await import("../src/server/events/hub");
  return { db, auth, health, hub };
}

afterEach(() => {
  vi.unstubAllEnvs();
});

describe("one database pool per process", () => {
  it("returns the same Db for repeated getDb() calls", async () => {
    const { db } = await freshModules();
    expect(db.getDb()).toBe(db.getDb());
  });

  it("shares the pool across module graphs (a second import of the module)", async () => {
    const first = (await freshModules()).db.getDb();
    const second = (await freshModules()).db.getDb();
    expect(second).toBe(first);
  });

  it("gives auth and health the same pool as getDb()", async () => {
    const { db, auth, health } = await freshModules();
    const shared = db.getDb();
    expect(auth.getAuthDb()).toBe(shared);
    expect(health.healthDb()).toBe(shared);
  });

  it("caps the pool and closes idle connections", async () => {
    const { db } = await freshModules();
    const opts = db.getDb().$client.options;
    expect(opts.max).toBeLessThanOrEqual(10);
    expect(opts.idle_timeout).toBeGreaterThan(0);
  });

  it("builds the hub on the shared pool", async () => {
    const { hub } = await freshModules();
    expect(typeof hub.getEventHub).toBe("function");
  });
});

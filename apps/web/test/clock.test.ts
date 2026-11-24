import { afterEach, describe, expect, it, vi } from "vitest";
import { resetEnvCache } from "../src/env";
import { E2E_NOW_COOKIE, requestNow } from "../src/server/clock";

let cookie: string | undefined;
vi.mock("next/headers", () => ({
  cookies: async () => ({
    get: (name: string) =>
      name === E2E_NOW_COOKIE && cookie !== undefined ? { value: cookie } : undefined,
  }),
}));

const PINNED = "2026-11-23T12:00:00.000Z";

describe("requestNow", () => {
  afterEach(() => {
    cookie = undefined;
    vi.unstubAllEnvs();
    resetEnvCache();
  });

  it("is the wall clock without the e2e flag, whatever the cookie says", async () => {
    vi.stubEnv("E2E_CLOCK", "");
    resetEnvCache();
    cookie = PINNED;
    const before = Date.now();
    const now = (await requestNow()).getTime();
    expect(now).toBeGreaterThanOrEqual(before);
    expect(now).toBeLessThanOrEqual(Date.now());
  });

  it("is the cookie's time with the e2e flag on", async () => {
    vi.stubEnv("E2E_CLOCK", "1");
    resetEnvCache();
    cookie = PINNED;
    expect((await requestNow()).toISOString()).toBe(PINNED);
  });

  it("falls back to the wall clock on a missing or malformed cookie", async () => {
    vi.stubEnv("E2E_CLOCK", "1");
    resetEnvCache();
    for (const value of [undefined, "not a date"]) {
      cookie = value;
      const before = Date.now();
      expect((await requestNow()).getTime()).toBeGreaterThanOrEqual(before);
    }
  });
});

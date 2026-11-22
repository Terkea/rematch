// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  SEEN_KEY,
  SEEN_MAX,
  seenStore,
} from "../../src/app/(app)/app/games/[gameId]/reveal/seen-store";

// R10: "seen" per device in localStorage["ci.review.seen"], at most 500 ids, oldest dropped; any
// storage failure reads as seen so the full reveal never loops.

beforeEach(() => localStorage.clear());
afterEach(() => vi.restoreAllMocks());

describe("seenStore", () => {
  it("has an id once it is added", () => {
    expect(seenStore.has("g1")).toBe(false);
    seenStore.add("g1");
    expect(seenStore.has("g1")).toBe(true);
    expect(JSON.parse(localStorage.getItem(SEEN_KEY) ?? "[]")).toEqual(["g1"]);
  });

  it("keeps at most 500 ids, dropping the oldest", () => {
    expect(SEEN_MAX).toBe(500);
    for (let i = 0; i <= 500; i++) seenStore.add(`g${i}`);
    expect(seenStore.has("g0")).toBe(false);
    expect(seenStore.has("g1")).toBe(true);
    expect(seenStore.has("g500")).toBe(true);
    expect(JSON.parse(localStorage.getItem(SEEN_KEY) ?? "[]")).toHaveLength(500);
  });

  it("does not duplicate an id added twice, and moves it to the newest end", () => {
    seenStore.add("a");
    seenStore.add("b");
    seenStore.add("a");
    expect(JSON.parse(localStorage.getItem(SEEN_KEY) ?? "[]")).toEqual(["b", "a"]);
  });

  it("reads as seen when the localStorage getter throws, and add does not throw", () => {
    vi.spyOn(window, "localStorage", "get").mockImplementation(() => {
      throw new Error("SecurityError");
    });
    expect(seenStore.has("g1")).toBe(true);
    expect(() => seenStore.add("g1")).not.toThrow();
  });

  it("reads as seen when getItem throws, and add survives setItem throwing", () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("denied");
    });
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("QuotaExceeded");
    });
    expect(seenStore.has("g1")).toBe(true);
    expect(() => seenStore.add("g1")).not.toThrow();
  });

  it("treats a corrupt value as empty and repairs it on add", () => {
    localStorage.setItem(SEEN_KEY, "{not json");
    expect(seenStore.has("g1")).toBe(false);
    seenStore.add("g1");
    expect(seenStore.has("g1")).toBe(true);
  });

  it("reports whether add stored the id", () => {
    expect(seenStore.add("g1")).toBe(true);
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("QuotaExceeded");
    });
    expect(seenStore.add("g2")).toBe(false);
  });
});

import { randomUUID } from "node:crypto";
import { describe, expect, it } from "vitest";
import { createDbEvalCache, createMemoryEvalCache } from "../../src/analysis/eval-cache.js";
import { getWorkerTestDb, workerTestDbConfigured } from "../db.js";

function suite(make: () => Promise<ReturnType<typeof createMemoryEvalCache>>) {
  it("round-trips cp and mate scores for the right protocol only", async () => {
    const cache = await make();
    const p = `test-${randomUUID()}`;
    const a = `fen-a-${randomUUID()} w - -`;
    const b = `fen-b-${randomUUID()} b - -`;
    await cache.putMany([
      {
        fenKey: a,
        protocol: p,
        eval: { score: { cp: -42 }, bestUci: "e2e4", pv: ["e2e4"] },
        source: "stockfish",
      },
      {
        fenKey: b,
        protocol: p,
        eval: { score: { mate: 3 }, bestUci: null, pv: [] },
        source: "lichess",
      },
    ]);
    const got = await cache.getMany([a, b, "missing"], p);
    expect(got.get(a)).toEqual({ score: { cp: -42 }, bestUci: "e2e4", pv: ["e2e4"] });
    expect(got.get(b)).toEqual({ score: { mate: 3 }, bestUci: null, pv: [] });
    expect(got.size).toBe(2);
    expect((await cache.getMany([a], `other-${p}`)).size).toBe(0);
  });

  it("keeps the first row on conflict and handles empty input", async () => {
    const cache = await make();
    const p = `test-${randomUUID()}`;
    const k = `fen-${randomUUID()} w - -`;
    await cache.putMany([]);
    expect((await cache.getMany([], p)).size).toBe(0);
    const e = (cp: number) => ({ score: { cp }, bestUci: null, pv: [] });
    await cache.putMany([{ fenKey: k, protocol: p, eval: e(1), source: "stockfish" }]);
    await cache.putMany([{ fenKey: k, protocol: p, eval: e(2), source: "stockfish" }]);
    expect((await cache.getMany([k], p)).get(k)?.score).toEqual({ cp: 1 });
  });
}

describe("memory eval cache", () => suite(async () => createMemoryEvalCache()));

// Inside Docker this suite must fail, not skip, when the test database is not configured.
describe.skipIf(!workerTestDbConfigured())("db eval cache", () => {
  suite(async () => {
    const db = await getWorkerTestDb();
    if (!db) throw new Error("worker test database unavailable");
    return createDbEvalCache(db);
  });
});

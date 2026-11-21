import { randomUUID } from "node:crypto";
import { fileURLToPath } from "node:url";
import { createDb } from "@ci/db";
import { sql } from "drizzle-orm";
import { migrate } from "drizzle-orm/postgres-js/migrator";
import { describe, expect, it } from "vitest";
import { createDbEvalCache, createMemoryEvalCache } from "../../src/analysis/eval-cache.js";

const url = process.env.DATABASE_URL_TEST;

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

describe.skipIf(!url)("db eval cache", () => {
  suite(async () => {
    const db = createDb(url as string);
    const exists = await db.execute(sql`select to_regclass('public.position_evals') as t`);
    if (!exists[0]?.t) {
      await migrate(db, {
        migrationsFolder: fileURLToPath(
          new URL("../../../../packages/db/drizzle", import.meta.url),
        ),
      });
    }
    return createDbEvalCache(db);
  });
});

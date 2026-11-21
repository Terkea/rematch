import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { EnginePool } from "../../src/engine/engine-pool.js";
import { skipEngineTests, stockfishPath } from "./stockfish.js";

const START = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

describe.skipIf(skipEngineTests)("EnginePool", () => {
  let pool: EnginePool;
  beforeAll(async () => {
    pool = await EnginePool.create({ size: 2, path: stockfishPath });
  });
  afterAll(async () => {
    await pool?.close();
  });

  it("resolves two parallel analyses", async () => {
    const [a, b] = await Promise.all([
      pool.analyse(START, { nodes: 20_000 }),
      pool.analyse(START, { nodes: 20_000 }),
    ]);
    expect(a.length).toBe(1);
    expect(b.length).toBe(1);
  });

  it("pool recovers after engine crash", async () => {
    const pid = pool.pids()[0];
    expect(pid).toBeDefined();
    process.kill(pid as number, "SIGKILL");
    const results = await Promise.all(
      [1, 2, 3, 4].map(() => pool.analyse(START, { nodes: 20_000 })),
    );
    expect(results.every((r) => r.length === 1)).toBe(true);
  });

  it("retries an in-flight job once when the engine dies mid-search", async () => {
    const p = pool.analyse(START, { nodes: 3_000_000 });
    await new Promise((r) => setTimeout(r, 300));
    for (const pid of pool.pids()) process.kill(pid, "SIGKILL");
    await expect(p).resolves.toHaveLength(1);
  }, 60_000);
});

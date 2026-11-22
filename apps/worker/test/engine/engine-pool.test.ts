import { chmodSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { EnginePool, EnginePoolClosedError } from "../../src/engine/engine-pool.js";
import { InvalidFenError } from "../../src/engine/uci-engine.js";
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

const isDead = (pid: number): boolean => {
  try {
    process.kill(pid, 0);
    return false;
  } catch {
    return true;
  }
};

describe.skipIf(skipEngineTests)("EnginePool close semantics", () => {
  it("rejects queued and in-flight jobs on close, leaves no processes, rejects later calls", async () => {
    const pool = await EnginePool.create({ size: 1, path: stockfishPath });
    const pids = pool.pids();
    expect(pids.length).toBe(1);
    const inFlight = pool.analyse(START, { nodes: 50_000_000 });
    const queued = pool.analyse(START, { nodes: 1000 });
    const inFlightResult = inFlight.then(
      () => "ok",
      (e: unknown) => e,
    );
    const queuedResult = queued.then(
      () => "ok",
      (e: unknown) => e,
    );
    await new Promise((r) => setTimeout(r, 300));
    await pool.close();
    expect(await queuedResult).toBeInstanceOf(EnginePoolClosedError);
    expect(await inFlightResult).toBeInstanceOf(EnginePoolClosedError);
    for (const pid of pids) expect(isDead(pid)).toBe(true);
    await expect(pool.analyse(START, { nodes: 1000 })).rejects.toBeInstanceOf(
      EnginePoolClosedError,
    );
  }, 30_000);
});

// A UCI engine that completes the handshake but never answers `go`, so the
// pool's only engine stays busy until close() however fast the host is.
const HUNG_ENGINE = `#!/usr/bin/env node
const rl = require("node:readline").createInterface({ input: process.stdin });
rl.on("line", (line) => {
  const cmd = line.trim();
  if (cmd === "uci") console.log("uciok");
  else if (cmd === "isready") console.log("readyok");
  else if (cmd === "quit") process.exit(0);
});
`;

describe.skipIf(process.platform === "win32")("EnginePool input validation", () => {
  let dir: string;
  let hungEngine: string;
  beforeAll(() => {
    dir = mkdtempSync(join(tmpdir(), "hung-engine-"));
    hungEngine = join(dir, "hung-engine.cjs");
    writeFileSync(hungEngine, HUNG_ENGINE);
    chmodSync(hungEngine, 0o755);
  });
  afterAll(() => {
    rmSync(dir, { recursive: true, force: true });
  });

  it("rejects invalid FEN without waiting for a free engine", async () => {
    const pool = await EnginePool.create({ size: 1, path: hungEngine });
    let busySettled = false;
    const busy = pool.analyse(START, { nodes: 1000 }).finally(() => {
      busySettled = true;
    });
    const busyResult = busy.then(
      () => "ok",
      (e: unknown) => e,
    );
    try {
      await expect(pool.analyse("not a fen", { nodes: 1000 })).rejects.toBeInstanceOf(
        InvalidFenError,
      );
      // The only engine is still searching: the rejection did not wait for it.
      expect(busySettled).toBe(false);
    } finally {
      await pool.close();
    }
    expect(await busyResult).toBeInstanceOf(EnginePoolClosedError);
  });
});

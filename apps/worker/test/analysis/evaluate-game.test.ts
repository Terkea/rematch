import { type EngineScore, fenKey, parsePgn } from "@ci/chess-core";
import { describe, expect, it } from "vitest";
import { createMemoryEvalCache } from "../../src/analysis/eval-cache.js";
import { evaluateGame } from "../../src/analysis/evaluate-game.js";
import { ANALYSIS_PROTOCOL, ENGINE_NODES } from "../../src/engine/protocol.js";
import type { AnalysisLine } from "../../src/engine/uci-engine.js";

const SCHOLARS = "1. e4 e5 2. Qh5 Nc6 3. Bc4 Nf6 4. Qxf7# 1-0";
// Sam Loyd's 10-move stalemate.
const STALEMATE =
  "1. e3 a5 2. Qh5 Ra6 3. Qxa5 h5 4. h4 Rah6 5. Qxc7 f6 6. Qxd7+ Kf7 7. Qxb7 Qd3 8. Qxb8 Qh7 9. Qxc8 Kg6 10. Qe6 1/2-1/2";

function fakePool(score: (fen: string) => EngineScore = () => ({ cp: 30 })) {
  const calls: { fen: string; nodes: number; multipv: number }[] = [];
  return {
    calls,
    pool: {
      async analyse(
        fen: string,
        opts: { nodes?: number; multipv?: number },
      ): Promise<AnalysisLine[]> {
        calls.push({ fen, nodes: opts.nodes as number, multipv: opts.multipv as number });
        return [{ multipv: 1, score: score(fen), pv: ["e2e4", "e7e5"], depth: 10, nodes: 1 }];
      },
    },
  };
}

describe("evaluateGame", () => {
  it("analyses every position once at ENGINE_NODES and caches results", async () => {
    const game = parsePgn("1. e4 e5 2. Nf3 *");
    const cache = createMemoryEvalCache();
    const { pool, calls } = fakePool();
    const evals = await evaluateGame({ pool, cache }, game, null);
    expect(evals).toHaveLength(game.plies.length + 1);
    expect(calls).toHaveLength(4);
    expect(calls.every((c) => c.nodes === ENGINE_NODES && c.multipv === 1)).toBe(true);
    const keys = [game.plies[0]?.fenBefore as string, ...game.plies.map((p) => p.fenAfter)].map(
      fenKey,
    );
    const cached = await cache.getMany(keys, ANALYSIS_PROTOCOL);
    expect(cached.size).toBe(4);
  });

  it("makes 0 engine calls on a second run", async () => {
    const game = parsePgn("1. e4 e5 2. Nf3 *");
    const cache = createMemoryEvalCache();
    const first = fakePool();
    const a = await evaluateGame({ pool: first.pool, cache }, game, null);
    const second = fakePool();
    const b = await evaluateGame({ pool: second.pool, cache }, game, null);
    expect(second.calls).toHaveLength(0);
    expect(b).toEqual(a);
  });

  it("terminal positions not sent to engine: checkmate gives cp 1000 for white", async () => {
    const game = parsePgn(SCHOLARS);
    const { pool, calls } = fakePool();
    const evals = await evaluateGame({ pool, cache: createMemoryEvalCache() }, game, null);
    expect(evals).toHaveLength(game.plies.length + 1);
    expect(evals.at(-1)?.score).toEqual({ cp: 1000 });
    expect(calls.map((c) => c.fen)).not.toContain(game.plies.at(-1)?.fenAfter);
    expect(calls).toHaveLength(game.plies.length);
  });

  it("gives -1000 when black delivers mate", async () => {
    const game = parsePgn("1. f3 e5 2. g4 Qh4# 0-1");
    const { pool } = fakePool();
    const evals = await evaluateGame({ pool, cache: createMemoryEvalCache() }, game, null);
    expect(evals.at(-1)?.score).toEqual({ cp: -1000 });
  });

  it("terminal positions not sent to engine: stalemate gives cp 0", async () => {
    const game = parsePgn(STALEMATE);
    expect(game.terminal).toBe("stalemate");
    const { pool, calls } = fakePool();
    const evals = await evaluateGame({ pool, cache: createMemoryEvalCache() }, game, null);
    expect(evals.at(-1)?.score).toEqual({ cp: 0 });
    expect(calls.map((c) => c.fen)).not.toContain(game.plies.at(-1)?.fenAfter);
    expect(calls).toHaveLength(game.plies.length);
  });

  it("converts side-to-move scores to white point of view", async () => {
    const game = parsePgn("1. e4 *");
    const { pool } = fakePool(() => ({ cp: 30 }));
    const evals = await evaluateGame({ pool, cache: createMemoryEvalCache() }, game, null);
    expect(evals[0]?.score).toEqual({ cp: 30 }); // white to move
    expect(evals[1]?.score).toEqual({ cp: -30 }); // black to move
    expect(evals[0]?.bestUci).toBe("e2e4");
    expect(evals[0]?.pv).toEqual(["e2e4", "e7e5"]);
  });

  it("uses complete lichess evals and calls the engine only for the start position", async () => {
    const game = parsePgn("1. e4 e5 2. Nf3 *");
    const cache = createMemoryEvalCache();
    const { pool, calls } = fakePool();
    const lichess: EngineScore[] = [{ cp: 20 }, { cp: 10 }, { mate: -3 }];
    const evals = await evaluateGame({ pool, cache }, game, lichess);
    expect(calls).toHaveLength(1);
    expect(calls[0]?.fen).toBe(game.plies[0]?.fenBefore);
    expect(evals.slice(1).map((e) => e.score)).toEqual(lichess);
    const stored = await cache.getMany(
      [fenKey(game.plies[0]?.fenAfter as string)],
      ANALYSIS_PROTOCOL,
    );
    expect(stored.size).toBe(1);
  });

  it("makes 0 engine calls with complete lichess evals and a cached start position", async () => {
    const game = parsePgn("1. e4 e5 2. Nf3 *");
    const fresh = createMemoryEvalCache();
    const start = fenKey(game.plies[0]?.fenBefore as string);
    await fresh.putMany([
      {
        fenKey: start,
        protocol: ANALYSIS_PROTOCOL,
        eval: { score: { cp: 30 }, bestUci: null, pv: [] },
        source: "stockfish",
      },
    ]);
    const { pool, calls } = fakePool();
    await evaluateGame({ pool, cache: fresh }, game, [{ cp: 1 }, { cp: 2 }, { cp: 3 }]);
    expect(calls).toHaveLength(0);
  });

  it("analyses null lichess entries with the engine", async () => {
    const game = parsePgn("1. e4 e5 2. Nf3 *");
    const { pool, calls } = fakePool();
    const evals = await evaluateGame({ pool, cache: createMemoryEvalCache() }, game, [
      { cp: 20 },
      null,
      { cp: 5 },
    ]);
    expect(calls).toHaveLength(2); // start + ply 2
    expect(evals[1]?.score).toEqual({ cp: 20 });
    expect(evals[2]?.score).toEqual({ cp: 30 }); // white to move after ply 2? black moved, so white to move
  });

  it("analyses duplicate positions once", async () => {
    const game = parsePgn("1. Nf3 Nf6 2. Ng1 Ng8 *");
    const { pool, calls } = fakePool();
    const evals = await evaluateGame({ pool, cache: createMemoryEvalCache() }, game, null);
    expect(evals).toHaveLength(5);
    expect(calls).toHaveLength(4); // 5 positions, the final one repeats the start
  });
});

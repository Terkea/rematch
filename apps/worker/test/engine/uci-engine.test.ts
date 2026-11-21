import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { InvalidFenError, UciEngine } from "../../src/engine/uci-engine.js";
import { skipEngineTests, stockfishPath } from "./stockfish.js";

const START = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

describe.skipIf(skipEngineTests)("UciEngine", () => {
  let engine: UciEngine;
  beforeAll(async () => {
    engine = await UciEngine.start({ path: stockfishPath });
  });
  afterAll(async () => {
    await engine?.quit();
  });

  it("analyses the start position", async () => {
    const lines = await engine.analyse(START, { nodes: 50_000 });
    const first = lines[0];
    expect(first).toBeDefined();
    const score = first?.score as { cp: number };
    expect(Math.abs(score.cp)).toBeLessThanOrEqual(100);
    expect(first?.pv[0]).toMatch(/^[a-h][1-8][a-h][1-8][qrbn]?$/);
  });

  it("finds mate in one", async () => {
    const lines = await engine.analyse("6k1/5ppp/8/8/8/8/5PPP/3R2K1 w - - 0 1", { nodes: 50_000 });
    expect(lines[0]?.score).toEqual({ mate: 1 });
    expect(lines[0]?.pv[0]).toBe("d1d8");
  });

  it("returns multipv lines", async () => {
    const lines = await engine.analyse(START, { nodes: 50_000, multipv: 3 });
    expect(lines.map((l) => l.multipv)).toEqual([1, 2, 3]);
  });

  it("rejects invalid FEN without killing engine", async () => {
    await expect(engine.analyse("not a fen", { nodes: 1000 })).rejects.toBeInstanceOf(
      InvalidFenError,
    );
    expect(engine.alive).toBe(true);
    const lines = await engine.analyse(START, { nodes: 10_000 });
    expect(lines.length).toBe(1);
  });

  it("returns a synthetic mate-0 line for checkmate", async () => {
    const fen = "rnb1kbnr/pppp1ppp/8/4p3/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 1 3";
    const lines = await engine.analyse(fen, { nodes: 1000 });
    expect(lines).toEqual([{ multipv: 1, score: { mate: 0 }, pv: [], depth: 0, nodes: 0 }]);
  });

  it("returns a synthetic cp-0 line for stalemate", async () => {
    const lines = await engine.analyse("7k/5Q2/6K1/8/8/8/8/8 b - - 0 1", { nodes: 1000 });
    expect(lines).toEqual([{ multipv: 1, score: { cp: 0 }, pv: [], depth: 0, nodes: 0 }]);
  });

  it("times out, kills the process and rejects", async () => {
    const e = await UciEngine.start({ path: stockfishPath });
    await expect(e.analyse(START, { nodes: 5_000_000_000, timeoutMs: 200 })).rejects.toThrow(
      /timed out/,
    );
    expect(e.alive).toBe(false);
  });
});

import { describe, expect, it } from "vitest";
import { games } from "../src/fixtures/index.js";
import {
  derivedLine,
  type ExplorerMove,
  explorerReducer,
  initialExplorerState,
  lineMoves,
  moveFrom,
  type PositionLine,
  positionAt,
  tryMatch,
} from "../src/position-explorer/model.js";

// The explorer's model (feat/position-explorer): pure, so it is tested without a DOM.

const game = games.middlegameBlunder;
const fenBefore = (ply: number) =>
  ply === 1 ? game.startFen : (game.plies[ply - 2]?.fenAfter ?? "");
const asMove = (ply: number): ExplorerMove => {
  const p = game.plies[ply - 1];
  if (!p) throw new Error("fixture changed");
  return {
    ply,
    color: p.color,
    san: p.san,
    uci: p.uci,
    fenBefore: fenBefore(ply),
    fenAfter: p.fenAfter,
  };
};

// 28. Nd2?? (ply 55) with four plies either side.
const CITED = 55;
const line: PositionLine = {
  game: [51, 52, 53, 54, 55, 56, 57, 58, 59].map(asMove),
  engine: [],
};

describe("moveFrom", () => {
  it("plays a legal UCI move and gives its SAN and the FEN after", () => {
    const m = moveFrom(game.startFen, "e2e4", 1);
    expect(m).toMatchObject({ ply: 1, color: "white", san: "e4", uci: "e2e4" });
    expect(m?.fenAfter.startsWith("rnbqkbnr/pppppppp/8/8/4P3/")).toBe(true);
  });

  it("refuses an illegal or malformed move", () => {
    expect(moveFrom(game.startFen, "e2e5", 1)).toBeNull();
    expect(moveFrom(game.startFen, "zz", 1)).toBeNull();
    expect(moveFrom("not a fen", "e2e4", 1)).toBeNull();
  });
});

describe("derivedLine", () => {
  it("without a loaded line, the game is the move played and the engine its better move", () => {
    const d = derivedLine({
      ply: 1,
      fenBefore: game.startFen,
      played: { san: "d4", uci: "d2d4" },
      best: { san: "e4", uci: "e2e4" },
    });
    expect(d.game.map((m) => m.san)).toEqual(["d4"]);
    expect(d.engine.map((m) => m.san)).toEqual(["e4"]);
  });

  it("no better move gives no engine line", () => {
    const d = derivedLine({
      ply: 1,
      fenBefore: game.startFen,
      played: { san: "d4", uci: "d2d4" },
      best: null,
    });
    expect(d.engine).toEqual([]);
  });
});

describe("lineMoves and the cursor", () => {
  it("starts on the position before the cited move", () => {
    const s = initialExplorerState(line, CITED);
    expect(s.mode).toBe("game");
    const moves = lineMoves(s);
    expect(positionAt(moves, s.cursor, fenBefore(51))).toBe(fenBefore(CITED));
  });

  it("the engine line shares the moves before the cited ply, then follows the engine", () => {
    const engine = [moveFrom(fenBefore(CITED), "e4c3", CITED)].filter(
      (m): m is ExplorerMove => m !== null,
    );
    const s = explorerReducer(initialExplorerState({ ...line, engine }, CITED), {
      type: "mode",
      mode: "engine",
    });
    const moves = lineMoves(s);
    expect(moves.map((m) => m.ply)).toEqual([51, 52, 53, 54, 55]);
    expect(moves.at(-1)?.uci).toBe("e4c3");
    expect(s.cursor).toBe(4);
  });

  it("steps back and forward within the line, and clamps at both ends", () => {
    let s = initialExplorerState(line, CITED);
    s = explorerReducer(s, { type: "step", by: 1 });
    expect(s.cursor).toBe(5);
    s = explorerReducer(s, { type: "step", by: -10 });
    expect(s.cursor).toBe(0);
    s = explorerReducer(s, { type: "end" });
    expect(s.cursor).toBe(9);
    s = explorerReducer(s, { type: "step", by: 1 });
    expect(s.cursor).toBe(9);
    s = explorerReducer(s, { type: "start" });
    expect(s.cursor).toBe(0);
  });

  it("goes to a ply of the shown line", () => {
    let s = initialExplorerState(line, CITED);
    s = explorerReducer(s, { type: "goto", ply: 57 });
    expect(s.cursor).toBe(7);
  });

  it("a newly loaded line keeps the cited position on the board", () => {
    let s = initialExplorerState(
      derivedLine({
        ply: CITED,
        fenBefore: fenBefore(CITED),
        played: { san: "Nd2", uci: "e4d2" },
        best: null,
      }),
      CITED,
    );
    expect(s.cursor).toBe(0);
    s = explorerReducer(s, { type: "loaded", line });
    expect(s.cursor).toBe(4);
    expect(positionAt(lineMoves(s), s.cursor, fenBefore(51))).toBe(fenBefore(CITED));
  });
});

describe("trying moves", () => {
  it("a move played on the board branches from the shown position", () => {
    let s = initialExplorerState(line, CITED);
    const tried = moveFrom(fenBefore(CITED), "e4c3", CITED);
    if (!tried) throw new Error("illegal");
    s = explorerReducer(s, { type: "play", move: tried });
    expect(s.mode).toBe("try");
    const moves = lineMoves(s);
    expect(moves.map((m) => m.ply)).toEqual([51, 52, 53, 54, 55]);
    expect(moves.at(-1)?.uci).toBe("e4c3");
    expect(s.cursor).toBe(5);
    expect(s.tryFrom).toBe(4);
  });

  it("stepping back and playing again replaces the tried moves after the cursor", () => {
    let s = initialExplorerState(line, CITED);
    const a = moveFrom(fenBefore(CITED), "e4c3", CITED);
    if (!a) throw new Error("illegal");
    s = explorerReducer(s, { type: "play", move: a });
    const b = moveFrom(a.fenAfter, (game.plies[55]?.uci ?? "").slice(0, 4), CITED + 1);
    if (b) s = explorerReducer(s, { type: "play", move: b });
    s = explorerReducer(s, { type: "step", by: -1 });
    if (b) s = explorerReducer(s, { type: "step", by: -1 });
    const c = moveFrom(fenBefore(CITED), "e4g5", CITED);
    if (!c) throw new Error("illegal");
    s = explorerReducer(s, { type: "play", move: c });
    expect(lineMoves(s).at(-1)?.uci).toBe("e4g5");
    expect(lineMoves(s)).toHaveLength(5);
  });

  it("reset returns to where the trying started, with no tried moves", () => {
    let s = initialExplorerState(line, CITED);
    const a = moveFrom(fenBefore(CITED), "e4c3", CITED);
    if (!a) throw new Error("illegal");
    s = explorerReducer(s, { type: "play", move: a });
    s = explorerReducer(s, { type: "reset" });
    expect(s.mode).toBe("try");
    expect(lineMoves(s)).toHaveLength(4);
    expect(s.cursor).toBe(4);
  });

  it("switching to a line drops the tried moves and returns to the cited position", () => {
    let s = initialExplorerState(line, CITED);
    s = explorerReducer(s, { type: "step", by: 3 });
    s = explorerReducer(s, { type: "mode", mode: "try" });
    expect(s.tryFrom).toBe(7);
    s = explorerReducer(s, { type: "mode", mode: "game" });
    expect(s.cursor).toBe(4);
    expect(s.tried).toEqual([]);
  });

  it("names a tried move that is the game's move or the engine's", () => {
    const engine = [moveFrom(fenBefore(CITED), "e4c3", CITED)].filter(
      (m): m is ExplorerMove => m !== null,
    );
    const l = { ...line, engine };
    let s = initialExplorerState(l, CITED);
    const played = moveFrom(fenBefore(CITED), "e4d2", CITED);
    if (!played) throw new Error("illegal");
    s = explorerReducer(s, { type: "play", move: played });
    expect(tryMatch(s)).toBe("game");
    s = explorerReducer(s, { type: "reset" });
    const best = engine[0];
    if (!best) throw new Error("illegal");
    s = explorerReducer(s, { type: "play", move: best });
    expect(tryMatch(s)).toBe("engine");
    s = explorerReducer(s, { type: "reset" });
    expect(tryMatch(s)).toBeNull();
  });
});

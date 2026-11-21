import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { parsePgn, UnsupportedVariantError } from "../src/pgn.js";

const fixture = (name: string) =>
  readFileSync(new URL(`./fixtures/${name}`, import.meta.url), "utf8");

describe("parsePgn", () => {
  it("parses Scholar's mate with clocks", () => {
    const game = parsePgn(fixture("scholars-mate.pgn"));
    expect(game.plies.length).toBe(7);
    expect(game.plies[0]).toMatchObject({
      ply: 1,
      color: "white",
      san: "e4",
      uci: "e2e4",
      clockSeconds: 178,
    });
    expect(game.plies[0]?.fenBefore).toBe(game.startFen);
    expect(game.plies[0]?.fenAfter).toBe(
      "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1",
    );
    expect(game.plies[1]).toMatchObject({ color: "black", clockSeconds: 177 });
    expect(game.plies[5]?.clockSeconds).toBe(160);
    expect(game.plies[6]?.uci).toBe("h5f7");
    expect(game.plies[6]?.clockSeconds).toBe(169);
    expect(game.terminal).toBe("checkmate");
    expect(game.headers.TimeControl).toBe("180");
  });

  it("parses game without clocks", () => {
    const game = parsePgn(fixture("no-clocks.pgn"));
    expect(game.plies.length).toBe(7);
    expect(game.plies.every((p) => p.clockSeconds === null)).toBe(true);
  });

  it("rejects chess960", () => {
    expect(() => parsePgn(fixture("chess960.pgn"))).toThrow(UnsupportedVariantError);
  });

  it("rejects non-standard variant headers and SetUp/FEN games", () => {
    expect(() => parsePgn('[Variant "Atomic"]\n\n1. e4 *')).toThrow(UnsupportedVariantError);
    expect(() => parsePgn('[SetUp "1"]\n[FEN "8/8/8/8/8/8/8/K6k w - - 0 1"]\n\n*')).toThrow(
      UnsupportedVariantError,
    );
  });

  it("accepts an explicit Standard variant and reports promotions and draws", () => {
    const game = parsePgn(
      '[Variant "Standard"]\n\n1. e4 {[%clk 0:00:59.5]} d5 2. exd5 c6 3. dxc6 Nf6 4. cxb7 Nbd7 5. bxa8=Q Nb6 *',
    );
    expect(game.plies[0]?.clockSeconds).toBeCloseTo(59.5, 5);
    expect(game.plies[8]?.uci).toBe("b7a8q");
    expect(game.terminal).toBeNull();
  });
});

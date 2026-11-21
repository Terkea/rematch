import { describe, expect, it } from "vitest";
import { fenKey } from "../src/fen.js";
import { classifyTimeControl } from "../src/time-control.js";

describe("classifyTimeControl", () => {
  it("classifies by estimated duration base + 40 * increment", () => {
    expect(classifyTimeControl(60, 0)).toBe("bullet");
    expect(classifyTimeControl(180, 0)).toBe("blitz");
    expect(classifyTimeControl(180, 2)).toBe("blitz");
    expect(classifyTimeControl(300, 3)).toBe("blitz");
    expect(classifyTimeControl(480, 0)).toBe("rapid");
    expect(classifyTimeControl(600, 0)).toBe("rapid");
    expect(classifyTimeControl(900, 10)).toBe("rapid");
    expect(classifyTimeControl(1800, 0)).toBe("classical");
  });

  it("uses exclusive upper bounds", () => {
    expect(classifyTimeControl(179, 0)).toBe("bullet");
    expect(classifyTimeControl(479, 0)).toBe("blitz");
    expect(classifyTimeControl(1499, 0)).toBe("rapid");
    expect(classifyTimeControl(1500, 0)).toBe("classical");
  });
});

describe("fenKey", () => {
  it("returns the first four FEN fields", () => {
    expect(fenKey("rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1")).toBe(
      "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq -",
    );
  });
});

import { describe, expect, it } from "vitest";
import { findLeaks } from "../../src/validator/leaks.js";
import { parseTokens } from "../../src/validator/tokens.js";

const ESCAPES: string[] = [
  "12. Nf3 d5",
  "1. e4 e5 2. Nf3 Nc6",
  "Instead d5 was best",
  "...d6",
  "Nxe5 d6 (e4)",
  "e2-e4",
  "Ng1-f3",
  "e2xd5",
  "e8Q",
  "e8(Q)",
  "O–O",
  "O‑O‑O",
  "O—O",
  "O−O",
  "O-O",
  "0-0-0",
  "N⁦f3",
  "N͏f3",
  "♞️f3",
  "forty percent",
  "thirty games",
  "ten per cent",
  "one game",
  "three openings",
  "a third of games",
  "twenty-one games",
  "2.5x",
  "3 times",
  "nf3",
  "NF3",
  "♙e4",
  "Pe4",
  "Nf3Nc6",
  "exd6e.p.",
  "exd6 e.p.",
  "the 14th of October",
  "14 Oct",
  "14/10",
  "2026-10-14",
  ".5%",
  "1,5",
  "1 000",
  "1 000",
  "１２ games",
  "١٢ games",
  "half of your games",
  "a quarter",
  "a dozen",
  "twice as often",
  "the second game",
  "ten",
  "е4",
  "N​f‌3",
  "Nㅤf3",
  "♟",
  "♔",
];

describe("findLeaks flags every escape string", () => {
  for (const s of ESCAPES) {
    it(JSON.stringify(s), () => {
      expect(findLeaks(parseTokens(s).stripped).length).toBeGreaterThan(0);
    });
  }
});

describe("findLeaks leaves clean prose alone", () => {
  for (const s of [
    "you tend to rush in sharp positions",
    "the {{sq:e4}} square",
    "you lost the {{move:li-w1a:6}} knight too early; {{best:li-w1a:6}} was safer",
    "your rate was {{num:weaknesses.W1.posteriorRate|pct0}} over {{num:basis.analysedGames|int}} games",
    "Pay attention to king safety and loose pieces. Castling early helps.",
  ]) {
    it(JSON.stringify(s), () => {
      const r = parseTokens(s);
      expect(r.errors).toEqual([]);
      expect(findLeaks(r.stripped)).toEqual([]);
    });
  }
});

describe("findLeaks reports kinds", () => {
  it("labels leak kinds", () => {
    const kinds = (s: string) => findLeaks(s).map((l) => l.kind);
    expect(kinds("play O-O")).toContain("castling");
    expect(kinds("play ♞")).toContain("figurine");
    expect(kinds("about 40")).toContain("digit");
    expect(kinds("about forty")).toContain("number_word");
    expect(kinds("a 14 Oct date")).toContain("date");
  });
});

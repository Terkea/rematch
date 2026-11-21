import { DEFAULT_POSITION as START_FEN } from "chess.js";
import { describe, expect, it } from "vitest";
import { extractDates, extractMoves, extractNumbers } from "../../src/validator/extract.js";
import { normalizeText } from "../../src/validator/normalize.js";
import { pvToSan } from "../../src/validator/pv.js";

describe("normalizeText", () => {
  it("applies NFKC (full-width digits and letters)", () => {
    expect(normalizeText("１２. Ｎｆ３")).toBe("12. Nf3");
  });
  it("strips zero-width characters", () => {
    expect(normalizeText("N​f‌3‍﻿⁠")).toBe("Nf3");
  });
  it("maps confusable minus signs before digits", () => {
    expect(normalizeText("−4.2 –3 — x")).toBe("-4.2 -3 — x");
  });
  it("maps figurines to letters", () => {
    expect(normalizeText("♔♕♖♗♘ ♚♛♜♝♞")).toBe("KQRBN KQRBN");
  });
  it("maps 0-0 castling to O-O", () => {
    expect(normalizeText("0-0 and 0-0-0")).toBe("O-O and O-O-O");
  });
  it("is idempotent", () => {
    const s = "−４.2 ♘f3 0-0";
    expect(normalizeText(normalizeText(s))).toBe(normalizeText(s));
  });
});

describe("extractMoves", () => {
  it("finds SAN with move numbers, suffixes and castling", () => {
    expect(extractMoves("After 12. Nxe5 dxe5 13.Qh5+ you missed O-O-O").san).toEqual([
      "Nxe5",
      "dxe5",
      "Qh5",
      "O-O-O",
    ]);
  });
  it("does not treat squares as moves", () => {
    expect(extractMoves("the knight on f3 guards e5").san).toEqual([]);
  });
  it("counts a bare pawn move after a move number", () => {
    expect(extractMoves("12...e5 was the problem").san).toEqual(["e5"]);
    expect(extractMoves("12. e4 e5").san).toEqual(["e4", "e5"]);
    expect(extractMoves("12. ... e5").san).toEqual(["e5"]);
  });
  it("counts a bare pawn move after play/push words", () => {
    expect(extractMoves("you played h4 too early").san).toEqual(["h4"]);
    expect(extractMoves("pushed e5 and then advance d4").san).toEqual(["e5", "d4"]);
  });
  it("keeps promotions without suffix", () => {
    expect(extractMoves("e8=Q+ wins").san).toEqual(["e8=Q"]);
    expect(extractMoves("exd8=N#").san).toEqual(["exd8=N"]);
  });
  it("handles O and 0 castling", () => {
    expect(extractMoves(normalizeText("castle with 0-0")).san).toEqual(["O-O"]);
    expect(extractMoves("0-0-0 and O-O+").san).toEqual(["O-O-O", "O-O"]);
  });
  it("handles figurine notation", () => {
    expect(extractMoves(normalizeText("♘f3")).san).toEqual(["Nf3"]);
    expect(extractMoves("♞xe5").san).toEqual(["Nxe5"]);
  });
  it("strips every annotation glyph", () => {
    expect(extractMoves("Rxe6?? lost and exd5 was fine").san).toEqual(["Rxe6", "exd5"]);
    for (const g of ["!", "?", "!!", "??", "!?", "?!"]) {
      expect(extractMoves(`Nf3${g} and Qh5+${g} and Qxf7#${g}`).san).toEqual([
        "Nf3",
        "Qh5",
        "Qxf7",
      ]);
    }
  });
  it("finds disambiguated SAN", () => {
    expect(extractMoves("Nbd7 R1e2").san).toEqual(["Nbd7", "R1e2"]);
    expect(extractMoves("Qh4xe1").uci).toEqual(["h4e1"]);
  });
  it("finds UCI", () => {
    expect(extractMoves("the engine line g1f3 d7d5").uci).toEqual(["g1f3", "d7d5"]);
    expect(extractMoves("a7a8q").uci).toEqual(["a7a8q"]);
  });
  it("is not fooled by zero-width characters inside a move", () => {
    expect(extractMoves("N​f3 and Q⁠h5").san).toEqual(["Nf3", "Qh5"]);
  });
  it("ignores ids, time controls, dates", () => {
    expect(extractMoves("W3 at B2 in 3+0 blitz on 2026-10-14")).toEqual({ san: [], uci: [] });
  });
  it("does not match SAN inside longer words", () => {
    expect(extractMoves("Bad Rd1ng Knights")).toEqual({ san: [], uci: [] });
  });
});

describe("extractNumbers", () => {
  it("handles thousands separators", () => {
    expect(
      extractNumbers("over your last 214 analysed rapid games (3,912 imported)").map(
        (n) => n.value,
      ),
    ).toEqual([214, 3912]);
  });
  it("normalises unicode minus and skips move numbers and SAN", () => {
    expect(extractNumbers(normalizeText("12. Nxe5 −4.2"))).toMatchObject([
      { value: -4.2, decimals: 1 },
    ]);
    expect(extractNumbers("12...e5 and 13.Qh5+ g1f3")).toEqual([]);
  });
  it("normalises full-width digits and zero-width splits", () => {
    expect(extractNumbers("２１４ games").map((n) => n.value)).toEqual([214]);
    expect(extractNumbers("2​1​4 games").map((n) => n.value)).toEqual([214]);
  });
  it("flags percent", () => {
    expect(extractNumbers("31% of games")).toMatchObject([{ value: 31, percent: true }]);
    expect(extractNumbers("31 percent of games")).toMatchObject([{ value: 31, percent: true }]);
    expect(extractNumbers("31 games")[0]?.percent).toBe(false);
  });
  it("handles decimals and signs", () => {
    expect(extractNumbers("+0.35 and -1.5 and 12.25")).toMatchObject([
      { value: 0.35, decimals: 2 },
      { value: -1.5, decimals: 1 },
      { value: 12.25, decimals: 2 },
    ]);
  });
  it("splits ranges into unsigned values", () => {
    expect(extractNumbers("10-12 games").map((n) => n.value)).toEqual([10, 12]);
    expect(extractNumbers("10–12 games").map((n) => n.value)).toEqual([10, 12]);
  });
  it("handles plus-minus", () => {
    expect(extractNumbers("45% ± 6").map((n) => n.value)).toEqual([45, 6]);
  });
  it("sets hedged", () => {
    expect(extractNumbers("about 40 games")).toMatchObject([{ value: 40, hedged: true }]);
    expect(extractNumbers("at least 5 games")[0]?.hedged).toBe(true);
    expect(extractNumbers("more than 5 games")[0]?.hedged).toBe(true);
    expect(extractNumbers("~12 games")[0]?.hedged).toBe(true);
    expect(extractNumbers("exactly 5 games")[0]?.hedged).toBe(false);
  });
  it("handles ratio words", () => {
    expect(extractNumbers("twice as often")).toMatchObject([{ value: 2, kind: "ratio" }]);
    expect(extractNumbers("double the rate")).toMatchObject([{ value: 2, kind: "ratio" }]);
    expect(extractNumbers("half of them")).toMatchObject([{ value: 0.5, kind: "ratio" }]);
    expect(extractNumbers("three times as often")).toMatchObject([{ value: 3, kind: "ratio" }]);
  });
  it("counts number words only with a unit", () => {
    expect(extractNumbers("three games").map((n) => n.value)).toEqual([3]);
    expect(extractNumbers("three rapid games").map((n) => n.value)).toEqual([3]);
    expect(extractNumbers("one of your games")).toEqual([]);
    expect(extractNumbers("one thing to try")).toEqual([]);
    expect(extractNumbers("ten percent")).toMatchObject([{ value: 10, percent: true }]);
  });
  it("excludes weakness ids, band ids and citations", () => {
    expect(extractNumbers("W12 and B3 [[g:abc:24]]")).toEqual([]);
    expect(extractNumbers("W3a and W10b")).toEqual([]);
  });
  it("excludes dates but keeps nearby numbers", () => {
    expect(extractNumbers("since 2026-10-14 you lost 5 games")).toMatchObject([{ value: 5 }]);
    expect(extractNumbers("since about 14 Oct").length).toBe(0);
    expect(extractNumbers("on Oct 14, 2026").length).toBe(0);
  });
  it("excludes game results", () => {
    expect(extractNumbers("lost 0-1 and won 1-0")).toEqual([]);
  });
  it("excludes ordinals and letter-glued digits", () => {
    expect(extractNumbers("your 3rd game, square e5")).toEqual([]);
  });
});

describe("extractDates", () => {
  it("parses day-month and month-day forms", () => {
    expect(extractDates("since about 14 Oct")).toEqual([{ year: null, month: 10, day: 14 }]);
    expect(extractDates("on Oct 14th")).toEqual([{ year: null, month: 10, day: 14 }]);
    expect(extractDates("on 3 September 2026")).toEqual([{ year: 2026, month: 9, day: 3 }]);
  });
  it("parses ISO dates", () => {
    expect(extractDates("2026-10-14")).toEqual([{ year: 2026, month: 10, day: 14 }]);
  });
  it("ignores invalid and non-dates", () => {
    expect(extractDates("2026-13-45")).toEqual([]);
    expect(extractDates("you may 5 times")).toEqual([]);
    expect(extractDates("31 games")).toEqual([]);
  });
});

describe("pvToSan", () => {
  it("converts and stops at the first illegal move", () => {
    expect(pvToSan(START_FEN, ["e2e4", "e7e5", "e1e8"])).toEqual(["e4", "e5"]);
  });
  it("never throws", () => {
    expect(pvToSan("garbage", ["e2e4"])).toEqual([]);
    expect(pvToSan(START_FEN, ["zz", "", "e2e4"])).toEqual([]);
  });
  it("handles promotion", () => {
    expect(pvToSan("8/P6k/8/8/8/8/8/K7 w - - 0 1", ["a7a8q"])).toEqual(["a8=Q"]);
  });
});

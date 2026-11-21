import { describe, expect, it } from "vitest";
import { evalBarLabel, formatEval, formatNotation } from "../src/board/notation.js";

describe("formatEval", () => {
  it("rounds centipawns to one decimal with a real minus sign", () => {
    expect(formatEval({ cp: -420 })).toBe("−4.2");
    // Math.round(|cp| / 10) / 10, so the float rounding is pinned
    expect(formatEval({ cp: 35 })).toBe("+0.4");
    expect(formatEval({ cp: 4 })).toBe("0.0");
    expect(formatEval({ cp: -4 })).toBe("0.0");
    expect(formatEval({ cp: 570 })).toBe("+5.7");
  });

  it("writes mates with # and a real minus sign", () => {
    expect(formatEval({ mate: 3 })).toBe("#3");
    expect(formatEval({ mate: -2 })).toBe("#−2");
  });
});

describe("formatNotation", () => {
  it("joins number, move, glyph and evaluation", () => {
    expect(
      formatNotation({
        moveNumber: 31,
        color: "white",
        san: "Rxe6",
        judgement: "blunder",
        score: { cp: -420 },
      }),
    ).toBe("31. Rxe6?? −4.2");
    expect(formatNotation({ moveNumber: 31, color: "black", san: "Kg7" })).toBe("31… Kg7");
    expect(
      formatNotation({ moveNumber: 12, color: "white", san: "Nf5", judgement: "inaccuracy" }),
    ).toBe("12. Nf5?!");
    expect(formatNotation({ moveNumber: 9, color: "black", san: "b5", judgement: "mistake" })).toBe(
      "9… b5?",
    );
    expect(formatNotation({ moveNumber: 4, color: "white", san: "O-O", judgement: null })).toBe(
      "4. O-O",
    );
  });
});

describe("evalBarLabel", () => {
  it("keeps the eval's sign and writes mates as M (DESIGN.md, Eval bar)", () => {
    expect(evalBarLabel({ cp: 570 })).toBe("+5.7");
    expect(evalBarLabel({ cp: -570 })).toBe("−5.7");
    expect(evalBarLabel({ mate: 2 })).toBe("M2");
    expect(evalBarLabel({ mate: -2 })).toBe("M2");
  });
});

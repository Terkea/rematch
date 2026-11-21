import { describe, expect, it } from "vitest";
import { flipScore, moveAccuracy, scoreToCp, winningChances, winPercent } from "../src/score.js";

describe("score", () => {
  it("computes win percent", () => {
    expect(winPercent({ cp: 0 })).toBe(50);
    expect(winPercent({ cp: 100 })).toBeCloseTo(59.1, 1);
    expect(winPercent({ cp: -100 })).toBeCloseTo(40.9, 1);
    expect(winPercent({ cp: 2000 })).toBeCloseTo(winPercent({ cp: 1000 }), 10);
    expect(winPercent({ mate: 3 })).toBeCloseTo(97.54, 1);
  });

  it("computes winning chances in [-1, 1]", () => {
    expect(winningChances({ cp: 0 })).toBe(0);
    expect(winningChances({ mate: -1 })).toBeCloseTo(-winningChances({ mate: 1 }), 10);
  });

  it("clamps cp and maps mate to +-1000", () => {
    expect(scoreToCp({ cp: 5000 })).toBe(1000);
    expect(scoreToCp({ cp: -5000 })).toBe(-1000);
    expect(scoreToCp({ cp: 37 })).toBe(37);
    expect(scoreToCp({ mate: 4 })).toBe(1000);
    expect(scoreToCp({ mate: -4 })).toBe(-1000);
  });

  it("flips the point of view", () => {
    expect(flipScore({ cp: 30 })).toEqual({ cp: -30 });
    expect(flipScore({ mate: 2 })).toEqual({ mate: -2 });
  });

  it("computes move accuracy", () => {
    expect(moveAccuracy(50, 50)).toBe(100);
    expect(moveAccuracy(50, 60)).toBe(100);
    expect(moveAccuracy(60, 50)).toBeCloseTo(64.58, 1);
    expect(moveAccuracy(100, 0)).toBe(0);
  });
});

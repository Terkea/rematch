import { describe, expect, it } from "vitest";
import { gameAccuracy } from "../src/game-accuracy.js";

describe("gameAccuracy", () => {
  it("is 100 when the evaluation never moves", () => {
    const series = Array.from({ length: 21 }, () => 50);
    expect(gameAccuracy(series, "white")).toBe(100);
    expect(gameAccuracy(series, "black")).toBe(100);
  });

  it("penalises the colour that made the big drop", () => {
    // Black's move (between positions 5 and 6) lets white% jump from 50 to 90.
    const series = [50, 50, 50, 50, 50, 50, 90, 90, 90, 90, 90];
    const white = gameAccuracy(series, "white");
    const black = gameAccuracy(series, "black");
    expect(white).toBe(100);
    expect(black).toBeLessThan(white);
    expect(black).toBeGreaterThan(0);
  });

  it("returns 100 for a colour with no moves", () => {
    expect(gameAccuracy([50, 50], "black")).toBe(100);
  });
});

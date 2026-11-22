import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { gameAccuracy, gameAccuracyFrom } from "../src/game-accuracy.js";
import { winPercent } from "../src/score.js";

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

// Audit regressions (A2, A4): real eval sequences, anonymised to white-POV centipawns, with the
// values an independent port of lila `AccuracyPercent.gameAccuracy` gives for them.
describe("gameAccuracy against lila (audit examples)", () => {
  type Case = { name: string; whiteCp: number[]; lila: { white: number; black: number } };
  const fixture = JSON.parse(
    readFileSync(
      new URL("../../../backend/fixtures/accuracy/lila-examples.json", import.meta.url),
      "utf8",
    ),
  ) as { cases: Case[] };
  const load = (name: string) => {
    const c = fixture.cases.find((x) => x.name === name);
    if (!c) throw new Error(`no case ${name}`);
    return { wins: c.whiteCp.map((cp) => winPercent({ cp })), lila: c.lila };
  };

  it("floors a zero-accuracy move at 1 in the harmonic mean", () => {
    const { wins, lila } = load("harmonic-floor");
    // Before the fix: 38.91 (the harmonic half was zeroed).
    expect(gameAccuracy(wins, "black")).toBeCloseTo(lila.black, 9);
    expect(gameAccuracy(wins, "white")).toBeCloseTo(lila.white, 9);
  });

  it("sizes the window from moves, not positions", () => {
    const { wins, lila } = load("window-from-moves");
    // Before the fix: 61.21 and 83.29 (window 3 from 30 positions).
    expect(gameAccuracy(wins, "black")).toBeCloseTo(lila.black, 9);
    expect(gameAccuracy(wins, "white")).toBeCloseTo(lila.white, 9);
  });

  it("never zeroes the harmonic half for one blundered move", () => {
    // Ten perfect white moves and one at accuracy 0: the harmonic mean is 11 / (10 / 100 + 1 / 1)
    // = 10, not 0, so the game is 5 points above half the weighted mean (14.71 before the fix).
    const wins = [...Array.from({ length: 20 }, () => 50), 99, 1, 1];
    expect(gameAccuracy(wins, "white")).toBeCloseTo(14.705882352941176 + 5, 9);
  });

  it("from a black start, index 0 is a black move", () => {
    const wins = [50, 90, 90, 90, 90];
    expect(gameAccuracyFrom(wins, "black", "white")).toBe(100);
    expect(gameAccuracyFrom(wins, "black", "black")).toBeLessThan(50);
    // From a white start the jump is a white gain.
    expect(gameAccuracyFrom(wins, "white", "white")).toBe(100);
  });

  it("from is null for a colour without moves", () => {
    expect(gameAccuracyFrom([50, 50], "white", "black")).toBeNull();
    expect(gameAccuracyFrom([50], "white", "white")).toBeNull();
    expect(gameAccuracyFrom([], "white", "white")).toBeNull();
  });

  it("from a white start matches gameAccuracy", () => {
    const { wins, lila } = load("window-from-moves");
    expect(gameAccuracyFrom(wins, "white", "black")).toBeCloseTo(lila.black, 9);
  });
});

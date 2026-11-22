import { describe, expect, it } from "vitest";
import { compareView } from "../../src/app/(app)/app/_filters/compare-view.js";
import { statRange, statValue } from "../../src/app/(app)/app/_filters/stat-text.js";
import { colourSplitView } from "../../src/app/(app)/app/_overview/colour-split-view.js";
import type { Estimate } from "../../src/server/stats/estimate.js";
import type { GameStatRow } from "../../src/server/stats/game-stats.js";
import { summariseByColor } from "../../src/server/stats/game-stats.js";
import { comparePeriodRows } from "../../src/server/stats/period-compare.js";

// The text of the overview's White | Black row and of the A | B | change block (T3). Honesty
// rules: n and units on every number, a dash and the shortfall below the minimum, the real minus,
// no NaN, and a change named only by its verdict. Synthetic rows only (B15).

const est = (value: number, low: number, high: number, n = 30): Estimate => ({
  kind: "estimate",
  value,
  interval: { low, high, level: 0.8 },
  n,
});

function rows(n: number, f: (i: number) => Partial<GameStatRow> = () => ({})): GameStatRow[] {
  return Array.from({ length: n }, (_, i) => ({
    id: `g${i}`,
    playedAt: new Date(Date.UTC(2026, 9, 1 + (i % 28))),
    color: i % 2 === 0 ? "white" : "black",
    result: i % 3 === 0 ? "loss" : i % 3 === 1 ? "draw" : "win",
    analysed: true,
    accuracy: 60 + (i % 7),
    blunders: i % 3,
    mistakes: 1,
    inaccuracies: 2,
    phaseAccuracy: { opening: null, middlegame: null, endgame: null },
    ...f(i),
  }));
}

describe("stat text", () => {
  it("writes each unit at its precision, with the range rounded outward", () => {
    expect(statValue(est(64.23, 61.04, 67.46), "accuracy")).toBe("64.2%");
    expect(statRange(est(64.23, 61.04, 67.46), "accuracy")).toBe("80% range 61.0% to 67.5%");
    expect(statValue(est(1.234, 0.91, 1.62), "perGame")).toBe("1.23 a game");
    expect(statRange(est(1.234, 0.91, 1.62), "perGame")).toBe("80% range 0.9 to 1.7");
    expect(statValue(est(0.583, 0.5, 0.66), "score")).toBe("58.3%");
  });

  it("shows a dash and the shortfall below the minimum, never 0", () => {
    const short: Estimate = { kind: "insufficient", n: 4, needed: 6 };
    expect(statValue(short, "accuracy")).toBe("—");
    expect(statRange(short, "accuracy")).toBe("Need 6 more games");
  });
});

describe("colourSplitView", () => {
  it("gives White and Black their own accuracy, blunders, score with W-D-L and n", () => {
    const v = colourSplitView(summariseByColor(rows(40), "t"), "both", "rapid");
    expect(v.columns.map((c) => c.label)).toEqual(["White", "Black"]);
    const white = v.columns[0];
    expect(white?.games).toBe("20 rapid games");
    expect(white?.accuracy.value).toMatch(/^\d+\.\d%$/);
    expect(white?.accuracy.detail).toMatch(/^80% range .+, 20 analysed games$/);
    expect(white?.blunders.value).toMatch(/ a game$/);
    expect(white?.score.detail).toMatch(/^\d+ wins, \d+ draws, \d+ losses$/);
    expect(JSON.stringify(v)).not.toMatch(/NaN|Infinity|undefined/);
  });

  it("shows only the chosen colour when the filter names one", () => {
    const v = colourSplitView(summariseByColor(rows(40), "t"), "black", "rapid");
    expect(v.columns.map((c) => c.label)).toEqual(["Black"]);
  });

  it("says what is missing for a colour below the minimum", () => {
    const v = colourSplitView(
      summariseByColor(
        rows(24, (i) => ({ color: i < 20 ? "white" : "black" })),
        "t",
      ),
      "both",
      "rapid",
    );
    expect(v.columns[1]).toMatchObject({
      games: "4 rapid games",
      accuracy: { value: "—", detail: "Need 6 more games, 4 analysed games" },
    });
  });
});

describe("compareView", () => {
  const periods = {
    a: { kind: "days" as const, from: "2026-09-01", to: "2026-09-30" },
    b: { kind: "days" as const, from: "2026-10-01", to: "2026-10-31" },
  };

  it("labels each period with its days and games, and each change by its verdict", () => {
    const a = rows(30, (i) => ({ accuracy: 50 + (i % 3) }));
    const b = rows(30, (i) => ({ accuracy: 70 + (i % 3) }));
    const v = compareView({
      periods,
      spans: { a: null, b: null },
      comparison: comparePeriodRows(a, b, "t"),
    });
    expect(v.a).toEqual({ label: "Period A", detail: "1 Sep to 30 Sep 2026, 30 games" });
    expect(v.b).toEqual({ label: "Period B", detail: "1 Oct to 31 Oct 2026, 30 games" });
    const acc = v.rows[0];
    expect(acc?.label).toBe("Accuracy");
    expect(acc?.change.text).toBe("Better");
    expect(acc?.change.detail).toMatch(/^Difference \+20\.0 points, 80% range \+\d/);
  });

  it("writes the real minus, and no difference without enough games", () => {
    const a = rows(30, (i) => ({ blunders: 3 + (i % 2) }));
    const b = rows(30, (i) => ({ blunders: i % 2 }));
    const v = compareView({
      periods,
      spans: { a: null, b: null },
      comparison: comparePeriodRows(a, b, "t"),
    });
    expect(v.rows[1]?.change.detail).toMatch(/^Difference −3\.00 a game/);
    const short = compareView({
      periods,
      spans: { a: null, b: null },
      comparison: comparePeriodRows(rows(30), rows(4), "t"),
    });
    expect(short.rows[0]?.change).toEqual({
      verdict: "not_enough_games",
      text: "Not enough games to tell: need 6 more in period B",
      detail: null,
    });
  });

  it("names the games of a count period by their days", () => {
    const v = compareView({
      periods: {
        a: { kind: "games", skip: 50, take: 50 },
        b: { kind: "games", skip: 0, take: 50 },
      },
      spans: { a: { from: "2026-08-02", to: "2026-09-12" }, b: null },
      comparison: comparePeriodRows(rows(50), [], "t"),
    });
    expect(v.a.detail).toBe("The 50 games before, 2 Aug to 12 Sep 2026, 50 games");
    expect(v.b.detail).toBe("The last 50 games, 0 games");
  });
});

import { describe, expect, it } from "vitest";
import type { GameStatRow } from "../../src/server/stats/game-stats.js";
import { comparePeriodRows, verdictText } from "../../src/server/stats/period-compare.js";

// The overview's and progress page's period comparison (T3): A | B | change per game-level stat,
// where a change is named only by the ADR 0083 verdict rule. Synthetic rows only (B15).

function rows(
  n: number,
  f: (i: number) => Partial<GameStatRow> = () => ({}),
): GameStatRow[] {
  return Array.from({ length: n }, (_, i) => ({
    id: `g${i}`,
    playedAt: new Date(Date.UTC(2026, 9, 1 + (i % 28))),
    color: i % 2 === 0 ? "white" : "black",
    result: "win",
    analysed: true,
    accuracy: 70,
    blunders: 1,
    mistakes: 1,
    inaccuracies: 2,
    phaseAccuracy: { opening: null, middlegame: null, endgame: null },
    ...f(i),
  }));
}

describe("comparePeriodRows", () => {
  it("compares accuracy, blunders per game and score, with n on each side", () => {
    const a = rows(30, (i) => ({ accuracy: 60 + (i % 5), blunders: 2 + (i % 2) }));
    const b = rows(30, (i) => ({ accuracy: 75 + (i % 5), blunders: i % 2 }));
    const c = comparePeriodRows(a, b, "t");
    expect(c.n).toEqual({ a: 30, b: 30 });
    expect(c.rows.map((r) => r.key)).toEqual(["accuracy", "blunders", "score"]);
    const [acc, bl] = c.rows;
    expect(acc?.result.verdict).toBe("better");
    expect(acc?.a).toMatchObject({ kind: "estimate", n: 30 });
    // Fewer blunders is better.
    expect(bl?.result.verdict).toBe("better");
  });

  it("calls more blunders worse, and an overlap no clear change", () => {
    const a = rows(30, (i) => ({ blunders: i % 2 }));
    const b = rows(30, (i) => ({ blunders: 3 + (i % 2) }));
    expect(comparePeriodRows(a, b, "t").rows[1]?.result.verdict).toBe("worse");
    const same = rows(30, (i) => ({ accuracy: 60 + (i % 10) }));
    expect(comparePeriodRows(same, same, "t").rows[0]?.result.verdict).toBe("no_clear_change");
  });

  it("scores a draw as half a point and leaves unfinished games out", () => {
    const a = rows(12, (i) => ({ result: i % 2 === 0 ? "draw" : "unfinished" }));
    const c = comparePeriodRows(a, rows(12), "t");
    const score = c.rows[2];
    expect(score?.a).toMatchObject({ kind: "insufficient", n: 6 });
    expect(score?.result).toMatchObject({ verdict: "not_enough_games", short: "a" });
  });

  it("counts only analysed games for accuracy, never an unknown as 0", () => {
    const b = rows(14, (i) => (i < 8 ? { analysed: false, accuracy: null, blunders: null } : {}));
    const acc = comparePeriodRows(rows(20), b, "t").rows[0];
    expect(acc?.b).toMatchObject({ kind: "insufficient", n: 6, needed: 4 });
    expect(acc?.result).toMatchObject({ verdict: "not_enough_games", needed: { a: 0, b: 4 } });
  });
});

describe("verdictText", () => {
  const c = (a: number, b: number) => comparePeriodRows(rows(a), rows(b), "t").rows[0]?.result;

  it("names how many more games the short period needs", () => {
    expect(verdictText(c(20, 4)!)).toBe("Not enough games to tell: need 6 more in period B");
    expect(verdictText(c(9, 20)!)).toBe("Not enough games to tell: need 1 more in period A");
    expect(verdictText(c(3, 5)!)).toBe(
      "Not enough games to tell: need 7 more in period A and 5 more in period B",
    );
  });

  it("words the called verdicts", () => {
    const up = comparePeriodRows(
      rows(30, (i) => ({ accuracy: 60 + (i % 3) })),
      rows(30, (i) => ({ accuracy: 80 + (i % 3) })),
      "t",
    ).rows[0]?.result;
    expect(verdictText(up!)).toBe("Better");
    const flat = rows(30, (i) => ({ accuracy: 60 + (i % 10) }));
    expect(verdictText(comparePeriodRows(flat, flat, "t").rows[0]!.result)).toBe(
      "No clear change",
    );
  });
});

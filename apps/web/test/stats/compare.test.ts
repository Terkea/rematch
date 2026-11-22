import { describe, expect, it } from "vitest";
import {
  CONFIRM_P_CHANGE,
  CONFIRM_REL_CHANGE,
  compareMeans,
  compareRates,
  type RateSide,
} from "../../src/server/stats/compare";
import { KPI_MIN_GAMES } from "../../src/server/stats/constants";

const prior = { mu: 0.1, n0: 20 };
const side = (k: number, n: number, games = n): RateSide => ({ k, n, games });

describe("the compare thresholds", () => {
  it("are the engine's (timeline/series.rs) and the KPI minimum", () => {
    expect(CONFIRM_P_CHANGE).toBe(0.8);
    expect(CONFIRM_REL_CHANGE).toBe(0.15);
    expect(KPI_MIN_GAMES).toBe(10);
  });
});

describe("compareRates", () => {
  const table: {
    name: string;
    a: RateSide;
    b: RateSide;
    higherIsBetter: boolean;
    want: string;
  }[] = [
    {
      name: "blunder rate fell a lot over many games",
      a: side(60, 300, 60),
      b: side(20, 300, 60),
      higherIsBetter: false,
      want: "better",
    },
    {
      name: "blunder rate rose a lot",
      a: side(20, 300, 60),
      b: side(60, 300, 60),
      higherIsBetter: false,
      want: "worse",
    },
    {
      name: "score rose a lot (higher is better)",
      a: side(20, 60),
      b: side(40, 60),
      higherIsBetter: true,
      want: "better",
    },
    {
      name: "score fell a lot (higher is better)",
      a: side(40, 60),
      b: side(20, 60),
      higherIsBetter: true,
      want: "worse",
    },
    {
      name: "the same rate on both sides",
      a: side(30, 300, 60),
      b: side(30, 300, 60),
      higherIsBetter: false,
      want: "no_clear_change",
    },
    {
      name: "a likely but small change (under 15% relative)",
      a: side(1000, 10000, 500),
      b: side(930, 10000, 500),
      higherIsBetter: false,
      want: "no_clear_change",
    },
    {
      name: "a big but noisy change",
      a: side(2, 12, 12),
      b: side(1, 12, 12),
      higherIsBetter: false,
      want: "no_clear_change",
    },
  ];

  it.each(table)("$name -> $want", ({ a, b, higherIsBetter, want }) => {
    const r = compareRates(a, b, { prior, higherIsBetter });
    expect(r.verdict).toBe(want);
    if (r.verdict === "not_enough_games") return;
    expect(r.n).toEqual({ a: a.games, b: b.games });
    expect(r.pBetter).toBeGreaterThanOrEqual(0);
    expect(r.pBetter).toBeLessThanOrEqual(1);
    if (want === "better") expect(r.pBetter).toBeGreaterThanOrEqual(CONFIRM_P_CHANGE);
    if (want !== "no_clear_change") expect(r.relChange).toBeGreaterThanOrEqual(CONFIRM_REL_CHANGE);
  });

  it.each([
    { a: 4, b: 30, short: "a", needed: { a: 6, b: 0 } },
    { a: 30, b: 9, short: "b", needed: { a: 0, b: 1 } },
    { a: 0, b: 3, short: "both", needed: { a: 10, b: 7 } },
  ] as const)("names the short side: A $a games, B $b games", ({ a, b, short, needed }) => {
    const r = compareRates(side(0, a * 5, a), side(0, b * 5, b), { prior, higherIsBetter: false });
    expect(r).toMatchObject({ verdict: "not_enough_games", short, needed, n: { a, b } });
  });

  it("is deterministic", () => {
    const run = () =>
      compareRates(side(12, 80, 30), side(6, 80, 30), { prior, higherIsBetter: false });
    expect(run()).toEqual(run());
  });

  it("never reports NaN for odd input", () => {
    const r = compareRates(side(Number.NaN, 50, 20), side(3, 50, 20), {
      prior,
      higherIsBetter: false,
    });
    expect(JSON.stringify(r)).not.toContain("null");
    if (r.verdict !== "not_enough_games") expect(Number.isFinite(r.pBetter)).toBe(true);
  });
});

/** `n` values around `centre`, spread +-`spread`, deterministic. */
function sample(n: number, centre: number, spread: number): number[] {
  return Array.from({ length: n }, (_, i) => centre + spread * Math.sin(i * 1.7));
}

describe("compareMeans", () => {
  const table = [
    {
      name: "accuracy rose",
      a: sample(30, 70, 8),
      b: sample(30, 78, 8),
      hib: true,
      want: "better",
    },
    { name: "accuracy fell", a: sample(30, 78, 8), b: sample(30, 70, 8), hib: true, want: "worse" },
    {
      name: "blunders per game fell (lower is better)",
      a: sample(30, 2, 0.8),
      b: sample(30, 1, 0.8),
      hib: false,
      want: "better",
    },
    {
      name: "same accuracy",
      a: sample(30, 74, 8),
      b: sample(30, 74, 8),
      hib: true,
      want: "no_clear_change",
    },
    {
      name: "tiny shift within the noise",
      a: sample(15, 74, 15),
      b: sample(15, 75, 15),
      hib: true,
      want: "no_clear_change",
    },
  ];

  it.each(table)("$name -> $want", ({ a, b, hib, want }) => {
    const r = compareMeans(a, b, { higherIsBetter: hib, seed: "t" });
    expect(r.verdict).toBe(want);
    if (r.verdict === "not_enough_games") return;
    expect(r.n).toEqual({ a: a.length, b: b.length });
    expect(r.diff.interval.low).toBeLessThanOrEqual(r.diff.interval.high);
    const excludesZero = r.diff.interval.low > 0 || r.diff.interval.high < 0;
    expect(excludesZero).toBe(want !== "no_clear_change");
  });

  it("drops non-finite values and does not count them", () => {
    const a = [...sample(9, 70, 5), Number.NaN, Number.POSITIVE_INFINITY];
    const r = compareMeans(a, sample(20, 70, 5), { higherIsBetter: true, seed: "t" });
    expect(r).toMatchObject({
      verdict: "not_enough_games",
      short: "a",
      needed: { a: 1, b: 0 },
      n: { a: 9, b: 20 },
    });
  });

  it("is deterministic for a seed", () => {
    const run = () =>
      compareMeans(sample(20, 70, 9), sample(20, 73, 9), { higherIsBetter: true, seed: "s" });
    expect(run()).toEqual(run());
  });
});

import { describe, expect, it } from "vitest";
import { type LineEvidence, verdictWhy } from "../src/lib/explain";
import { lineEvidenceOf } from "../src/server/profile/openings";

const BAD = /NaN|undefined|Infinity|null/;

const base: LineEvidence = {
  verdict: "keep",
  color: "white",
  kind: "chosen",
  games: 68,
  perfVsExpected: -0.04,
  perfGames: 68,
  perfInterval: { low: -0.115, high: 0.031 },
  errorSpot: null,
  analysedGames: 60,
  avgWinAtOpeningEnd: 51,
  medianDeviationPly: null,
  share: 0.4,
  struggles: [],
};

describe("verdictWhy", () => {
  it.each<[string, Partial<LineEvidence>, string]>([
    [
      "fix names the move and its count",
      {
        verdict: "fix",
        games: 23,
        errorSpot: { san: "Qf3", games: 7 },
        struggles: ["repeated_deviation_error"],
      },
      "Fix means one move keeps costing you: Qf3, in 7 of your 23 games in this line, losing 10 win% points or more each time.",
    ],
    [
      "fix without a stored spot",
      { verdict: "fix", games: 23, errorSpot: null },
      "Fix means one move keeps costing you in this line, losing 10 win% points or more each time it is played.",
    ],
    [
      "keep inside normal variation",
      {},
      "Keep means no single move keeps costing you, and your −4 pp against what your rating predicts is within normal variation for 68 rated games (likely −12 to +4 pp).",
    ],
    [
      "keep above expectation",
      { perfVsExpected: 0.07, perfInterval: { low: 0.012, high: 0.13 } },
      "Keep means no single move keeps costing you, and you score above what your rating predicts: +7 pp over 68 rated games (likely +1 to +13 pp).",
    ],
    [
      "keep a little below, short of a struggle",
      { perfVsExpected: -0.05, perfInterval: { low: -0.09, high: -0.01 } },
      "Keep means no single move keeps costing you, and your −5 pp against what your rating predicts (likely −9 to −1 pp over 68 rated games) is short of the −10 pp that counts as a struggle.",
    ],
    [
      "keep without a range",
      { perfVsExpected: -0.04, perfGames: 1, perfInterval: null },
      "Keep means no single move keeps costing you, and your −4 pp against what your rating predicts has no range yet (1 rated game).",
    ],
    [
      "keep without a rating comparison",
      { perfVsExpected: null, perfGames: 0, perfInterval: null },
      "Keep means no single move keeps costing you over your 68 games here, and there is no comparison with your rating yet.",
    ],
    [
      "replace says the rule",
      {
        verdict: "replace",
        games: 20,
        perfGames: 20,
        perfVsExpected: -0.18,
        perfInterval: { low: -0.26, high: -0.09 },
        struggles: ["score_below_expected"],
      },
      "Replace means this line keeps scoring well below what your rating predicts: −18 pp over 20 rated games (likely −26 to −9 pp, all below zero). Replace needs at least 15 games, −15 pp or worse, and the whole range below zero.",
    ],
    [
      "learn from an early exit from theory",
      { verdict: "learn", medianDeviationPly: 9, share: 0.31 },
      "Learn deeper means you play it often (31% of your games as White) but usually leave the known moves by move 5.",
    ],
    [
      "learn from leaving the opening worse",
      {
        verdict: "learn",
        kind: "faced",
        color: "black",
        avgWinAtOpeningEnd: 36.4,
        analysedGames: 9,
        struggles: ["worse_after_opening"],
      },
      "Learn deeper means you leave the opening worse off: your win chance averages 36% when it ends, over 9 analysed games.",
    ],
    [
      "learn from a low score short of replacing",
      {
        verdict: "learn",
        games: 11,
        perfGames: 11,
        perfVsExpected: -0.12,
        perfInterval: { low: -0.2, high: -0.03 },
        struggles: ["score_below_expected"],
      },
      "Learn deeper means you score below what your rating predicts here: −12 pp (likely −20 to −3 pp over 11 rated games), short of the 15 games and −15 pp that would mean replacing it.",
    ],
  ])("%s", (_name, patch, want) => {
    const text = verdictWhy({ ...base, ...patch });
    expect(text).toBe(want);
    expect(text).not.toMatch(BAD);
  });

  it("never prints NaN from broken numbers", () => {
    const text = verdictWhy({
      ...base,
      perfVsExpected: Number.NaN,
      perfInterval: { low: Number.NaN, high: 1 },
    });
    expect(text).not.toMatch(BAD);
  });
});

describe("lineEvidenceOf", () => {
  const summary = {
    games: 23,
    color: "white",
    kind: "chosen",
    perfVsExpected: -0.04,
    perfGames: 20,
    perfInterval: { low: -0.1, high: 0.02 },
    errorSpot: { fenKey: "x", uci: "d1f3", san: "Qf3", games: 7 },
    analysedGames: 18,
    avgWinAtOpeningEnd: 47.5,
    medianDeviationPly: 12,
    share: 0.2,
    struggles: ["repeated_deviation_error"],
    verdict: "fix",
  };

  it("reads a stored line summary", () => {
    expect(lineEvidenceOf(summary)).toEqual({
      verdict: "fix",
      color: "white",
      kind: "chosen",
      games: 23,
      perfVsExpected: -0.04,
      perfGames: 20,
      perfInterval: { low: -0.1, high: 0.02 },
      errorSpot: { san: "Qf3", games: 7 },
      analysedGames: 18,
      avgWinAtOpeningEnd: 47.5,
      medianDeviationPly: 12,
      share: 0.2,
      struggles: ["repeated_deviation_error"],
    });
  });

  it("drops a malformed number to null instead of guessing", () => {
    const e = lineEvidenceOf({ ...summary, perfVsExpected: "x", perfInterval: { low: 1 } });
    expect(e?.perfVsExpected).toBeNull();
    expect(e?.perfInterval).toBeNull();
  });

  it.each([null, 3, { games: "x" }, { ...summary, verdict: "maybe" }])("rejects %o", (bad) => {
    expect(lineEvidenceOf(bad)).toBeNull();
  });
});

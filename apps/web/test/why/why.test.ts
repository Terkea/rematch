import { describe, expect, it } from "vitest";
import { ALGORITHM_ID } from "../../src/server/algorithm";
import {
  type EstimateFacts,
  NOT_RANKED,
  RANK_RULE,
  whyDrill,
  whyFocus,
  whyPlan,
  whyWeakness,
} from "../../src/server/why";

const estimate = (over: Partial<EstimateFacts> = {}): EstimateFacts => ({
  seriesKey: "W1",
  kUnweighted: 12,
  nUnweighted: 400,
  mean: 0.03,
  ciLow: 0.022,
  ciHigh: 0.04,
  bandMedian: 0.01,
  excess: 0.05,
  priority: 0.4,
  algorithmId: ALGORITHM_ID,
  ...over,
});

describe("Why this? for a weakness", () => {
  it("gives the counts, the range, the comparison, the cost, the rule and the algorithm", () => {
    const w = whyWeakness(estimate());
    expect(w.facts).toEqual({
      events: 12,
      chances: 400,
      rate: { mean: 0.03, low: 0.022, high: 0.04, level: 0.8 },
      comparison: { group: "Players at your rating", rate: 0.01 },
      pointsLostPerGame: 0.05,
    });
    expect(w.rule).toEqual({ ranked: true, text: RANK_RULE });
    expect(w.algorithm.id).toBe(ALGORITHM_ID);
    expect(w.algorithm.href).toContain("algorithm/CHANGELOG.md#algo-");
  });

  it("says not ranked: not enough evidence yet, below the floor", () => {
    const w = whyWeakness(estimate({ priority: 0 }));
    expect(w.rule).toEqual({ ranked: false, text: NOT_RANKED });
    expect(NOT_RANKED).toMatch(/^Not ranked: not enough evidence yet/);
  });

  it("names the evaluation as W5's comparison, and no comparison without a benchmark", () => {
    expect(whyWeakness(estimate({ seriesKey: "W5" })).facts.comparison?.group).toBe(
      "What the evaluation predicts",
    );
    const none = whyWeakness(estimate({ bandMedian: null, excess: null, priority: 0 }));
    expect(none.facts.comparison).toBeNull();
    expect(none.facts.pointsLostPerGame).toBeNull();
  });

  it("reads a row from before versioning as having no algorithm", () => {
    expect(whyWeakness(estimate({ algorithmId: null })).algorithm).toEqual({
      id: null,
      version: null,
      href: null,
    });
  });
});

describe("Why this? for the plan", () => {
  it("orders the ranked weaknesses by priority and leaves the unranked out", () => {
    const plan = whyPlan([
      estimate({ seriesKey: "W9b", priority: 0.2 }),
      estimate({ seriesKey: "W4.missed", priority: 0 }),
      estimate({ seriesKey: "W1", priority: 0.4 }),
    ]);
    expect(plan.map((p) => [p.rank, p.seriesKey, p.kind])).toEqual([
      [1, "W1", "plan"],
      [2, "W9b", "plan"],
    ]);
  });
});

describe("Why this? for the focus", () => {
  const focus = {
    seriesKey: "W5",
    targetRate: 0.15,
    gamesN: 20,
    baselineRate: 0.2,
    baselineK: 4,
    baselineN: 20,
    peerRate: 0.18,
    algorithmId: ALGORITHM_ID,
  };
  it("gives the baseline and a relative target", () => {
    const f = whyFocus(focus);
    expect(f.target).toEqual({ rate: 0.15, basis: "relative", gamesN: 20 });
    expect(f.facts).toMatchObject({ events: 4, chances: 20 });
    expect(f.rule.text).toMatch(/a quarter fewer/);
    expect(f.algorithm.id).toBe(ALGORITHM_ID);
  });
  it("names a peer target", () => {
    const f = whyFocus({ ...focus, targetRate: 0.1, peerRate: 0.1 });
    expect(f.target?.basis).toBe("peer");
    expect(f.rule.text).toMatch(/players at your rating/);
  });
});

describe("Why this? for a drill", () => {
  it("gives the position and the win share it cost", () => {
    const d = whyDrill({
      sourceKey: "W4.missed",
      gameId: "g1",
      ply: 21,
      winBefore: 95,
      winAfter: 40,
      algorithmId: ALGORITHM_ID,
    });
    expect(d.position).toEqual({ gameId: "g1", ply: 21, winBefore: 95, winAfter: 40 });
    expect(d.kind).toBe("drill");
    expect(d.algorithm.version).toBe(d.algorithm.id?.slice(5).split("+")[0]);
  });
});

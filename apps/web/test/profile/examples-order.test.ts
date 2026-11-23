import { describe, expect, it } from "vitest";
import { orderExamples } from "../../src/app/(app)/app/weaknesses/[seriesKey]/exemplars";
import type { Example } from "../../src/server/profile/types";

// Critic 1: examples read worst first: a lost or allowed mate, then the biggest win% drop. An
// example that opens in the review wins a tie.

const ex = (gameId: string, winBefore: number, winAfter: number, mate: Example["mate"] = null) =>
  ({
    gameId,
    ply: 30,
    playedAt: "2026-11-01T10:00:00.000Z",
    fen: "6k1/5ppp/8/8/8/8/5PPP/R5K1 w - - 0 1",
    played: { san: "Ra2", uci: "a1a2" },
    best: null,
    bestPv: [],
    refutationPv: [],
    tags: [],
    winBefore,
    winAfter,
    source: "games",
    judgement: "blunder",
    mate,
  }) satisfies Example;

describe("orderExamples", () => {
  it("puts a lost mate first, then the biggest drop", () => {
    const list = [
      ex("small", 60, 50),
      ex("big", 80, 20),
      ex("mate", 98, 95, { kind: "lost", n: 3 }),
    ];
    expect(orderExamples(list, new Map()).map((e) => e.gameId)).toEqual(["mate", "big", "small"]);
  });

  it("breaks a tie by whether the game opens in the review", () => {
    const list = [ex("away", 70, 30), ex("home", 70, 30)];
    const ids = new Map([["home", "uuid-home"]]);
    expect(orderExamples(list, ids).map((e) => e.gameId)).toEqual(["home", "away"]);
  });
});

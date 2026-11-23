import { describe, expect, it } from "vitest";
import type { GameStatRow } from "../../src/server/stats/game-stats.js";
import {
  computeTrajectory,
  type RatedGame,
  TRAJECTORY_MIN_GAMES,
  TRAJECTORY_WINDOW,
  type TrajectoryWeaknessInput,
} from "../../src/server/trajectory/model.js";

// The rating trajectory (F6/F7): the rating over time, the peak and the current rating, and the
// "then" window (the games around the peak, or the first games) against the last games, with
// every difference called only by the ADR 0083 rule. Synthetic games only (B15).

const DAY = 86_400_000;
const START = Date.UTC(2025, 0, 1);

function rated(ratings: number[]): RatedGame[] {
  return ratings.map((rating, i) => ({
    id: `g${i}`,
    playedAt: new Date(START + i * DAY),
    rating,
  }));
}

function stats(
  games: RatedGame[],
  f: (i: number) => Partial<GameStatRow> = () => ({}),
): GameStatRow[] {
  return games.map((g, i) => ({
    id: g.id,
    playedAt: g.playedAt,
    color: i % 2 === 0 ? "white" : "black",
    result: i % 2 === 0 ? "win" : "loss",
    analysed: true,
    accuracy: 70 + (i % 3),
    blunders: 1 + (i % 2),
    mistakes: 1,
    inaccuracies: 2,
    phaseAccuracy: { opening: null, middlegame: null, endgame: null },
    ...f(i),
  }));
}

/** Up 10 a game to 1,100 at game 40, then down 10 a game. */
const riseAndFall = Array.from({ length: 100 }, (_, i) =>
  i <= 40 ? 700 + i * 10 : 1100 - (i - 40) * 10 + (i % 2) * 4,
);

describe("computeTrajectory", () => {
  it("says how many more rated games it needs below the minimum", () => {
    const games = rated(Array.from({ length: 15 }, () => 900));
    const t = computeTrajectory({
      games,
      current: { rating: 900, at: games.at(-1)?.playedAt as Date },
      stats: stats(games),
      weaknesses: [],
      seed: "t",
    });
    expect(t.kind).toBe("not_enough_games");
    if (t.kind !== "not_enough_games") return;
    expect(t.games).toBe(15);
    expect(t.needed).toBe(TRAJECTORY_MIN_GAMES - 15);
    expect(t.series).toHaveLength(15);
  });

  it("finds a fall from an earlier peak: the 30 games around the peak against the last 30", () => {
    const games = rated(riseAndFall);
    const t = computeTrajectory({
      games,
      current: { rating: 520, at: new Date(START + 100 * DAY) },
      stats: stats(games),
      weaknesses: [],
      seed: "t",
    });
    expect(t.kind).toBe("falling");
    if (t.kind === "not_enough_games") return;
    expect(t.peak).toMatchObject({ rating: 1100, gameId: "g40" });
    expect(t.current.rating).toBe(520);
    expect(t.change).toBe(520 - 1100);
    expect(t.then.basis).toBe("peak");
    expect(t.then.games).toBe(TRAJECTORY_WINDOW);
    // Centred on the peak: games 25 to 54.
    expect(t.then.firstId).toBe("g25");
    expect(t.then.lastId).toBe("g54");
    expect(t.now.basis).toBe("last");
    expect(t.now.firstId).toBe("g70");
    expect(t.now.lastId).toBe("g99");
    expect(t.rating.verdict).toBe("worse");
  });

  it("keeps the peak window clear of the last games", () => {
    // The peak at game 50 of 70: its window ends where the last 30 begin.
    const ratings = Array.from({ length: 70 }, (_, i) => (i === 50 ? 1200 : 900 - i));
    const games = rated(ratings);
    const t = computeTrajectory({
      games,
      current: { rating: 830, at: new Date(START + 70 * DAY) },
      stats: stats(games),
      weaknesses: [],
      seed: "t",
    });
    if (t.kind === "not_enough_games") throw new Error("expected a trajectory");
    expect(t.then.basis).toBe("start");
    // Game 50 sits inside the last 30, so "then" is the first 30 games.
    expect(t.then.firstId).toBe("g0");
    expect(t.then.lastId).toBe("g29");
  });

  it("finds a rise: the first 30 games against the last 30, with the peak now", () => {
    const games = rated(Array.from({ length: 80 }, (_, i) => 800 + i * 4));
    const t = computeTrajectory({
      games,
      current: { rating: 1120, at: new Date(START + 80 * DAY) },
      stats: stats(games),
      weaknesses: [],
      seed: "t",
    });
    expect(t.kind).toBe("rising");
    if (t.kind === "not_enough_games") return;
    expect(t.then.basis).toBe("start");
    expect(t.then.firstId).toBe("g0");
    expect(t.peak.rating).toBe(1120);
    expect(t.peak.gameId).toBeNull();
    expect(t.rating.verdict).toBe("better");
  });

  it("calls a level rating flat", () => {
    const games = rated(Array.from({ length: 80 }, (_, i) => 900 + (i % 2 === 0 ? 12 : -12)));
    const t = computeTrajectory({
      games,
      current: { rating: 900, at: new Date(START + 80 * DAY) },
      stats: stats(games),
      weaknesses: [],
      seed: "t",
    });
    expect(t.kind).toBe("flat");
    if (t.kind === "not_enough_games") return;
    expect(t.rating.verdict).toBe("no_clear_change");
  });

  it("halves the windows when there are fewer than 60 rated games", () => {
    const games = rated(Array.from({ length: 40 }, (_, i) => 900 + i));
    const t = computeTrajectory({
      games,
      current: null,
      stats: stats(games),
      weaknesses: [],
      seed: "t",
    });
    if (t.kind === "not_enough_games") throw new Error("expected a trajectory");
    expect(t.then.games).toBe(20);
    expect(t.now.games).toBe(20);
    // No stored current rating: the latest game's rating is the current one.
    expect(t.current.rating).toBe(939);
  });

  it("compares accuracy, blunders a game and score over the two windows", () => {
    const games = rated(riseAndFall);
    const t = computeTrajectory({
      games,
      current: { rating: 520, at: new Date(START + 100 * DAY) },
      stats: stats(games, (i) =>
        i >= 70 ? { accuracy: 60 + (i % 4), blunders: 3 + (i % 2) } : { accuracy: 80 + (i % 4) },
      ),
      weaknesses: [],
      seed: "t",
    });
    if (t.kind === "not_enough_games") throw new Error("expected a trajectory");
    const byKey = new Map(t.rows.map((r) => [r.key, r]));
    expect([...byKey.keys()]).toEqual(["accuracy", "blunders", "score"]);
    expect(byKey.get("accuracy")?.result.verdict).toBe("worse");
    expect(byKey.get("accuracy")?.a).toMatchObject({ kind: "estimate", n: 30 });
    expect(byKey.get("blunders")?.result.verdict).toBe("worse");
    expect(byKey.get("score")?.result.verdict).toBe("no_clear_change");
  });

  it("compares a weakness rate over each window's own games, by the rate rule", () => {
    const games = rated(riseAndFall);
    const points = new Map<string, { events: number; opportunities: number }>();
    games.forEach((g, i) => {
      // 3 of 10 chances in the peak period, 6 of 10 in the last games.
      points.set(g.id, { events: i >= 70 ? 6 : 3, opportunities: 10 });
    });
    const weakness: TrajectoryWeaknessInput = {
      seriesKey: "W3.shallow",
      label: "Missing a short tactic",
      prior: { mu: 0.3, n0: 60 },
      points,
    };
    const t = computeTrajectory({
      games,
      current: { rating: 520, at: new Date(START + 100 * DAY) },
      stats: stats(games),
      weaknesses: [weakness],
      seed: "t",
    });
    if (t.kind === "not_enough_games") throw new Error("expected a trajectory");
    const w = t.rows.find((r) => r.key === "weakness:W3.shallow");
    if (w?.unit !== "rate") throw new Error("expected a rate row");
    expect(w?.label).toBe("Missing a short tactic");
    expect(w?.unit).toBe("rate");
    expect(w?.counts).toEqual({
      a: { events: 90, opportunities: 300, games: 30 },
      b: { events: 180, opportunities: 300, games: 30 },
    });
    expect(w?.result.verdict).toBe("worse");
  });

  it("says a weakness needs more games when a window has too few of its chances", () => {
    const games = rated(riseAndFall);
    const points = new Map([["g99", { events: 1, opportunities: 2 }]]);
    const t = computeTrajectory({
      games,
      current: { rating: 520, at: new Date(START + 100 * DAY) },
      stats: stats(games),
      weaknesses: [{ seriesKey: "W5", label: "x", prior: { mu: 0.2, n0: 30 }, points }],
      seed: "t",
    });
    if (t.kind === "not_enough_games") throw new Error("expected a trajectory");
    const w = t.rows.find((r) => r.key === "weakness:W5");
    expect(w?.result.verdict).toBe("not_enough_games");
    expect(w?.a).toMatchObject({ kind: "insufficient" });
  });
});

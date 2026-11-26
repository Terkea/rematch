import { describe, expect, it } from "vitest";
import { RATING_VERDICT_SHOWN, trajectoryView } from "../../src/app/(app)/app/_trajectory/view.js";
import type { GameStatRow } from "../../src/server/stats/game-stats.js";
import { computeTrajectory, type RatedGame } from "../../src/server/trajectory/model.js";

// The trajectory's words (F6): the headline, the "since then" sentence and the then | now table.
// Honest wording: a change is "clearly worse" or "clearly better" only when the rule calls it,
// and nothing prints NaN, undefined or an arrow. Synthetic games only (B15).

const DAY = 86_400_000;
/** 1 January 2026: game 40 (the peak) falls on 10 February. */
const START = Date.UTC(2026, 0, 1);

function rated(ratings: number[]): RatedGame[] {
  return ratings.map((rating, i) => ({ id: `g${i}`, playedAt: new Date(START + i * DAY), rating }));
}

function stats(games: RatedGame[], f: (i: number) => Partial<GameStatRow>): GameStatRow[] {
  return games.map((g, i) => ({
    id: g.id,
    playedAt: g.playedAt,
    color: "white",
    result: i % 2 === 0 ? "win" : "loss",
    analysed: true,
    accuracy: 70,
    blunders: 1,
    mistakes: 1,
    inaccuracies: 2,
    phaseAccuracy: { opening: null, middlegame: null, endgame: null },
    ...f(i),
  }));
}

const fall = Array.from({ length: 100 }, (_, i) =>
  i <= 40 ? 800 + i * 7 : 1080 - (i - 40) * 4 + (i % 2) * 3,
);

function falling() {
  const games = rated(fall);
  const points = new Map(
    games.map((g, i) => [g.id, { events: i >= 70 ? 4 : 3, opportunities: 10 }] as const),
  );
  return computeTrajectory({
    games,
    current: { rating: 816, at: new Date(START + 100 * DAY) },
    stats: stats(games, (i) =>
      i >= 70 ? { accuracy: 60 + (i % 5), blunders: 2 + (i % 2) } : { accuracy: 78 + (i % 5) },
    ),
    weaknesses: [
      {
        seriesKey: "W3.shallow",
        label: "Missing a short tactic",
        prior: { mu: 0.3, n0: 60 },
        points,
      },
    ],
    seed: "v",
  });
}

const forbidden = /NaN|undefined|Infinity|→/;
/** The copy a rule that clears EXP-0030 would get back (ADR 0116). */
const SHOWN = { ratingVerdict: true } as const;
/** Words that give the rating a direction. */
const direction =
  /falling|rising|clearly (better|worse)|no clear change|about where|up from|down from/i;

function rise() {
  const games = rated(Array.from({ length: 80 }, (_, i) => 800 + i * 4));
  return computeTrajectory({
    games,
    current: { rating: 1120, at: new Date(START + 80 * DAY) },
    stats: stats(games, () => ({})),
    weaknesses: [],
    seed: "v",
  });
}

function level() {
  const games = rated(Array.from({ length: 80 }, (_, i) => 900 + (i % 2 === 0 ? 12 : -12)));
  return computeTrajectory({
    games,
    current: { rating: 900, at: new Date(START + 80 * DAY) },
    stats: stats(games, () => ({})),
    weaknesses: [],
    seed: "v",
  });
}

describe("trajectoryView, with the rating's verdict withheld (EXP-0015, ADR 0116)", () => {
  it("is withheld by the manifest", () => {
    expect(RATING_VERDICT_SHOWN).toBe(false);
  });

  it("states the peak, the current rating and the change as facts", () => {
    const t = falling();
    if (t.kind === "not_enough_games") throw new Error("expected a trajectory");
    expect(t.kind).toBe("falling");
    const v = trajectoryView(t, "rapid");
    if (v.state !== "ready") throw new Error("expected a trajectory");
    expect(v.headline).toBe(
      "Your rapid rating peaked at 1,080 in February 2026 and is 816 now (−264).",
    );
    expect(v.kind).toBeNull();
    expect(JSON.stringify(v)).not.toMatch(forbidden);
  });

  it("gives a rise and a level line no direction either", () => {
    const up = trajectoryView(rise(), "blitz");
    if (up.state !== "ready") throw new Error("expected a trajectory");
    expect(up.headline).toBe(
      "Your blitz rating is 1,120 now; it averaged 858 over your first 30 rated games (+262).",
    );
    const flat = trajectoryView(level(), "rapid");
    if (flat.state !== "ready") throw new Error("expected a trajectory");
    expect(flat.headline).toBe(
      "Your rapid rating peaked at 912 in January 2026 and is 900 now (−12).",
    );
    for (const v of [up, flat]) {
      expect(v.kind).toBeNull();
      expect(v.headline).not.toMatch(direction);
    }
  });

  it("shows the rating row's windows and difference with no range and no verdict", () => {
    const v = trajectoryView(falling(), "rapid");
    if (v.state !== "ready") throw new Error("expected a trajectory");
    const [r, ...rest] = v.compare.rows;
    expect(r?.key).toBe("rating");
    expect(r?.change).toEqual({
      verdict: "withheld",
      text: "Not called",
      detail: expect.any(String),
    });
    expect(r?.change.detail).toMatch(/^Difference −\d+ points$/);
    expect(r?.a.value).toMatch(/^\d,\d{3} on average$/);
    expect(r?.a.detail).toBe("30 rated games");
    expect(r?.b.detail).toBe("30 rated games");
    expect(JSON.stringify(r)).not.toMatch(/80% range|Worse|Better/);
    expect(r?.explain.measured).toContain("not called better or worse");
    expect(v.explain.measured).toContain("The rating's own change is shown and not called");
    // Accuracy and blunders keep their verdicts.
    expect(rest.find((x) => x.key === "accuracy")?.change.text).toBe("Worse");
    expect(rest.find((x) => x.key === "blunders")?.change.text).toBe("Worse");
    expect(v.since).toMatch(/^Since then: accuracy .* \(clearly worse\)/);
  });
});

describe("trajectoryView, with the rating's verdict stated", () => {
  it("states the peak, the current rating and the change from it", () => {
    const v = trajectoryView(falling(), "rapid", SHOWN);
    if (v.state !== "ready") throw new Error("expected a trajectory");
    expect(v.headline).toBe(
      "Your rapid rating peaked at 1,080 in February 2026 and is 816 now (−264).",
    );
    expect(v.kind).toBe("falling");
    expect(v.explain.measured).not.toContain("not called");
    expect(JSON.stringify(v)).not.toMatch(forbidden);
  });

  it("names each change by its verdict in the since-then sentence", () => {
    const v = trajectoryView(falling(), "rapid");
    if (v.state !== "ready") throw new Error("expected a trajectory");
    expect(v.since).toMatch(
      /^Since then: accuracy \d+(\.\d)?% then, \d+(\.\d)?% now \(clearly worse\)/,
    );
    expect(v.since).toContain("blunders per game");
    expect(v.since).toMatch(
      /missing a short tactic \d+% then, \d+% now of chances \((no clear change|clearly worse)\)/,
    );
    expect(v.since.endsWith(".")).toBe(true);
  });

  it("labels the two windows with their games and dates", () => {
    const v = trajectoryView(falling(), "rapid");
    if (v.state !== "ready") throw new Error("expected a trajectory");
    expect(v.compare.a.label).toBe("Around your peak");
    expect(v.compare.a.detail).toBe("30 rated games, 26 Jan to 24 Feb 2026");
    expect(v.compare.b.label).toBe("Your last 30 games");
    expect(v.compare.b.detail).toBe("30 rated games, 12 Mar to 10 Apr 2026");
    const keys = v.compare.rows.map((r) => r.key);
    expect(keys).toEqual(["rating", "accuracy", "blunders", "score", "weakness:W3.shallow"]);
    const rating = v.compare.rows[0];
    expect(rating?.change.text).toBe("Not called");
    const shown = trajectoryView(falling(), "rapid", SHOWN);
    if (shown.state !== "ready") throw new Error("expected a trajectory");
    expect(shown.compare.rows[0]?.change.text).toBe("Worse");
    const w = v.compare.rows.at(-1);
    expect(w?.a.detail).toMatch(/^80% range \d+% to \d+%, 90 of 300 chances in 30 games$/);
  });

  it("describes a rise from the first games", () => {
    const v = trajectoryView(rise(), "blitz", SHOWN);
    if (v.state !== "ready") throw new Error("expected a trajectory");
    expect(v.headline).toBe(
      "Your blitz rating is 1,120 now, up from an average of 858 over your first 30 rated games (+262).",
    );
    expect(v.compare.a.label).toBe("Your first 30 games");
  });

  it("calls a level rating no clear change", () => {
    const v = trajectoryView(level(), "rapid", SHOWN);
    if (v.state !== "ready") throw new Error("expected a trajectory");
    // A level line still has a highest game, so it is compared with the games around it.
    expect(v.headline).toBe(
      "Your rapid rating is 900 now, −12 from its peak of 912 in January 2026: no clear change since the games around it.",
    );
    expect(v.kind).toBe("flat");
  });

  it("says how many more rated games it needs", () => {
    const games = rated([900, 910, 905]);
    const t = computeTrajectory({ games, current: null, stats: [], weaknesses: [], seed: "v" });
    const v = trajectoryView(t, "rapid");
    expect(v).toEqual({
      state: "not_enough_games",
      text: "Your rating trajectory appears after 20 rated rapid games: 17 more to go.",
      games: 3,
    });
  });
});

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

/** Up to a peak of 1,080 at game 40, then down below where it started. */
const fall = Array.from({ length: 100 }, (_, i) =>
  i <= 40 ? 800 + i * 7 : 1080 - (i - 40) * 10 + (i % 2) * 3,
);

function falling() {
  const games = rated(fall);
  const points = new Map(
    games.map((g, i) => [g.id, { events: i >= 70 ? 4 : 3, opportunities: 10 }] as const),
  );
  return computeTrajectory({
    games,
    current: { rating: 500, at: new Date(START + 100 * DAY) },
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
/** The copy with the verdict withheld (ADR 0116), for an input whose rule did not clear. */
const HIDDEN = { ratingVerdict: false } as const;
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

describe("trajectoryView, with the rating's verdict stated (EXP-0030, ADR 0117)", () => {
  it("is stated by the manifest", () => {
    expect(RATING_VERDICT_SHOWN).toBe(true);
  });

  it("compares the windows' averages and states the peak as a plain fact", () => {
    const t = falling();
    if (t.kind === "not_enough_games") throw new Error("expected a trajectory");
    expect(t.kind).toBe("falling");
    const v = trajectoryView(t, "rapid");
    if (v.state !== "ready") throw new Error("expected a trajectory");
    expect(v.headline).toBe(
      "Your rapid rating averaged 637 over your last 30 games, down from 902 over your first 30\u00a0(−265).",
    );
    expect(v.facts).toBe("It is 500 now. Your highest was 1,080, in February 2026.");
    // No difference against the peak: it is a maximum of the rating's own wander.
    expect(v.headline).not.toMatch(/peaked at|from its peak/);
    expect(v.kind).toBe("falling");
    expect(v.explain.measured).not.toContain("not called");
    expect(JSON.stringify(v)).not.toMatch(forbidden);
  });

  it("describes a rise from the first games", () => {
    const v = trajectoryView(rise(), "blitz");
    if (v.state !== "ready") throw new Error("expected a trajectory");
    expect(v.headline).toBe(
      "Your blitz rating averaged 1,058 over your last 30 games, up from 858 over your first 30\u00a0(+200).",
    );
    // The current rating is the highest: no separate peak.
    expect(v.facts).toBe("It is 1,120 now.");
    expect(v.compare.a.label).toBe("Your first 30 games");
  });

  it("calls a level rating no clear change", () => {
    const v = trajectoryView(level(), "rapid");
    if (v.state !== "ready") throw new Error("expected a trajectory");
    expect(v.headline).toBe(
      "Your rapid rating averaged 900 over your last 30 games and 900 over your first 30\u00a0(±0): no clear change.",
    );
    expect(v.facts).toBe("It is 900 now. Your highest was 912, in January 2026.");
    expect(v.kind).toBe("flat");
  });

  it("gives the rating row its verdict and the range of the difference", () => {
    const v = trajectoryView(falling(), "rapid");
    if (v.state !== "ready") throw new Error("expected a trajectory");
    const [r, ...rest] = v.compare.rows;
    expect(r?.key).toBe("rating");
    expect(r?.change.text).toBe("Worse");
    expect(r?.change.detail).toMatch(/^Difference −\d+ points, 80% range −\d+ to −\d+$/);
    expect(r?.a.detail).toMatch(/^80% range [\d,]+ to [\d,]+, 30 rated games$/);
    expect(r?.explain.measured).toContain("carries most of its error over");
    expect(r?.explain.whyItMatters).toContain("It says the rating moved, not why.");
    expect(rest.find((x) => x.key === "accuracy")?.change.text).toBe("Worse");
    expect(rest.find((x) => x.key === "blunders")?.change.text).toBe("Worse");
  });

  it("names each change by its verdict in the since-then sentence", () => {
    const v = trajectoryView(falling(), "rapid");
    if (v.state !== "ready") throw new Error("expected a trajectory");
    expect(v.since).toMatch(
      /^Since your first games: accuracy \d+(\.\d)?% then, \d+(\.\d)?% now \(clearly worse\)/,
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
    expect(v.compare.a.label).toBe("Your first 30 games");
    expect(v.compare.a.detail).toBe("30 rated games, 1 Jan to 30 Jan 2026");
    expect(v.compare.b.label).toBe("Your last 30 games");
    expect(v.compare.b.detail).toBe("30 rated games, 12 Mar to 10 Apr 2026");
    const keys = v.compare.rows.map((r) => r.key);
    expect(keys).toEqual(["rating", "accuracy", "blunders", "score", "weakness:W3.shallow"]);
    const w = v.compare.rows.at(-1);
    expect(w?.a.detail).toMatch(/^80% range \d+% to \d+%, 90 of 300 chances in 30 games$/);
  });

  it("shows differences that add up from the numbers beside them", () => {
    // The first number in a shown text: "902 on average", "27%", "−265".
    const num = (s: string) =>
      Number((s.replace(/,/g, "").replace("−", "-").match(/-?\d+/) ?? ["NaN"])[0]);
    for (const t of [falling(), rise(), level()]) {
      const v = trajectoryView(t, "rapid");
      if (v.state !== "ready") throw new Error("expected a trajectory");
      const m = v.headline.match(
        /averaged ([\d,]+) .* ([\d,]+) over your first \d+\s\(([−+±][\d,]+)\)/,
      );
      if (!m) throw new Error(v.headline);
      const [, last, first, delta] = m as unknown as [string, string, string, string];
      expect(Number.isFinite(num(last))).toBe(true);
      expect(num(delta.replace("±", ""))).toBe(num(last) - num(first));
      const r = v.compare.rows[0];
      const d = r?.change.detail?.match(/^Difference ([−+±][\d,]+) points/)?.[1] ?? "";
      expect(num(d.replace("±", ""))).toBe(num(r?.b.value ?? "") - num(r?.a.value ?? ""));
      for (const w of v.compare.rows.filter((x) => x.key.startsWith("weakness:"))) {
        const p = w.change.detail?.match(/^Difference ([−+±]\d+) points of chances/)?.[1];
        if (p) expect(num(p.replace("±", ""))).toBe(num(w.b.value) - num(w.a.value));
      }
    }
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

describe("trajectoryView, with the rating's verdict withheld (ADR 0116)", () => {
  it("states the windows' averages with no direction", () => {
    const v = trajectoryView(falling(), "rapid", HIDDEN);
    if (v.state !== "ready") throw new Error("expected a trajectory");
    expect(v.headline).toBe(
      "Your rapid rating averaged 637 over your last 30 games and 902 over your first 30\u00a0(−265).",
    );
    expect(v.kind).toBeNull();
    for (const t of [rise(), level()]) {
      const w = trajectoryView(t, "rapid", HIDDEN);
      if (w.state !== "ready") throw new Error("expected a trajectory");
      expect(w.kind).toBeNull();
      expect(w.headline).not.toMatch(direction);
    }
    expect(JSON.stringify(v)).not.toMatch(forbidden);
  });

  it("shows the rating row's windows and difference with no range and no verdict", () => {
    const v = trajectoryView(falling(), "rapid", HIDDEN);
    if (v.state !== "ready") throw new Error("expected a trajectory");
    const [r, ...rest] = v.compare.rows;
    expect(r?.change).toEqual({
      verdict: "withheld",
      text: "Not called",
      detail: expect.any(String),
    });
    expect(r?.change.detail).toMatch(
      /^Difference −\d+ points\. A rating moves after every game, so this isn't judged on its own\.$/,
    );
    expect(r?.a.value).toMatch(/^\d+ on average$/);
    expect(r?.a.detail).toBe("30 rated games");
    expect(JSON.stringify(r)).not.toMatch(/80% range|Worse|Better/);
    expect(r?.explain.measured).toContain("not called better or worse");
    expect(v.explain.measured).toContain("The rating's own change is shown and not called");
    expect(rest.find((x) => x.key === "accuracy")?.change.text).toBe("Worse");
  });
});

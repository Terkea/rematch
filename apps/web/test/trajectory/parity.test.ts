import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import type { GameStatRow } from "../../src/server/stats/game-stats.js";
import {
  computeTrajectory,
  type RatedGame,
  type Trajectory,
  type TrajectoryInput,
} from "../../src/server/trajectory/model.js";

// One trajectory rule for the web and the coach (ADR 0088): this test writes the web's results
// for a few synthetic cases to `backend/fixtures/trajectory/cases.json`, and the Rust builder's
// test (`profile/tests/trajectory.rs`) must reproduce every number from the same inputs. The two
// share the seeds, the mulberry32 stream and the bootstrap, so the verdicts agree exactly.
// Regenerate with `TRAJECTORY_FIXTURE_WRITE=1 pnpm --filter @ci/web exec vitest run
// test/trajectory/parity.test.ts`.

const here = dirname(fileURLToPath(import.meta.url));
const FIXTURE = join(here, "../../../../backend/fixtures/trajectory/cases.json");
const DAY = 86_400_000;
const START = Date.UTC(2025, 2, 1, 18);

type CaseInput = {
  games: { id: string; playedAt: string; rating: number }[];
  current: { rating: number; at: string } | null;
  stats: {
    id: string;
    playedAt: string;
    result: GameStatRow["result"];
    analysed: boolean;
    accuracy: number | null;
    blunders: number | null;
  }[];
  weaknesses: {
    seriesKey: string;
    label: string;
    prior: { mu: number; n0: number };
    points: [string, number, number][];
  }[];
  seed: string;
};

/** A small deterministic generator, so a case reads the same on every run. */
function lcg(seed: number) {
  let s = seed >>> 0;
  return () => {
    s = (Math.imul(s, 1664525) + 1013904223) >>> 0;
    return s / 4294967296;
  };
}

function build(
  name: string,
  n: number,
  rating: (i: number) => number,
  opts: { accuracy?: (i: number) => number; weakness?: boolean; current?: number | null } = {},
): [string, CaseInput] {
  const rnd = lcg(n * 31 + name.length);
  const games = Array.from({ length: n }, (_, i) => ({
    id: `${name}-${String(i).padStart(3, "0")}`,
    playedAt: new Date(START + i * DAY + Math.floor(rnd() * 3600) * 1000).toISOString(),
    rating: Math.round(rating(i) + (rnd() - 0.5) * 30),
  }));
  const results: GameStatRow["result"][] = ["win", "loss", "draw", "win", "loss", "unfinished"];
  const stats = games.map((g, i) => {
    const analysed = i % 9 !== 4;
    return {
      id: g.id,
      playedAt: g.playedAt,
      result: results[Math.floor(rnd() * results.length)] as GameStatRow["result"],
      analysed,
      accuracy: analysed
        ? Math.round(((opts.accuracy?.(i) ?? 72) + (rnd() - 0.5) * 16) * 10) / 10
        : null,
      blunders: analysed ? Math.floor(rnd() * 4) : null,
    };
  });
  const weaknesses = opts.weakness
    ? [
        {
          seriesKey: "W3.shallow",
          label: "Missing a short tactic",
          prior: { mu: 0.25, n0: 60 },
          points: games
            .filter((_, i) => i % 3 !== 1)
            .map((g, i): [string, number, number] => {
              const opp = 1 + Math.floor(rnd() * 4);
              const late = i > (n * 2) / 3 ? 0.25 : 0;
              return [g.id, Math.min(opp, Math.floor(rnd() * opp * (0.6 + late))), opp];
            }),
        },
        {
          seriesKey: "W5",
          label: "Few chances",
          prior: { mu: 0.4, n0: 30 },
          points: games.slice(-3).map((g): [string, number, number] => [g.id, 1, 2]),
        },
      ]
    : [];
  const last = games.at(-1);
  const current =
    opts.current === null || !last
      ? null
      : { rating: opts.current ?? last.rating, at: new Date(Date.parse(last.playedAt) + 60_000).toISOString() };
  return [name, { games, current, stats, weaknesses, seed: `trajectory:fixture:${name}` }];
}

const CASES: [string, CaseInput][] = [
  build("falling", 140, (i) => (i < 60 ? 850 + i * 4 : 1090 - (i - 60) * 3.4), {
    accuracy: (i) => (i < 100 ? 79 : 66),
    weakness: true,
    current: 816,
  }),
  build("rising", 90, (i) => 760 + i * 3.5, { weakness: true }),
  build("flat", 75, () => 1210, { current: null }),
  build("short", 34, (i) => 1500 - i * 2),
  build("few", 12, () => 900),
];

function toInput(c: CaseInput): TrajectoryInput {
  const games: RatedGame[] = c.games.map((g) => ({ ...g, playedAt: new Date(g.playedAt) }));
  const stats: GameStatRow[] = c.stats.map((s) => ({
    ...s,
    playedAt: new Date(s.playedAt),
    color: "white",
    mistakes: null,
    inaccuracies: null,
    phaseAccuracy: { opening: null, middlegame: null, endgame: null },
  }));
  return {
    games,
    current: c.current ? { rating: c.current.rating, at: new Date(c.current.at) } : null,
    stats,
    weaknesses: c.weaknesses.map((w) => ({
      seriesKey: w.seriesKey,
      label: w.label,
      prior: w.prior,
      points: new Map(w.points.map(([id, events, opportunities]) => [id, { events, opportunities }])),
    })),
    seed: c.seed,
  };
}

/** The numbers both sides must agree on. */
function project(t: Trajectory): unknown {
  if (t.kind === "not_enough_games") return { kind: t.kind, games: t.games, needed: t.needed };
  const est = (e: { kind: string } & Record<string, unknown>) =>
    e.kind === "estimate"
      ? {
          value: e.value,
          low: (e.interval as { low: number } | null)?.low ?? null,
          high: (e.interval as { high: number } | null)?.high ?? null,
          n: e.n ?? null,
        }
      : { value: null, low: null, high: null, n: e.n ?? null };
  const win = (w: typeof t.then) => ({
    basis: w.basis,
    games: w.games,
    firstId: w.firstId,
    lastId: w.lastId,
    meanRating: w.meanRating,
  });
  return {
    kind: t.kind,
    games: t.games,
    peak: { rating: t.peak.rating, gameId: t.peak.gameId },
    current: t.current.rating,
    change: t.change,
    then: win(t.then),
    now: win(t.now),
    rating: {
      verdict: t.rating.verdict,
      a: est(t.ratingSides.a as never),
      b: est(t.ratingSides.b as never),
      diff: t.rating.verdict === "not_enough_games" ? null : t.rating.diff,
    },
    rows: t.rows.map((r) =>
      r.unit === "rate"
        ? {
            key: r.key,
            verdict: r.result.verdict,
            counts: r.counts,
            a: r.a.kind === "estimate" ? { value: r.a.value, ...r.a.interval } : null,
            b: r.b.kind === "estimate" ? { value: r.b.value, ...r.b.interval } : null,
            pBetter: r.result.verdict === "not_enough_games" ? null : r.result.pBetter,
            pWorse: r.result.verdict === "not_enough_games" ? null : r.result.pWorse,
          }
        : {
            key: r.key,
            verdict: r.result.verdict,
            a: est(r.a as never),
            b: est(r.b as never),
            diff: r.result.verdict === "not_enough_games" ? null : r.result.diff,
          },
    ),
  };
}

describe("trajectory parity fixture", () => {
  const cases = CASES.map(([name, input]) => ({
    name,
    input,
    expected: project(computeTrajectory(toInput(input))),
  }));

  it("covers a fall, a rise, a level line, a short history and too few games", () => {
    const kinds = cases.map((c) => (c.expected as { kind: string }).kind);
    expect(kinds).toEqual(["falling", "rising", "flat", expect.any(String), "not_enough_games"]);
  });

  it("matches the committed fixture the Rust builder reads", () => {
    const text = `${JSON.stringify({ cases }, null, 2)}\n`;
    if (process.env.TRAJECTORY_FIXTURE_WRITE === "1") writeFileSync(FIXTURE, text);
    expect(readFileSync(FIXTURE, "utf8")).toBe(text);
  });
});

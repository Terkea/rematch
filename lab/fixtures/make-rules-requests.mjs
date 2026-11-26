#!/usr/bin/env node
// Writes the requests of the lab-v2 steps' contract tests (#125, ADR 0115): run it only to change
// a fixture, then regenerate the golden files (lab/README.md). The numbers are chosen, not
// measured, so each case shows one branch of the rule:
// - trajectory: `lab-peaked` climbs to a peak and falls well below it (Lichess, analysed games, a
//   top weakness); `lab-steady` wobbles around one rating (chess.com, so pre-game ratings shift
//   by one game); `lab-new` has too few rated games for a verdict.
// - timeline: `lab-improves` hangs pieces three times the peers' rate for 60 games, then at half
//   of it, so the detector confirms a change; its W9b stays at the peers' rate; an opening line
//   has no band statistic, so it is never an issue.
// - focus-target, focus-progress: one case per branch of `chooseTarget` and `focusProgress`.
// - trainer-sources: one candidate per branch of `keepCandidate`, and `primarySource`'s order.
import { writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const DIR = dirname(fileURLToPath(import.meta.url));
const START = Date.parse("2026-03-01T18:00:00Z");
const HOUR = 3_600_000;
const iso = (t) => new Date(t).toISOString().replace(".000Z", "Z");
const write = (name, doc) => {
  writeFileSync(join(DIR, name), `${JSON.stringify(doc, null, 2)}\n`);
  console.log(`wrote ${name}`);
};

// ---- trajectory -------------------------------------------------------------------------------

/** A deterministic wobble in [-1, 1]. */
const wobble = (i) => Math.sin(i * 1.7) * 0.6 + Math.sin(i * 0.37) * 0.4;
const results = ["win", "loss", "draw", "win", "loss"];

function games(n, rating, { analysed = true } = {}) {
  return Array.from({ length: n }, (_, i) => ({
    id: `g${String(i + 1).padStart(3, "0")}`,
    played_at: iso(START + i * 20 * HOUR),
    rating: rating(i),
    result: results[i % results.length],
    analysed: analysed && i % 7 !== 3,
    accuracy: analysed && i % 7 !== 3 ? Math.round((78 + 6 * wobble(i + 3)) * 10) / 10 : null,
    blunders: (i * 3) % 4 === 0 ? 1 : 0,
  }));
}

const peaked = games(80, (i) =>
  Math.round((i < 30 ? 1500 + i * 4 : 1620 - (i - 30) * 3) + 12 * wobble(i)),
);
const trajectory = {
  schema_version: "lab-v1",
  players: [
    {
      id: "lab-peaked",
      seed: "trajectory:lab-peaked:rapid",
      chesscom: false,
      games: peaked,
      weaknesses: [
        {
          series_key: "W1",
          prior: { mu: 0.004, n0: 400 },
          points: peaked.map((g, i) => ({
            game_id: g.id,
            events: i >= 40 && i % 3 === 0 ? 1 : i % 9 === 0 ? 1 : 0,
            opportunities: 40,
          })),
        },
      ],
    },
    {
      id: "lab-steady",
      seed: "trajectory:lab-steady:rapid",
      chesscom: true,
      games: games(60, (i) => Math.round(1400 + 15 * wobble(i))),
      weaknesses: [],
    },
    {
      id: "lab-new",
      seed: "trajectory:lab-new:rapid",
      games: games(12, (i) => 1200 + i, { analysed: false }),
    },
  ],
};
write("trajectory-request.json", trajectory);

// ---- timeline ---------------------------------------------------------------------------------

const uuid = (i) => `00000000-0000-4000-8000-${(i + 1).toString(16).padStart(12, "0")}`;

function stat(metric, median) {
  return {
    metric,
    median,
    p25: median * 0.6,
    p40: median * 0.85,
    p75: median * 1.5,
    pooledRate: median,
    eplPerEvent: 0.35,
    oppsPerGame: 50,
    oppSlopePer100: null,
    nPlayers: 40,
    nGames: 1200,
    lowSample: false,
  };
}

const point = (i, events, opportunities) => ({
  game_id: uuid(i),
  played_at: iso(START + i * 20 * HOUR),
  events,
  opportunities,
});

const timeline = {
  schema_version: "lab-v1",
  players: [
    {
      id: "lab-improves",
      reference: {
        "weakness:W1": stat("weakness:W1", 0.01),
        "weakness:W9b": stat("weakness:W9b", 0.05),
      },
      series: {
        // 0.03 a chance for 60 games (1.5 of 50 a game), then 0.005 (one in four games).
        W1: Array.from({ length: 120 }, (_, i) =>
          point(i, i < 60 ? (i % 2 === 0 ? 2 : 1) : i % 4 === 0 ? 1 : 0, 50),
        ),
        W9b: Array.from({ length: 80 }, (_, i) => point(i, i % 5 === 0 ? 1 : 0, 4)),
        "opening:white:italian-game": Array.from({ length: 20 }, (_, i) =>
          point(i, i % 2, 1),
        ),
      },
    },
    {
      id: "lab-no-benchmark",
      reference: null,
      series: {
        W1: Array.from({ length: 30 }, (_, i) => point(i, 3, 50)),
      },
    },
  ],
};
write("timeline-request.json", timeline);

// ---- focus ------------------------------------------------------------------------------------

write("focus-target-request.json", {
  schema_version: "lab-v1",
  items: [
    { id: "relative-beats-a-low-peer", baseline: 0.2, peer: 0.1 },
    { id: "peer-is-easier", baseline: 0.2, peer: 0.17 },
    { id: "peer-too-close", baseline: 0.2, peer: 0.19 },
    { id: "no-peer", baseline: 0.04, peer: null },
    { id: "rounds-to-a-tenth", baseline: 0.1234, peer: null },
    { id: "already-zero", baseline: 0, peer: 0.01 },
  ],
});

write("focus-progress-request.json", {
  schema_version: "lab-v1",
  items: [
    { id: "too-early", k: 1, n: 6, games: 7, games_n: 20, target: 0.15 },
    { id: "on-track", k: 1, n: 13, games: 14, games_n: 20, target: 0.15 },
    { id: "met", k: 2, n: 19, games: 20, games_n: 20, target: 0.15 },
    { id: "not-met", k: 5, n: 12, games: 20, games_n: 20, target: 0.15 },
    { id: "inconclusive", k: 0, n: 4, games: 20, games_n: 20, target: 0.15 },
    { id: "no-chance-yet", k: 0, n: 0, games: 0, games_n: 20, target: 0.15 },
  ],
});

// ---- trainer ----------------------------------------------------------------------------------

// White's knight on f3 can take a queen on h4: a free capture.
const QUEEN_HANGS = "rnb1kbnr/pppp1ppp/8/4p3/4P2q/5N2/PPPP1PPP/RNBQKB1R w KQkq - 2 3";
// After 1.e4 d5: exd5 can be taken back, a pawn for a pawn.
const SCANDI = "rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 2";
const candidate = (id, o) => ({
  game_id: `game-${id}`,
  ply: 5,
  fen: SCANDI,
  uci: "b1c3",
  best_uci: "e4d5",
  win_before: 55,
  win_after: 40,
  prev_san: "d5",
  sources: ["W3.shallow"],
  ...o,
});

write("trainer-sources-request.json", {
  schema_version: "lab-v1",
  candidates: [
    candidate("kept", { sources: ["blunder", "W3.shallow", "W7.traps"] }),
    candidate("blunder-only", { sources: ["blunder"] }),
    candidate("no-source", { sources: [] }),
    candidate("played-the-best", { uci: "e4d5" }),
    candidate("no-best-move", { best_uci: null }),
    candidate("already-decided", { win_before: 98, win_after: 93 }),
    candidate("free-capture", { fen: QUEEN_HANGS, best_uci: "f3h4", prev_san: "Qh4" }),
    candidate("free-capture-missed", {
      fen: QUEEN_HANGS,
      best_uci: "f3h4",
      prev_san: "Qh4",
      sources: ["W4.missed"],
    }),
    ...Array.from({ length: 8 }, (_, i) =>
      candidate(`holdout-${i}`, { game_id: `game-holdout`, ply: 10 + i }),
    ),
  ],
});

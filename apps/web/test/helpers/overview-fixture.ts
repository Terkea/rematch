import type { ClassOverview } from "../../src/app/lab/overview/load.js";
import type { GameRow, TimeClass } from "../../src/server/games/types.js";
import type { Overview } from "../../src/server/overview/types.js";
import type { Estimate } from "../../src/server/stats/index.js";

// A synthetic overview for the lab tests (B15: tests only, never the owner's data).

export const est = (value: number, n: number, low = value * 0.9, high = value * 1.1): Estimate => ({
  kind: "estimate",
  value,
  interval: { low, high, level: 0.8 },
  n,
});
export const short = (n: number): Estimate => ({ kind: "insufficient", n, needed: 10 - n });
const DAY = 86_400_000;

/** A synthetic overview (tests only, B15): 30 rapid games, 3 blitz, one estimate with no spread. */
export function overview(timeClass: TimeClass | null = null): Overview {
  const n = timeClass === "blitz" ? 3 : timeClass === "rapid" ? 30 : 33;
  const points = Array.from({ length: n }, (_, i) => ({
    id: `g${i}`,
    playedAt: new Date(Date.UTC(2026, 9, 1) + i * DAY),
    accuracy: 50 + (i % 7) * 5,
    result: i % 3 === 0 ? ("win" as const) : i % 3 === 1 ? ("loss" as const) : ("draw" as const),
  }));
  const big = n >= 10;
  return {
    accounts: [
      {
        id: "acc",
        platform: "chesscom",
        username: "someplayer",
        displayName: "SomePlayer",
        verified: false,
        lastSyncedAt: new Date(Date.UTC(2026, 10, 21)),
        lastImport: null,
      },
    ],
    status: { total: 35, analysed: 33, waiting: 2, failed: 0, skipped: 0 },
    timeClasses: [
      { timeClass: "rapid", analysed: 30 },
      { timeClass: "blitz", analysed: 3 },
    ],
    accuracy: {
      points,
      rolling: points.slice(9).map((p) => ({ id: p.id, mean: 64, low: 60, high: 68 })),
      overall: big ? est(64.2, n) : short(n),
    },
    judgements: {
      columns: points.map((p, i) => ({
        id: p.id,
        playedAt: p.playedAt,
        blunders: i % 3,
        mistakes: i % 2,
        inaccuracies: 1,
      })),
      blunders: big ? est(1.2, n) : short(n),
      mistakes: big ? { kind: "estimate", value: 0.5, interval: null, n } : short(n),
      inaccuracies: big ? est(1, n) : short(n),
      weekly: [
        { week: "2026-W40", blunders: 1, mistakes: 0.5, inaccuracies: 1, accuracy: 63, games: 7 },
        {
          week: "2026-W41",
          blunders: null,
          mistakes: null,
          inaccuracies: null,
          accuracy: null,
          games: 0,
        },
        { week: "2026-W42", blunders: 1.4, mistakes: 0.5, inaccuracies: 1, accuracy: 66, games: 7 },
      ],
    },
    byTimeClass: [
      {
        timeClass: "rapid",
        games: 30,
        wins: 10,
        draws: 10,
        losses: 10,
        score: est(0.5, 30),
        accuracy: est(64, 30),
      },
      {
        timeClass: "blitz",
        games: 3,
        wins: 1,
        draws: 1,
        losses: 1,
        score: short(3),
        accuracy: short(3),
      },
    ],
    recent: timeClass === null ? [recentGame] : [],
  };
}

export const classes = (): ClassOverview[] => [
  { timeClass: "rapid", overview: overview("rapid") },
  { timeClass: "blitz", overview: overview("blitz") },
];

export const empty = (): Overview => ({
  ...overview(),
  accounts: [],
  status: { total: 0, analysed: 0, waiting: 0, failed: 0, skipped: 0 },
});

/** A recent game whose opponent b and c never show, so it must never cross to the client. */
export const recentGame: GameRow = {
  id: "r1",
  accountId: "acc",
  platform: "chesscom",
  playedAt: new Date(Date.UTC(2026, 10, 1)),
  timeClass: "rapid",
  baseSeconds: 600,
  incrementSeconds: 0,
  rated: true,
  playerColor: "white",
  player: { name: "someplayer", rating: 1200 },
  opponent: { name: "SecretOpponent", rating: 1234 },
  result: "win",
  termination: "resignation",
  eco: "C20",
  openingName: "King's Pawn Game",
  analysisStatus: "done",
  accuracy: 71.5,
  counts: { blunders: 1, mistakes: 0, inaccuracies: 2 },
  plies: 60,
};

import type { Db } from "@ci/db";
import type { StatsFilters } from "../filters/params";
import { comparePeriods, type Period } from "../filters/presets";
import { compareMeans, type MeanComparison, type Pair } from "./compare";
import type { Estimate } from "./estimate";
import { type GameStatRow, gameStatRows, scopeWithPeriod, summarise } from "./game-stats";

// Period A (earlier) against period B (later) for the game-level stats the overview and progress
// pages show: accuracy, blunders per game and score. Each side's estimate comes from `summarise`,
// and each change from `compareMeans` (ADR 0083 §6): nothing is called better or worse unless the
// bootstrap 80% interval of the difference excludes 0, with at least `KPI_MIN_GAMES` per side.
// Server code only: it imports the game-stats module, which reads the database.

export type CompareKey = "accuracy" | "blunders" | "score";

export type CompareRow = {
  key: CompareKey;
  /** Accuracy 0-100, blunders per game, score 0-1 (points per finished game). */
  a: Estimate;
  b: Estimate;
  result: MeanComparison;
};

export type PeriodComparison = {
  /** Games in scope per side. */
  n: Pair<number>;
  rows: CompareRow[];
};

const finite = (xs: (number | null)[]): number[] =>
  xs.filter((x): x is number => x !== null && Number.isFinite(x));

const POINTS = { win: 1, draw: 0.5, loss: 0 } as const;
const points = (rs: readonly GameStatRow[]) =>
  rs.flatMap((r) => (r.result === "unfinished" ? [] : [POINTS[r.result]]));
const analysed = (rs: readonly GameStatRow[], key: "accuracy" | "blunders") =>
  finite(rs.filter((r) => r.analysed).map((r) => r[key]));

/** The three stats of A against B (pure; `seed` fixes every bootstrap). */
export function comparePeriodRows(
  a: readonly GameStatRow[],
  b: readonly GameStatRow[],
  seed: string,
): PeriodComparison {
  const sa = summarise(a, `${seed}:a`);
  const sb = summarise(b, `${seed}:b`);
  return {
    n: { a: a.length, b: b.length },
    rows: [
      {
        key: "accuracy",
        a: sa.accuracy,
        b: sb.accuracy,
        result: compareMeans(analysed(a, "accuracy"), analysed(b, "accuracy"), {
          higherIsBetter: true,
          seed: `${seed}:accuracy`,
        }),
      },
      {
        key: "blunders",
        a: sa.perGame.blunders,
        b: sb.perGame.blunders,
        result: compareMeans(analysed(a, "blunders"), analysed(b, "blunders"), {
          higherIsBetter: false,
          seed: `${seed}:blunders`,
        }),
      },
      {
        key: "score",
        a: sa.score.score,
        b: sb.score.score,
        result: compareMeans(points(a), points(b), { higherIsBetter: true, seed: `${seed}:score` }),
      },
    ],
  };
}

/** The verdict in words: "Better", "Worse", "No clear change" or what is still needed. */
export function verdictText(c: MeanComparison): string {
  switch (c.verdict) {
    case "better":
      return "Better";
    case "worse":
      return "Worse";
    case "no_clear_change":
      return "No clear change";
    case "not_enough_games": {
      const { a, b } = c.needed;
      const head = "Not enough games to tell";
      if (a > 0 && b > 0) return `${head}: need ${a} more in period A and ${b} more in period B`;
      if (a > 0) return `${head}: need ${a} more in period A`;
      if (b > 0) return `${head}: need ${b} more in period B`;
      return head;
    }
  }
}

/** The two sides' periods and their comparison, read for `filters.cmp` (null without one). */
export async function loadPeriodComparison(
  db: Db,
  userId: string,
  filters: StatsFilters,
  now: Date,
  opts: { accountId?: string | null } = {},
): Promise<{
  periods: Pair<Period>;
  spans: Pair<DaySpan | null>;
  comparison: PeriodComparison;
} | null> {
  const periods = comparePeriods(filters, now);
  if (!periods) return null;
  const [a, b] = await Promise.all([
    gameStatRows(db, userId, scopeWithPeriod(filters, periods.a), opts),
    gameStatRows(db, userId, scopeWithPeriod(filters, periods.b), opts),
  ]);
  const seed = `period-compare:${JSON.stringify({ ...filters, periods })}`;
  return {
    periods,
    spans: { a: spanOf(a), b: spanOf(b) },
    comparison: comparePeriodRows(a, b, seed),
  };
}

/** The first and last UTC day of some games, newest-first or not. */
export type DaySpan = { from: string; to: string };

export function spanOf(rows: readonly GameStatRow[]): DaySpan | null {
  if (rows.length === 0) return null;
  const days = rows.map((r) => r.playedAt.toISOString().slice(0, 10)).sort();
  return { from: days[0] as string, to: days.at(-1) as string };
}

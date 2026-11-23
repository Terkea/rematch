import {
  type BetaPosterior,
  CONFIDENCE,
  compareMeans,
  compareRates,
  type Estimate,
  KPI_MIN_GAMES,
  type MeanComparison,
  type Pair,
  type Prior,
  posterior,
  type RateComparison,
} from "../stats";
import type { GameStatRow } from "../stats/game-stats";
import { comparePeriodRows } from "../stats/period-compare";

// The rating trajectory (F6, F7; ADR 0088): the player's rating over time in one time class, its
// peak and the current rating, and a "then" window against the last games. Pure: the loader
// (`load.ts`) reads the games, and the coach's v3 block is the same rule in Rust
// (`profile::coaching::trajectory`). Every difference is called only by the ADR 0083 rule
// (`compareMeans`, `compareRates`), so "worse" means the 80% evidence says so.

/** Games per window: "the 30 games around the peak" against "your last 30 games". */
export const TRAJECTORY_WINDOW = 30;
/** Rated games before anything is said: two windows of `KPI_MIN_GAMES`. */
export const TRAJECTORY_MIN_GAMES = 2 * KPI_MIN_GAMES;

/** One rated game, with the player's pre-game rating (ADR-FIX-C-1). */
export type RatedGame = { id: string; playedAt: Date; rating: number };

/** A top weakness: its catalogue prior and its per-game series points. */
export type TrajectoryWeaknessInput = {
  seriesKey: string;
  label: string;
  prior: Prior;
  points: ReadonlyMap<string, { events: number; opportunities: number }>;
};

export type TrajectoryInput = {
  /** Rated games of the time class, in any order. */
  games: readonly RatedGame[];
  /** The latest stored rating (after the newest game); null to use the newest game's. */
  current: { rating: number; at: Date } | null;
  /** Game-level stats of (at least) the windows' games. */
  stats: readonly GameStatRow[];
  /** The top weaknesses to compare, in their priority order. */
  weaknesses: readonly TrajectoryWeaknessInput[];
  seed: string;
};

export type RatingPoint = { id: string; at: Date; rating: number };

export type TrajectoryWindow = {
  /** `peak`: the games around the peak; `start`: the first games; `last`: the newest games. */
  basis: "peak" | "start" | "last";
  games: number;
  from: Date;
  to: Date;
  firstId: string;
  lastId: string;
  /** Mean pre-game rating over the window. */
  meanRating: number;
};

/** A rate's side: its counts and posterior, or insufficient below `KPI_MIN_GAMES` games. */
export type RateEstimate =
  | { kind: "estimate"; value: number; interval: { low: number; high: number; level: number } }
  | { kind: "insufficient"; needed: number };

export type ThenNowRow =
  | {
      key: "accuracy" | "blunders" | "score";
      label: string;
      unit: "accuracy" | "perGame" | "score";
      a: Estimate;
      b: Estimate;
      result: MeanComparison;
    }
  | {
      key: `weakness:${string}`;
      seriesKey: string;
      label: string;
      unit: "rate";
      a: RateEstimate;
      b: RateEstimate;
      counts: Pair<{ events: number; opportunities: number; games: number }>;
      result: RateComparison;
    };

export type TrajectoryKind = "falling" | "rising" | "flat";

export type Trajectory =
  | { kind: "not_enough_games"; games: number; needed: number; series: RatingPoint[] }
  | {
      kind: TrajectoryKind;
      games: number;
      series: RatingPoint[];
      /** The highest rating; `gameId` is null when the peak is the current rating. */
      peak: { rating: number; at: Date; gameId: string | null };
      current: { rating: number; at: Date };
      /** Current minus the reference: the peak (basis `peak`) or the first window's mean. */
      change: number;
      then: TrajectoryWindow;
      now: TrajectoryWindow;
      /** The windows' pre-game ratings compared: the trajectory's kind. */
      rating: MeanComparison;
      rows: ThenNowRow[];
    };

const LABEL = { accuracy: "Accuracy", blunders: "Blunders per game", score: "Score" } as const;
const UNIT = { accuracy: "accuracy", blunders: "perGame", score: "score" } as const;

function windowOf(
  series: readonly RatingPoint[],
  start: number,
  size: number,
  basis: TrajectoryWindow["basis"],
): TrajectoryWindow {
  const slice = series.slice(start, start + size);
  const first = slice[0] as RatingPoint;
  const last = slice.at(-1) as RatingPoint;
  return {
    basis,
    games: slice.length,
    from: first.at,
    to: last.at,
    firstId: first.id,
    lastId: last.id,
    meanRating: slice.reduce((s, p) => s + p.rating, 0) / slice.length,
  };
}

function rateEstimate(p: BetaPosterior | null, games: number): RateEstimate {
  if (!p || games < KPI_MIN_GAMES) {
    return { kind: "insufficient", needed: Math.max(0, KPI_MIN_GAMES - games) };
  }
  return {
    kind: "estimate",
    value: p.mean,
    interval: { low: p.interval.low, high: p.interval.high, level: CONFIDENCE },
  };
}

function weaknessRow(
  w: TrajectoryWeaknessInput,
  ids: Pair<readonly string[]>,
  seed: number,
): ThenNowRow {
  const side = (gameIds: readonly string[]) => {
    let events = 0;
    let opportunities = 0;
    let games = 0;
    for (const id of gameIds) {
      const p = w.points.get(id);
      if (!p || !(p.opportunities > 0)) continue;
      events += p.events;
      opportunities += p.opportunities;
      games += 1;
    }
    return { events, opportunities, games };
  };
  const a = side(ids.a);
  const b = side(ids.b);
  const result = compareRates(
    { k: a.events, n: a.opportunities, games: a.games },
    { k: b.events, n: b.opportunities, games: b.games },
    { prior: w.prior, higherIsBetter: false, seed },
  );
  const post = (s: typeof a) =>
    s.games >= KPI_MIN_GAMES ? posterior(s.events, s.opportunities, w.prior) : null;
  return {
    key: `weakness:${w.seriesKey}`,
    seriesKey: w.seriesKey,
    label: w.label,
    unit: "rate",
    a: rateEstimate(post(a), a.games),
    b: rateEstimate(post(b), b.games),
    counts: { a, b },
    result,
  };
}

/** The trajectory of some rated games (module docs). */
export function computeTrajectory(input: TrajectoryInput): Trajectory {
  const series: RatingPoint[] = input.games
    .filter((g) => Number.isFinite(g.rating) && Number.isFinite(g.playedAt.getTime()))
    .map((g) => ({ id: g.id, at: g.playedAt, rating: g.rating }))
    .sort((a, b) => a.at.getTime() - b.at.getTime() || (a.id < b.id ? -1 : a.id > b.id ? 1 : 0));
  const n = series.length;
  if (n < TRAJECTORY_MIN_GAMES) {
    return { kind: "not_enough_games", games: n, needed: TRAJECTORY_MIN_GAMES - n, series };
  }
  const newest = series[n - 1] as RatingPoint;
  const current =
    input.current && Number.isFinite(input.current.rating)
      ? input.current
      : { rating: newest.rating, at: newest.at };

  // The peak: the first time the highest rating was reached; the current rating counts too.
  let peakIdx = 0;
  for (let i = 1; i < n; i++) {
    if ((series[i] as RatingPoint).rating > (series[peakIdx] as RatingPoint).rating) peakIdx = i;
  }
  const peakGame = series[peakIdx] as RatingPoint;
  const peak =
    current.rating > peakGame.rating
      ? { rating: current.rating, at: current.at, gameId: null }
      : { rating: peakGame.rating, at: peakGame.at, gameId: peakGame.id };

  const size = Math.min(TRAJECTORY_WINDOW, Math.floor(n / 2));
  const nowStart = n - size;
  const peakEarlier = peak.gameId !== null && peakIdx < nowStart;
  const thenStart = peakEarlier
    ? Math.max(0, Math.min(peakIdx - Math.floor(size / 2), nowStart - size))
    : 0;
  const then = windowOf(series, thenStart, size, peakEarlier ? "peak" : "start");
  const now = windowOf(series, nowStart, size, "last");

  const ids = {
    a: series.slice(thenStart, thenStart + size).map((p) => p.id),
    b: series.slice(nowStart).map((p) => p.id),
  };
  const rating = compareMeans(
    series.slice(thenStart, thenStart + size).map((p) => p.rating),
    series.slice(nowStart).map((p) => p.rating),
    { higherIsBetter: true, seed: `${input.seed}:rating` },
  );
  const kind: TrajectoryKind =
    rating.verdict === "worse" ? "falling" : rating.verdict === "better" ? "rising" : "flat";

  const byId = new Map(input.stats.map((r) => [r.id, r]));
  const rowsOf = (list: readonly string[]) =>
    list.map((id) => byId.get(id)).filter((r): r is GameStatRow => r !== undefined);
  const compared = comparePeriodRows(rowsOf(ids.a), rowsOf(ids.b), `${input.seed}:then-now`);
  const rows: ThenNowRow[] = compared.rows.map((r) => ({
    key: r.key,
    label: LABEL[r.key],
    unit: UNIT[r.key],
    a: r.a,
    b: r.b,
    result: r.result,
  }));
  input.weaknesses.forEach((w, i) => {
    rows.push(weaknessRow(w, ids, i + 1));
  });

  return {
    kind,
    games: n,
    series,
    peak,
    current,
    change: Math.round(current.rating - (then.basis === "peak" ? peak.rating : then.meanRating)),
    then,
    now,
    rating,
    rows,
  };
}

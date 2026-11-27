import {
  type BetaPosterior,
  CONFIDENCE,
  compareRates,
  type Estimate,
  KPI_MIN_GAMES,
  type MeanComparison,
  type Pair,
  type Prior,
  posterior,
  type RateComparison,
  Z,
} from "../stats";
import { finishInterval } from "../stats/bootstrap";
import type { GameStatRow } from "../stats/game-stats";
import { comparePeriodRows } from "../stats/period-compare";

// The rating trajectory (F6, F7; ADR 0089): the player's rating over time in one time class, its
// peak and the current rating, and the first games against the last games. Pure: the loader
// (`load.ts`) reads the games, and the coach's v3 block is the same rule in Rust
// (`profile::coaching::trajectory`, `RatingRule::MemoryHalves`). The rating row's 80% interval
// counts the rating's memory from game to game (EXP-0030, ADR 0117): a rating carries most of its
// error over to the next game, so 30 consecutive ratings are not 30 independent measurements, and
// the interval comes from the player's own rating steps, not a bootstrap of the window (which
// EXP-0015 found six to thirteen times too narrow). The other rows are called by the ADR 0083
// rules (`compareMeans`, `compareRates`), so "worse" means the 80% evidence says so.

/** Games per window: "your first 30 games" against "your last 30 games". */
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
      /** The difference the verdict tests: the last window's mean minus the first window's. */
      change: number;
      then: TrajectoryWindow;
      now: TrajectoryWindow;
      /** The windows' pre-game ratings compared: the trajectory's kind. */
      rating: MeanComparison;
      /** Each window's mean pre-game rating with its 80% interval. */
      ratingSides: Pair<Estimate>;
      rows: ThenNowRow[];
    };

const RATING_DOMAIN = { min: 0, max: 4000 };

/** The scale of every chess rating: 400 points is a factor of 10 in the odds (Rust `RATING_SCALE`). */
export const RATING_SCALE = Math.LN10 / 400;

/**
 * A rating as a mean-reverting series around the player's strength (EXP-0030, Rust `Memory`): the
 * error after each game is `phi` times the one before plus fresh noise, with stationary variance
 * `sigma2`. Both come from the root mean square of the player's own steps `s`: `a = q s / 2`
 * (at most 1), `phi = 1 - a`, `sigma2 = s^2 / (a (2 - a))`.
 */
export type Memory = { sigma2: number; phi: number };

/** The memory of a series of pre-game ratings; null with fewer than two or no movement. */
export function memoryOf(ratings: readonly number[]): Memory | null {
  if (ratings.length < 2) return null;
  let sq = 0;
  for (let i = 1; i < ratings.length; i++) {
    const d = (ratings[i] as number) - (ratings[i - 1] as number);
    sq += d * d;
  }
  const rms = Math.sqrt(sq / (ratings.length - 1));
  if (!(Number.isFinite(rms) && rms > 0)) return null;
  const a = Math.min((RATING_SCALE * rms) / 2, 1);
  return { sigma2: (rms * rms) / (a * (2 - a)), phi: Math.min(Math.max(1 - a, 0), 1 - 1e-12) };
}

/** The sum of `phi^|i - j|` over a block of `m` consecutive games. */
function blockSum(mem: Memory, m: number): number {
  const p = mem.phi;
  if (p <= 0) return m;
  return (m * (1 + p)) / (1 - p) - (2 * p * (1 - p ** m)) / ((1 - p) * (1 - p));
}

/** The variance of a block mean of `m` games around the strength. */
function meanVar(mem: Memory, m: number): number {
  return (mem.sigma2 * blockSum(mem, m)) / (m * m);
}

/** The variance of `mean(b) - mean(a)` for blocks `[a0, a1)` and `[b0, b1)`, `b0 >= a1`. */
export function diffVar(mem: Memory, a: [number, number], b: [number, number]): number {
  const ma = a[1] - a[0];
  const mb = b[1] - b[0];
  const p = mem.phi;
  const cross =
    p <= 0 ? 0 : (p ** (b[0] + 1 - a[1]) * (1 - p ** ma) * (1 - p ** mb)) / ((1 - p) * (1 - p));
  const v = meanVar(mem, ma) + meanVar(mem, mb) - (2 * mem.sigma2 * cross) / (ma * mb);
  return Math.max(v, 0);
}

/**
 * The rating row under the memory model: each window's mean with its 80% interval, and the
 * difference `mean(now) - mean(then)`, called only when its 80% interval excludes 0. A series that
 * never moved gets no verdict.
 */
export function memoryCompare(
  ratings: readonly number[],
  then: [number, number],
  now: [number, number],
): { rating: MeanComparison; sides: Pair<Estimate> } {
  const mean = (r: [number, number]) => {
    let s = 0;
    for (let i = r[0]; i < r[1]; i++) s += ratings[i] as number;
    return s / (r[1] - r[0]);
  };
  const ma = mean(then);
  const mb = mean(now);
  const diff = mb - ma;
  const mem = memoryOf(ratings);
  const side = (m: number, r: [number, number]): Estimate => {
    const n = r[1] - r[0];
    if (!mem) return { kind: "estimate", value: m, interval: null, n };
    const h = Z[0.8] * Math.sqrt(meanVar(mem, n));
    return { kind: "estimate", value: m, interval: finishInterval(m - h, m + h, RATING_DOMAIN), n };
  };
  const sides = { a: side(ma, then), b: side(mb, now) };
  const base = {
    n: { a: then[1] - then[0], b: now[1] - now[0] },
    mean: { a: ma, b: mb },
  };
  if (!mem) {
    return {
      rating: {
        verdict: "no_clear_change",
        ...base,
        diff: { value: diff, interval: { low: diff, high: diff, level: CONFIDENCE } },
      },
      sides,
    };
  }
  const h = Z[0.8] * Math.sqrt(diffVar(mem, then, now));
  const low = diff - h;
  const high = diff + h;
  const verdict = low > 0 ? "better" : high < 0 ? "worse" : "no_clear_change";
  return {
    rating: { verdict, ...base, diff: { value: diff, interval: { low, high, level: CONFIDENCE } } },
    sides,
  };
}
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

  // The first games against the last (EXP-0030): no window is placed on a selected peak, which
  // is a maximum of the same noise the verdict tests.
  const size = Math.min(TRAJECTORY_WINDOW, Math.floor(n / 2));
  const nowStart = n - size;
  const thenStart = 0;
  const then = windowOf(series, thenStart, size, "start");
  const now = windowOf(series, nowStart, size, "last");

  const ids = {
    a: series.slice(thenStart, thenStart + size).map((p) => p.id),
    b: series.slice(nowStart).map((p) => p.id),
  };
  const { rating, sides: ratingSides } = memoryCompare(
    series.map((p) => p.rating),
    [thenStart, thenStart + size],
    [nowStart, n],
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
    change: Math.round(now.meanRating - then.meanRating),
    then,
    now,
    rating,
    ratingSides,
    rows,
  };
}

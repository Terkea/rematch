import { fnv1a, mulberry32 } from "./bootstrap";
import { BOOTSTRAP_RESAMPLES, CONFIDENCE, KPI_MIN_GAMES } from "./constants";
import { estimateMean } from "./estimate";
import {
  type BetaPosterior,
  P_IMP_DRAWS,
  P_IMP_SEED,
  type Prior,
  pImprovement,
  posterior,
  pRegression,
} from "./posterior";
import type { Interval } from "./wilson";

// Period comparison, A (the earlier period) against B (the later one). A change is called only
// when both sides hold the minimum evidence and the change clears the engine's bar (the timeline's
// `CONFIRM_P_CHANGE` and `CONFIRM_REL_CHANGE`, `profile-core` timeline/series.rs); otherwise the
// verdict is "no clear change" or "not enough games", never a guess.

/** Probability a rate change must reach before it is called (Rust `CONFIRM_P_CHANGE`). */
export const CONFIRM_P_CHANGE = 0.8;
/** Relative change a rate must reach before it is called (Rust `CONFIRM_REL_CHANGE`). */
export const CONFIRM_REL_CHANGE = 0.15;

export type Side = "a" | "b";
export type Pair<T> = { a: T; b: T };

export type NotEnough = {
  verdict: "not_enough_games";
  /** The side (or both) below the minimum. */
  short: Side | "both";
  /** More games each side needs (0 for a side that has enough). */
  needed: Pair<number>;
  n: Pair<number>;
};

/** `better` and `worse` are from the player's point of view, for B against A. */
export type Called = "better" | "worse" | "no_clear_change";

export type RateComparison =
  | NotEnough
  | {
      verdict: Called;
      n: Pair<number>;
      posterior: Pair<BetaPosterior>;
      /** P(B's true rate is better than A's), from the two posteriors. */
      pBetter: number;
      /** P(B's true rate is worse than A's). */
      pWorse: number;
      /** `|mean_b / mean_a - 1|` of the posterior means (Infinity from a zero A to a positive B). */
      relChange: number;
    };

export type MeanComparison =
  | NotEnough
  | {
      verdict: Called;
      n: Pair<number>;
      mean: Pair<number>;
      /** `mean_b - mean_a` with its seeded bootstrap 80% interval. */
      diff: { value: number; interval: Interval };
    };

/** `k` events in `n` opportunities over `games` games, in one closed period. */
export type RateSide = { k: number; n: number; games: number };

const finiteCount = (x: number) => (Number.isFinite(x) && x > 0 ? Math.floor(x) : 0);

function notEnough(n: Pair<number>, minGames: number): NotEnough | null {
  const needed = {
    a: Math.max(0, minGames - n.a),
    b: Math.max(0, minGames - n.b),
  };
  if (needed.a === 0 && needed.b === 0) return null;
  const short = needed.a > 0 && needed.b > 0 ? "both" : needed.a > 0 ? "a" : "b";
  return { verdict: "not_enough_games", short, needed, n };
}

function relativeChange(a: number, b: number): number {
  if (a > 0) return Math.abs(b / a - 1);
  return b > 0 ? Number.POSITIVE_INFINITY : 0;
}

/**
 * A rate in A against B. Each side gets the posterior of its closed window under the same prior
 * (weight 1, no decay). The verdict is `better` (or `worse`) only when both sides have at least
 * `minGames` games, P(better) (or P(worse)) is at least `CONFIRM_P_CHANGE`, and the posterior
 * means differ by at least `CONFIRM_REL_CHANGE` relative to A.
 */
export function compareRates(
  a: RateSide,
  b: RateSide,
  opts: { prior: Prior; higherIsBetter: boolean; minGames?: number; seed?: number },
): RateComparison {
  const n = { a: finiteCount(a.games), b: finiteCount(b.games) };
  const short = notEnough(n, opts.minGames ?? KPI_MIN_GAMES);
  if (short) return short;
  const pa = posterior(a.k, a.n, opts.prior);
  const pb = posterior(b.k, b.n, opts.prior);
  const mc = { draws: P_IMP_DRAWS, seed: opts.seed ?? P_IMP_SEED };
  // P(theta_b < theta_a) and P(theta_b > theta_a), from the same seeded stream.
  const pLower = pImprovement(pb, pa, { ...mc, deltaRel: 0 });
  const pHigher = pRegression(pb, pa, { ...mc, factor: 1 });
  const pBetter = opts.higherIsBetter ? pHigher : pLower;
  const pWorse = opts.higherIsBetter ? pLower : pHigher;
  const relChange = relativeChange(pa.mean, pb.mean);
  const big = relChange >= CONFIRM_REL_CHANGE;
  const verdict: Called =
    big && pBetter >= CONFIRM_P_CHANGE
      ? "better"
      : big && pWorse >= CONFIRM_P_CHANGE
        ? "worse"
        : "no_clear_change";
  return { verdict, n, posterior: { a: pa, b: pb }, pBetter, pWorse, relChange };
}

/** The seeded percentile bootstrap of `mean(b) - mean(a)`, resampling each side independently. */
function bootstrapDiff(a: number[], b: number[], seed: string): Interval {
  const rnd = mulberry32(fnv1a(seed));
  const resampledMean = (xs: number[]) => {
    let sum = 0;
    for (let i = 0; i < xs.length; i++) sum += xs[Math.floor(rnd() * xs.length)] as number;
    return sum / xs.length;
  };
  const diffs = new Float64Array(BOOTSTRAP_RESAMPLES);
  for (let r = 0; r < BOOTSTRAP_RESAMPLES; r++) diffs[r] = resampledMean(b) - resampledMean(a);
  diffs.sort();
  const tail = (1 - CONFIDENCE) / 2;
  const at = (q: number) =>
    diffs[
      Math.min(BOOTSTRAP_RESAMPLES - 1, Math.max(0, Math.floor(q * BOOTSTRAP_RESAMPLES)))
    ] as number;
  return { low: at(tail), high: at(1 - tail), level: CONFIDENCE };
}

/**
 * A per-game mean (accuracy, blunders per game, ...) in A against B. Non-finite values are
 * dropped and not counted. A change is called only when both sides have `KPI_MIN_GAMES` values
 * and the seeded bootstrap 80% interval of `mean_b - mean_a` excludes 0.
 */
export function compareMeans(
  a: number[],
  b: number[],
  opts: { higherIsBetter: boolean; seed: string },
): MeanComparison {
  const ea = estimateMean(a, `${opts.seed}:a`, { min: -Infinity, max: Infinity });
  const eb = estimateMean(b, `${opts.seed}:b`, { min: -Infinity, max: Infinity });
  const n = { a: ea.n, b: eb.n };
  const short = notEnough(n, KPI_MIN_GAMES);
  if (short || ea.kind !== "estimate" || eb.kind !== "estimate") {
    return short ?? { verdict: "not_enough_games", short: "both", needed: { a: 0, b: 0 }, n };
  }
  const clean = (xs: number[]) => xs.filter((v) => Number.isFinite(v));
  const interval = bootstrapDiff(clean(a), clean(b), `${opts.seed}:diff`);
  const up = interval.low > 0;
  const down = interval.high < 0;
  const verdict: Called =
    up || down ? (up === opts.higherIsBetter ? "better" : "worse") : "no_clear_change";
  return {
    verdict,
    n,
    mean: { a: ea.value, b: eb.value },
    diff: { value: eb.value - ea.value, interval },
  };
}

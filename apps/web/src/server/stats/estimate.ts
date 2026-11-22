import { bootstrapMean, nonDegenerate } from "./bootstrap";
import { KPI_MIN_GAMES } from "./constants";
import { type Interval, wilson } from "./wilson";

export type Estimate =
  | { kind: "estimate"; value: number; interval: Interval; n: number }
  | { kind: "insufficient"; n: number; needed: number };

function insufficient(n: number): Estimate {
  return { kind: "insufficient", n, needed: Math.max(0, KPI_MIN_GAMES - n) };
}

/** Mean with a seeded bootstrap interval. Non-finite values are dropped, and do not count in n. */
export function estimateMean(values: number[], seed: string): Estimate {
  const clean = values.filter((v) => Number.isFinite(v));
  if (clean.length < KPI_MIN_GAMES) return insufficient(clean.length);
  const b = bootstrapMean(clean, seed);
  if (!b) return insufficient(clean.length);
  const [low, high] = nonDegenerate(b.mean, b.interval.low, b.interval.high);
  return {
    kind: "estimate",
    value: b.mean,
    interval: { low, high, level: b.interval.level },
    n: clean.length,
  };
}

/** A rate (successes / n) with a Wilson interval. `successes` may be fractional (draws count half). */
export function estimateRate(successes: number, n: number): Estimate {
  if (!Number.isFinite(n) || !Number.isFinite(successes) || n < KPI_MIN_GAMES) {
    return insufficient(Number.isFinite(n) && n > 0 ? n : 0);
  }
  const s = Math.min(Math.max(successes, 0), n);
  const interval = wilson(s, n);
  if (!interval) return insufficient(0);
  return { kind: "estimate", value: s / n, interval, n };
}

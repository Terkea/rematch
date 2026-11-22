import { Z } from "./constants";

export type Interval = { low: number; high: number; level: 0.8 };

/** Wilson score interval. `successes` may be fractional (a draw counts half). Null when n is 0. */
export function wilson(successes: number, n: number, level: 0.8 = 0.8): Interval | null {
  if (!Number.isFinite(n) || !Number.isFinite(successes) || n <= 0) return null;
  const s = Math.min(Math.max(successes, 0), n);
  const z = Z[level];
  const z2 = z * z;
  const p = s / n;
  const denom = 1 + z2 / n;
  const centre = (p + z2 / (2 * n)) / denom;
  const half = (z * Math.sqrt((p * (1 - p)) / n + z2 / (4 * n * n))) / denom;
  const low = s === 0 ? 0 : Math.max(0, centre - half);
  const high = s === n ? 1 : Math.min(1, centre + half);
  return { low, high, level };
}

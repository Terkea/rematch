import { BOOTSTRAP_RESAMPLES, CONFIDENCE, MIN_HALF_WIDTH } from "./constants";
import type { Interval } from "./wilson";

export function fnv1a(text: string): number {
  let h = 0x811c9dc5;
  for (let i = 0; i < text.length; i++) {
    h ^= text.charCodeAt(i);
    h = Math.imul(h, 0x01000193);
  }
  return h >>> 0;
}

export function mulberry32(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/** Percentile bootstrap of the mean. Null when empty. One value gives a degenerate interval. */
export function bootstrapMean(
  values: number[],
  seed: string,
): { mean: number; interval: Interval } | null {
  const n = values.length;
  if (n === 0) return null;
  const mean = values.reduce((a, b) => a + b, 0) / n;
  const rnd = mulberry32(fnv1a(seed));
  const means = new Float64Array(BOOTSTRAP_RESAMPLES);
  for (let r = 0; r < BOOTSTRAP_RESAMPLES; r++) {
    let sum = 0;
    for (let i = 0; i < n; i++) sum += values[Math.floor(rnd() * n)] as number;
    means[r] = sum / n;
  }
  means.sort();
  const tail = (1 - CONFIDENCE) / 2;
  const at = (q: number) =>
    means[
      Math.min(BOOTSTRAP_RESAMPLES - 1, Math.max(0, Math.floor(q * BOOTSTRAP_RESAMPLES)))
    ] as number;
  const low = Math.min(at(tail), mean);
  const high = Math.max(at(1 - tail), mean);
  return { mean, interval: { low, high, level: CONFIDENCE } };
}

/** Widens a degenerate band so a drawn interval is never zero-width. */
export function nonDegenerate(mean: number, low: number, high: number): [number, number] {
  return [Math.min(low, mean - MIN_HALF_WIDTH), Math.max(high, mean + MIN_HALF_WIDTH)];
}

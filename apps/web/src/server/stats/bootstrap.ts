import { BOOTSTRAP_RESAMPLES, CONFIDENCE } from "./constants";
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

/**
 * The stream seed of a bootstrap: a stable hash of `label` and the samples (length, then the
 * little-endian words of each value, sorted; the Rust profile crate hashes the same way). The same
 * data gives the same stream whatever the row order, an id or a user id in the label never moves
 * it, and different data gives a different stream. Pass a label without ids (a metric name).
 */
/** A sorted copy: resampling and summing run in a canonical order, so row order cannot move a figure. */
export function canonical(sample: readonly number[]): number[] {
  return Array.from(Float64Array.from(sample).sort());
}

export function dataSeed(label: string, ...samples: readonly number[][]): number {
  const words = new Uint32Array(new Float64Array(1).buffer);
  const view = new Float64Array(words.buffer);
  let h = fnv1a(label);
  const mix = (w: number) => {
    h = Math.imul(h ^ w, 0x01000193) >>> 0;
  };
  for (const sample of samples) {
    mix(sample.length);
    for (const v of canonical(sample)) {
      view[0] = v;
      mix(words[0] as number);
      mix(words[1] as number);
    }
  }
  return h;
}

/** Percentile bootstrap of the mean. Null when empty. One value gives a degenerate interval. */
export function bootstrapMean(
  raw: number[],
  seed: string,
): { mean: number; interval: Interval } | null {
  const values = canonical(raw);
  const n = values.length;
  if (n === 0) return null;
  const mean = values.reduce((a, b) => a + b, 0) / n;
  const rnd = mulberry32(dataSeed(seed, values));
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
  return { mean, interval: { low: at(tail), high: at(1 - tail), level: CONFIDENCE } };
}

export type Domain = { min: number; max: number };

/**
 * Clamps a band to the metric's domain (counts at least 0, percentages within 0 to 100). A band
 * with no spread cannot be estimated, so it is null; it is never widened.
 */
export function finishInterval(low: number, high: number, domain: Domain): Interval | null {
  const lo = Math.max(low, domain.min);
  const hi = Math.min(high, domain.max);
  if (!Number.isFinite(lo) || !Number.isFinite(hi) || hi - lo <= 0) return null;
  return { low: lo, high: hi, level: CONFIDENCE };
}

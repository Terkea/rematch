import { finishInterval } from "./bootstrap";
import { T80, TREND_MIN, TREND_WINDOW, Z } from "./constants";

type Rolling = { id: string; mean: number; low: number | null; high: number | null };

/**
 * Trailing mean of a percentage with an 80% Student t band, one entry per point once the window
 * holds TREND_MIN values. The band is analytic (no resampling), so it is cheap and deterministic.
 * It is null when the window has no spread, and clamped to 0 to 100. `_seed` is kept for the
 * brief's signature.
 */
export function rollingMean(points: { id: string; value: number }[], _seed: string): Rolling[] {
  const out: Rolling[] = [];
  const finite = points.filter((p) => Number.isFinite(p.value));
  for (let i = 0; i < finite.length; i++) {
    const start = Math.max(0, i - TREND_WINDOW + 1);
    const n = i - start + 1;
    if (n < TREND_MIN) continue;
    let sum = 0;
    for (let j = start; j <= i; j++) sum += (finite[j] as { value: number }).value;
    const mean = sum / n;
    let ss = 0;
    for (let j = start; j <= i; j++) ss += ((finite[j] as { value: number }).value - mean) ** 2;
    const half = (T80[n - 1] ?? Z[0.8]) * Math.sqrt(ss / (n - 1) / n);
    const band = finishInterval(mean - half, mean + half, { min: 0, max: 100 });
    out.push({
      id: (finite[i] as { id: string }).id,
      mean,
      low: band?.low ?? null,
      high: band?.high ?? null,
    });
  }
  return out;
}

import { bootstrapMean, nonDegenerate } from "./bootstrap";
import { TREND_MIN, TREND_WINDOW } from "./constants";

/** Trailing mean with an 80% band, one entry per point once the window holds TREND_MIN values. */
export function rollingMean(
  points: { id: string; value: number }[],
  seed: string,
): { id: string; mean: number; low: number; high: number }[] {
  const out: { id: string; mean: number; low: number; high: number }[] = [];
  const finite = points.filter((p) => Number.isFinite(p.value));
  for (let i = 0; i < finite.length; i++) {
    const window = finite.slice(Math.max(0, i - TREND_WINDOW + 1), i + 1);
    if (window.length < TREND_MIN) continue;
    const point = finite[i];
    const b = bootstrapMean(
      window.map((p) => p.value),
      `${seed}:${point?.id}`,
    );
    if (!point || !b) continue;
    const [low, high] = nonDegenerate(b.mean, b.interval.low, b.interval.high);
    out.push({ id: point.id, mean: b.mean, low, high });
  }
  return out;
}

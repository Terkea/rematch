import type { SideRate } from "@ci/ui/profile";
import type { WeaknessSummary } from "../../../../server/profile/types";

/** Each series' White and Black estimate (null where a colour has no row), by series key. */
export function sideRates(
  white: WeaknessSummary[],
  black: WeaknessSummary[],
): Map<string, { white: SideRate | null; black: SideRate | null }> {
  const rate = (w: WeaknessSummary): SideRate => ({
    mean: w.rate.mean,
    chances: w.evidence.chances,
    sufficient: w.evidence.sufficient,
  });
  const out = new Map<string, { white: SideRate | null; black: SideRate | null }>();
  for (const w of white) out.set(w.seriesKey, { white: rate(w), black: null });
  for (const b of black) {
    const cur = out.get(b.seriesKey) ?? { white: null, black: null };
    out.set(b.seriesKey, { ...cur, black: rate(b) });
  }
  return out;
}

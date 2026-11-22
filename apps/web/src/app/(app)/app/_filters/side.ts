import type { StatsFilters } from "../../../../server/filters/params";

/**
 * The side of the stored per-colour estimates (weakness estimates, KPI snapshots, band
 * reference) that the colour filter reads. Every read of them takes its side from here, so the
 * colour filter reaches them in one line.
 *
 * TODO(ADR 0082): return `f.color` once `feat/colour-split` lands the `side` column
 * ('both' | 'white' | 'black') and the read models take it. Until then the stored data is
 * Both only, and the bar says so ("Colour split for weaknesses is coming").
 */
export function storedSide(_f: Pick<StatsFilters, "color">): "both" {
  return "both";
}

/**
 * The coach scope with the colour filter's stored side attached. The read models ignore `side`
 * today; once ADR 0082's read models take it, they read it from here and nothing else changes.
 */
export function withSide<S extends object>(
  scope: S,
  f: Pick<StatsFilters, "color">,
): S & { side: ReturnType<typeof storedSide> } {
  return { ...scope, side: storedSide(f) };
}

import type { StatsFilters } from "../../../../server/filters/params";
import type { Side } from "../../../../server/profile/types";

/**
 * The side of the stored per-colour rows (weakness estimates, KPI snapshots, area scores and
 * the peer benchmark's rows, ADR 0082) that the colour filter reads. Timeline states have no
 * side: they are always every game's.
 */
export function storedSide(f: Pick<StatsFilters, "color">): Side {
  return f.color;
}

/** The coach scope with the colour filter's side, which the profile read models read. */
export function withSide<S extends object>(
  scope: S,
  f: Pick<StatsFilters, "color">,
): S & { side: Side } {
  return { ...scope, side: storedSide(f) };
}

/** "White", "Black", or null for both. */
export const sideName = (side: Side | undefined) =>
  side === "white" ? "White" : side === "black" ? "Black" : null;

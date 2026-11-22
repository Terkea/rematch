import { kpiDef } from "../../../../../server/profile/catalogue";
import type { Area, KpiCurrent } from "../../../../../server/profile/types";

/**
 * The components of each area's score, as Rust `profile_core::kpi::area_scores::area_components`
 * lists them (catalogue metric ids). Rust's area `n` is the smallest sample among them: games for
 * a mean component, chances (opportunities) for a rate component.
 */
export const AREA_COMPONENTS: Record<Area, readonly string[]> = {
  openings: ["accuracy.opening", "eval_after_opening", "weakness:W7.traps", "weakness:W7.prep"],
  middlegame: ["accuracy.middlegame", "weakness:W10", "weakness:W5", "weakness:W6"],
  endgame: ["accuracy.endgame", "weakness:W8.basics", "weakness:W8.advanced"],
  tactics: ["weakness:W3.shallow", "weakness:W3.deep", "weakness:W4.missed", "missed_punish_rate"],
  calculation: [
    "weakness:W1",
    "weakness:W2",
    "weakness:W11",
    "weakness:W4.allowed",
    "blunders_per_game",
  ],
  time: ["weakness:W9a", "weakness:W9b", "weakness:W9c", "critical_time_ratio"],
  mindset: ["score_after_loss", "session_accuracy_decline"],
};

/** What an area score's `n` counts: the component it came from, or null when none matches. */
export type ScoreBasis =
  | { n: number; label: string; unit: "games" | "chances" }
  | { n: number; label: null; unit: null };

/**
 * Names the sample behind an area's `n` by finding the component whose current (last-50) row has
 * that n. Any match is a true statement of the count (two components with the same n count the
 * same number), so the first in Rust's order is used. No match names no unit.
 */
export function scoreBasis(area: Area, n: number, current: KpiCurrent[]): ScoreBasis {
  for (const metric of AREA_COMPONENTS[area]) {
    const row = current.find((k) => k.metric === metric);
    if (!row || row.n !== n) continue;
    const kind = kpiDef(metric)?.kind;
    return { n, label: row.label, unit: kind === "rate" ? "chances" : "games" };
  }
  return { n, label: null, unit: null };
}

/**
 * The unit of an area's `need_more`. Rust takes it from the first benchmarked component short of
 * its minimum (`min_n - n`, a missing row counting 0), so it is games or chances by that
 * component's kind. The first component in Rust's order whose shortfall equals `needMore` is it;
 * none matching names no unit.
 */
export function shortfallUnit(
  area: Area,
  needMore: number,
  current: KpiCurrent[],
): "games" | "chances" | null {
  for (const metric of AREA_COMPONENTS[area]) {
    const def = kpiDef(metric);
    if (!def) continue;
    const row = current.find((k) => k.metric === metric);
    const short = def.minN - (row?.n ?? 0);
    if (short === needMore) return def.kind === "rate" ? "chances" : "games";
  }
  return null;
}

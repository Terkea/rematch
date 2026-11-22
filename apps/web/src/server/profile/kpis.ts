import type { Db } from "@ci/db";
import { kpiSnapshot } from "@ci/db/generated";
import { and, desc, eq, inArray } from "drizzle-orm";
import { readOnly } from "../games";
import { KPIS, type KpiDef, kpiDef } from "./catalogue";
import { ownsScope } from "./scope";
import type { CoachScope, KpiCurrent, KpiTrend } from "./types";

/** The window of the current value of every KPI and area score. */
export const CURRENT_WINDOW = "last50";

type SnapshotRow = Pick<
  typeof kpiSnapshot.$inferSelect,
  "metric" | "windowKey" | "value" | "ciLow" | "ciHigh" | "n" | "computedAt"
>;

/** The newest row per (metric, window) of the scope, for the given metrics. */
export async function latestSnapshots(
  tx: Db,
  scope: CoachScope,
  metrics: string[] | null,
  windowKey?: string,
): Promise<SnapshotRow[]> {
  if (metrics != null && metrics.length === 0) return [];
  return tx
    .selectDistinctOn([kpiSnapshot.metric, kpiSnapshot.windowKey], {
      metric: kpiSnapshot.metric,
      windowKey: kpiSnapshot.windowKey,
      value: kpiSnapshot.value,
      ciLow: kpiSnapshot.ciLow,
      ciHigh: kpiSnapshot.ciHigh,
      n: kpiSnapshot.n,
      computedAt: kpiSnapshot.computedAt,
    })
    .from(kpiSnapshot)
    .where(
      and(
        eq(kpiSnapshot.accountId, scope.accountId),
        eq(kpiSnapshot.timeClass, scope.timeClass),
        // The colour filter's side (ADR 0082); every game unless the scope names a colour.
        eq(kpiSnapshot.side, scope.side ?? "both"),
        metrics == null ? undefined : inArray(kpiSnapshot.metric, metrics),
        windowKey == null ? undefined : eq(kpiSnapshot.windowKey, windowKey),
      ),
    )
    .orderBy(
      kpiSnapshot.metric,
      kpiSnapshot.windowKey,
      desc(kpiSnapshot.computedAt),
      desc(kpiSnapshot.id),
    );
}

/** A stored row as a current value: nulls stay null, and `n` is checked against `minN`. */
export function kpiCurrent(def: KpiDef, row: SnapshotRow): KpiCurrent {
  const sufficient = row.n >= def.minN;
  return {
    metric: def.metric,
    label: def.label,
    unit: def.unit,
    higherIsBetter: def.higherIsBetter,
    value: row.value,
    low: row.ciLow,
    high: row.ciHigh,
    n: row.n,
    minN: def.minN,
    sufficient,
    needMore: sufficient ? null : def.minN - row.n,
    computedAt: row.computedAt,
  };
}

/**
 * KPI trends: per metric, the newest row per window; weeks oldest first, `current` the newest
 * `last50` row. `metrics` keeps its order and drops ids outside the catalogue; omitted, it is
 * every catalogue metric that has a row, in catalogue order.
 */
export async function kpiTrends(
  db: Db,
  userId: string,
  scope: CoachScope,
  metrics?: string[],
): Promise<KpiTrend[]> {
  const defs = metrics ? metrics.map(kpiDef).filter((d): d is KpiDef => d != null) : [...KPIS];
  return readOnly(db, async (tx) => {
    const owned = await ownsScope(tx, userId, scope);
    const rows = owned
      ? await latestSnapshots(
          tx,
          scope,
          defs.map((d) => d.metric),
        )
      : [];
    const byMetric = new Map<string, SnapshotRow[]>();
    for (const r of rows) byMetric.set(r.metric, [...(byMetric.get(r.metric) ?? []), r]);
    return defs
      .filter((d) => metrics != null || byMetric.has(d.metric))
      .map((d) => {
        const own = byMetric.get(d.metric) ?? [];
        const current = own.find((r) => r.windowKey === CURRENT_WINDOW);
        return {
          metric: d.metric,
          label: d.label,
          unit: d.unit,
          current: current ? kpiCurrent(d, current) : null,
          points: own
            .filter((r) => /^\d{4}-W\d{2}$/.test(r.windowKey))
            .sort((a, b) => (a.windowKey < b.windowKey ? -1 : 1))
            .map((r) => ({
              week: r.windowKey,
              value: r.value,
              low: r.ciLow,
              high: r.ciHigh,
              n: r.n,
            })),
        };
      });
  });
}

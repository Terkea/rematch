import type { Db } from "@ci/db";
import { weaknessStateHistory } from "@ci/db/generated";
import { and, asc, eq } from "drizzle-orm";
import { readOnly } from "../games";
import { AREAS, KPIS, seriesArea, seriesLabel } from "./catalogue";
import { CURRENT_WINDOW, kpiCurrent, latestSnapshots } from "./kpis";
import { ownsScope } from "./scope";
import type { Area, AreaView, CoachScope, TimelineSeries } from "./types";
import { openingNames, summariesIn, timelineOf } from "./weaknesses";

const areaMetric = (area: Area) => `area:${area}`;

/** One skill area: its `last50` score (null stays null), KPIs, weaknesses and state timelines. */
export async function getArea(
  db: Db,
  userId: string,
  scope: CoachScope,
  area: Area,
): Promise<AreaView> {
  return readOnly(db, async (tx) => {
    const empty: AreaView = { area, score: null, kpis: [], weaknesses: [], timeline: [] };
    if (!(await ownsScope(tx, userId, scope))) return empty;

    const defs = KPIS.filter((d) => d.area === area);
    const rows = await latestSnapshots(
      tx,
      scope,
      [areaMetric(area), ...defs.map((d) => d.metric)],
      CURRENT_WINDOW,
    );
    const rowOf = new Map(rows.map((r) => [r.metric, r]));
    const scoreRow = rowOf.get(areaMetric(area));
    const kpis = defs.flatMap((d) => {
      const row = rowOf.get(d.metric);
      return row ? [kpiCurrent(d, row)] : [];
    });

    const weaknesses = (await summariesIn(tx, scope)).filter((w) => w.area === area);

    const history = await tx
      .select()
      .from(weaknessStateHistory)
      .where(
        and(
          eq(weaknessStateHistory.accountId, scope.accountId),
          eq(weaknessStateHistory.timeClass, scope.timeClass),
        ),
      )
      .orderBy(asc(weaknessStateHistory.id));
    const bySeries = new Map<string, typeof history>();
    for (const r of history) {
      if (seriesArea(r.seriesKey) !== area) continue;
      bySeries.set(r.seriesKey, [...(bySeries.get(r.seriesKey) ?? []), r]);
    }
    const names = await openingNames(tx, scope, [...bySeries.keys()]);
    const uncompared = new Set(weaknesses.filter((w) => !w.compared).map((w) => w.seriesKey));
    const timeline: TimelineSeries[] = [...bySeries]
      .map(([key, rs]) => ({
        ...timelineOf(key, seriesLabel(key, names.get(key)), rs),
        ...(uncompared.has(key) ? { compared: false } : {}),
      }))
      .filter((t) => t.segments.length > 0)
      .sort((a, b) => (a.label < b.label ? -1 : a.label > b.label ? 1 : 0));

    return {
      area,
      score: scoreRow
        ? { value: scoreRow.value, n: scoreRow.n, computedAt: scoreRow.computedAt }
        : null,
      kpis,
      weaknesses,
      timeline,
    };
  });
}

/** The seven area scores in display order; an area without a row is `{ value: null, n: 0 }`. */
export async function listAreaScores(
  db: Db,
  userId: string,
  scope: CoachScope,
): Promise<{ area: Area; value: number | null; n: number }[]> {
  return readOnly(db, async (tx) => {
    const rows = (await ownsScope(tx, userId, scope))
      ? await latestSnapshots(tx, scope, AREAS.map(areaMetric), CURRENT_WINDOW)
      : [];
    const rowOf = new Map(rows.map((r) => [r.metric, r]));
    return AREAS.map((area) => {
      const row = rowOf.get(areaMetric(area));
      return { area, value: row?.value ?? null, n: row?.n ?? 0 };
    });
  });
}

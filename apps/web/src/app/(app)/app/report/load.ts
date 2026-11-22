import type { Db } from "@ci/db";
import { changeNotes, getReport } from "../../../../server/coaching";
import type { ReportPayload } from "../../../../server/coaching/types";
import type { CoachScope } from "../../../../server/profile";
import { noteKeysInScope, reviewGameIds, weeklyRates } from "../weaknesses/queries";
import { changeKey, reportGameIds } from "./_sections/links";
import type { ReportPageData } from "./report-view";

/**
 * Model statements keep their validated `segments` only: the raw `text` (with `{{…}}` tokens) is
 * never sent to the browser (ADR 0060), so it is dropped from every `{ text, segments }` object.
 */
export function withoutRawText<T>(value: T): T {
  if (Array.isArray(value)) return value.map((v) => withoutRawText(v)) as T;
  if (value && typeof value === "object" && !(value instanceof Date)) {
    const o = value as Record<string, unknown>;
    const isStatement = Array.isArray(o.segments) && typeof o.text === "string";
    const out: Record<string, unknown> = {};
    for (const [k, v] of Object.entries(o)) {
      if (isStatement && k === "text") continue;
      out[k] = withoutRawText(v);
    }
    return out as T;
  }
  return value;
}

/**
 * Everything `/app/report` shows for one scope, in a fixed number of reads whatever the number
 * of weaknesses (no per-weakness query): the report, the change notes written for this scope's
 * time class and for the exact change shown, the weekly rates of every series in one query, and
 * the review ids of the cited games. Every read checks the scope against the user's accounts.
 */
export async function loadReport(
  db: Db,
  userId: string,
  scope: CoachScope,
  now: Date,
  intervalDays: number,
): Promise<ReportPageData> {
  const view = await getReport(db, userId, scope, now, { intervalDays });
  const computed = view.payload?.computed;
  const changes = computed ? [...computed.newThisPeriod, ...computed.fixedThisPeriod] : [];
  const shown = new Set(changes.map(changeKey));
  const series = [...new Set(changes.map((c) => c.seriesKey))];

  const [notes, keysPerSeries, weekly] = await Promise.all([
    series.length > 0 ? changeNotes(db, userId, scope.accountId, series) : [],
    Promise.all(series.map((s) => noteKeysInScope(db, userId, scope, s))),
    computed && computed.weaknesses.length > 0 ? weeklyRates(db, userId, scope) : new Map(),
  ]);
  const inScope = new Set(keysPerSeries.flatMap((k) => [...k]));
  const scopedNotes = notes.filter((n) => inScope.has(n.changeKey) && shown.has(n.changeKey));

  const cited = [
    ...(view.payload ? reportGameIds(view.payload) : []),
    ...reportGameIds({
      sections: { notes: scopedNotes.map((n) => n.item) },
      computed: { weaknesses: [], openings: [], strengths: [] },
    }),
  ];
  const gameIds = await reviewGameIds(db, userId, scope, cited);

  return {
    scope,
    view: {
      ...view,
      payload: view.payload ? withoutRawText<ReportPayload>(view.payload) : null,
    },
    notes: withoutRawText(scopedNotes),
    trends: Object.fromEntries(weekly),
    gameIds: Object.fromEntries(gameIds),
    now,
  };
}

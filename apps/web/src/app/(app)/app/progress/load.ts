import type { Db } from "@ci/db";
import { betaReportIntervalDays, getReport, listWeeklyReviews } from "../../../../server/coaching";
import {
  type CoachScope,
  getWeakness,
  kpiTrends,
  listWeaknesses,
  type WeaknessDetail,
} from "../../../../server/profile";
import { PROGRESS_METRICS, type ProgressInput } from "./progress-view";

/**
 * Everything the progress page shows, read through the read models only: each one checks that
 * the user owns the scope (ADR 0064 §19), so a foreign scope reads as empty.
 */
export async function loadProgress(
  db: Db,
  userId: string,
  scope: CoachScope,
  now: Date = new Date(),
): Promise<ProgressInput> {
  const target = { accountId: scope.accountId, timeClass: scope.timeClass };
  const [trends, summaries, weekly, report] = await Promise.all([
    kpiTrends(db, userId, scope, [...PROGRESS_METRICS]),
    listWeaknesses(db, userId, scope),
    listWeeklyReviews(db, userId, target),
    getReport(db, userId, target, now, { intervalDays: betaReportIntervalDays() }),
  ]);
  const details = await Promise.all(
    summaries.map((w) => getWeakness(db, userId, scope, w.seriesKey)),
  );
  const option = scope.options.find(
    (o) => o.accountId === scope.accountId && o.timeClass === scope.timeClass,
  );
  return {
    now,
    basis: option ? { analysedGames: option.analysedGames, timeClass: scope.timeClass } : null,
    trends,
    weaknesses: details.filter((d): d is WeaknessDetail => d != null),
    reportAt: report.createdAt,
    weekly,
  };
}

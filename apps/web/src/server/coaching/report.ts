import type { Db } from "@ci/db";
import { reportRuns, reportSections, reports } from "@ci/db/generated";
import { and, desc, eq } from "drizzle-orm";
import { isVerifiedLink } from "../accounts/verified";
import { readOnly, scopedAccountIds } from "../games";
import { assembleReportPayload } from "./payload";
import { type ReportState, reportState } from "./state";
import {
  type CoachTarget,
  type ReportPayload,
  type ReportSectionRow,
  SECTION_KEYS,
  type SectionKey,
} from "./types";

/** Rust `FREE_REPORT_INTERVAL_DAYS`: a free plan gets one non-onboarding report per 30 days. */
export const FREE_REPORT_INTERVAL_DAYS = 30;
const DAY_MS = 86_400_000;

export type ReportView = {
  state: ReportState;
  payload: ReportPayload | null;
  createdAt: Date | null;
  nextManualAt: Date | null;
  /**
   * `unverified`: the user's link to the account is not verified, so the report is not theirs to
   * read (holistic F10). The page shows the calm "verify" state; nothing of the report is sent.
   */
  locked?: "unverified";
  /** The algorithm of the shown report's run (ADR 0105); null before versioning or no report. */
  algorithmId?: string | null;
};

const EMPTY: ReportView = {
  state: { kind: "none" },
  payload: null,
  createdAt: null,
  nextManualAt: null,
};

/** The sections of a report that are written (`ready` with content), in `SECTION_KEYS` order. */
function readyKeys(sections: ReportSectionRow[]): SectionKey[] {
  const ready = new Set(
    sections.filter((s) => s.status === "ready" && s.content != null).map((s) => s.section),
  );
  return SECTION_KEYS.filter((k) => ready.has(k));
}

/**
 * The scope's report: the newest run and the newest report (report rows are account data, so
 * every user linked to the account reads them; ADR 0064 §18). A foreign or malformed account
 * sees the empty view. `nextManualAt` is the newest report's `created_at` plus
 * `intervalDays` while that is in the future.
 */
export async function getReport(
  db: Db,
  userId: string,
  scope: CoachTarget,
  now: Date = new Date(),
  opts: { intervalDays?: number } = {},
): Promise<ReportView> {
  const intervalDays = opts.intervalDays ?? FREE_REPORT_INTERVAL_DAYS;
  return readOnly(db, async (tx) => {
    const ids = await scopedAccountIds(tx, userId, scope.accountId);
    if (ids.length === 0) return EMPTY;
    if (!(await isVerifiedLink(tx, userId, scope.accountId))) {
      return { ...EMPTY, locked: "unverified" };
    }
    const inScope = (t: typeof reportRuns | typeof reports) =>
      and(eq(t.accountId, scope.accountId), eq(t.timeClass, scope.timeClass));

    const [run] = await tx
      .select()
      .from(reportRuns)
      .where(inScope(reportRuns))
      .orderBy(desc(reportRuns.createdAt), desc(reportRuns.id))
      .limit(1);
    const [report] = await tx
      .select()
      .from(reports)
      .where(inScope(reports))
      .orderBy(desc(reports.createdAt), desc(reports.version))
      .limit(1);

    const sections = report
      ? await tx.select().from(reportSections).where(eq(reportSections.reportId, report.id))
      : [];
    let ready: SectionKey[] = [];
    if (run?.reportId) {
      ready =
        run.reportId === report?.id
          ? readyKeys(sections)
          : readyKeys(
              await tx
                .select()
                .from(reportSections)
                .where(eq(reportSections.reportId, run.reportId)),
            );
    }

    const algorithmId = !report?.reportRunId
      ? null
      : run && run.id === report.reportRunId
        ? run.algorithmId
        : ((
            await tx
              .select({ algorithmId: reportRuns.algorithmId })
              .from(reportRuns)
              .where(eq(reportRuns.id, report.reportRunId))
              .limit(1)
          )[0]?.algorithmId ?? null);
    const next = report ? new Date(report.createdAt.getTime() + intervalDays * DAY_MS) : null;
    return {
      state: reportState(run ?? null, report ?? null, ready),
      payload: report ? assembleReportPayload(report, sections) : null,
      createdAt: report?.createdAt ?? null,
      nextManualAt: next && next > now ? next : null,
      algorithmId,
    };
  });
}

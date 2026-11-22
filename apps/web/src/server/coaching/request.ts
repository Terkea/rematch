import type { Db } from "@ci/db";
import { reportRuns, reports } from "@ci/db/generated";
import { and, desc, eq, inArray, sql } from "drizzle-orm";
import { scopedAccountIds } from "../games";
import { enqueueJob, JOB } from "../jobs";
import { FREE_REPORT_INTERVAL_DAYS } from "./report";
import { LIVE_RUN_STATUSES } from "./state";
import type { CoachTarget } from "./types";

/** `report_runs.plan` the web writes (ruling R4; the worker's `Entitlements` decide quotas). */
export const AI_PLAN = "free" as const;
/** Rust `REPORT_PROMPT_VERSION`. */
export const REPORT_PROMPT_VERSION = "report@1";
/** `pg_advisory_xact_lock` class for report requests (R3 uses 30001–30004 and 30020). */
export const REPORT_REQUEST_LOCK_CLASS = 30030;
const DAY_MS = 86_400_000;

export type ReportRequest =
  | { kind: "queued"; runId: string }
  | { kind: "building"; runId: string }
  | { kind: "interval"; availableAt: Date }
  | { kind: "not_found" };

/**
 * "Build my report" / "Rebuild report" (ruling R4, ADR 0069). In ONE transaction, serialised per
 * scope by `pg_advisory_xact_lock(30030, hashtext(account_id || ':' || time_class))`:
 * - a live run (`queued`, `running`, `awaiting_retry`, `awaiting_batch`) is returned as is;
 * - with no report yet the trigger is `onboarding` (`is_first_report`), else `manual`;
 * - a `manual` request inside the interval (`intervalDays` since the newest report, default
 *   the free 30) is refused with its date, so the player never waits for a `plan_interval`;
 * - otherwise the run and its `generate-report` job are inserted together.
 * A foreign or malformed account is `not_found`.
 */
export async function requestReport(
  db: Db,
  userId: string,
  target: CoachTarget,
  now: Date = new Date(),
  opts: { intervalDays?: number } = {},
): Promise<ReportRequest> {
  const intervalDays = opts.intervalDays ?? FREE_REPORT_INTERVAL_DAYS;
  return db.transaction(async (raw) => {
    const tx = raw as unknown as Db;
    const ids = await scopedAccountIds(tx, userId, target.accountId);
    if (ids.length === 0) return { kind: "not_found" } as const;
    const { accountId, timeClass } = target;

    await tx.execute(
      sql`select pg_advisory_xact_lock(${REPORT_REQUEST_LOCK_CLASS}::int4, hashtext(${accountId}::text || ':' || ${timeClass}::text))`,
    );

    const [live] = await tx
      .select({ id: reportRuns.id })
      .from(reportRuns)
      .where(
        and(
          eq(reportRuns.accountId, accountId),
          eq(reportRuns.timeClass, timeClass),
          inArray(reportRuns.status, [...LIVE_RUN_STATUSES]),
        ),
      )
      .orderBy(desc(reportRuns.createdAt))
      .limit(1);
    if (live) return { kind: "building", runId: live.id } as const;

    const [latest] = await tx
      .select({ createdAt: reports.createdAt })
      .from(reports)
      .where(and(eq(reports.accountId, accountId), eq(reports.timeClass, timeClass)))
      .orderBy(desc(reports.createdAt))
      .limit(1);
    if (latest) {
      const availableAt = new Date(latest.createdAt.getTime() + intervalDays * DAY_MS);
      if (availableAt > now) return { kind: "interval", availableAt } as const;
    }

    const [run] = await tx
      .insert(reportRuns)
      .values({
        accountId,
        timeClass,
        trigger: latest ? "manual" : "onboarding",
        plan: AI_PLAN,
        mode: "sync",
        status: "queued",
        promptVersion: REPORT_PROMPT_VERSION,
        isFirstReport: !latest,
      })
      .returning({ id: reportRuns.id });
    if (!run) throw new Error("requestReport: no run inserted");
    await enqueueJob(tx, {
      kind: JOB.report.kind,
      payload: JOB.report.payload(run.id),
      dedupeKey: JOB.report.key(run.id),
      userKey: accountId,
      priority: JOB.report.priority,
      maxAttempts: JOB.report.maxAttempts,
    });
    return { kind: "queued", runId: run.id } as const;
  });
}

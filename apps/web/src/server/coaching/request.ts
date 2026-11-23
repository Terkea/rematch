import type { Db } from "@ci/db";
import { reportRuns, reports } from "@ci/db/generated";
import { and, desc, eq, inArray, sql } from "drizzle-orm";
import { getEnv } from "../../env";
import { isVerifiedLink } from "../accounts/verified";
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
const MINUTE_MS = 60_000;
/** After a failed run the next request waits 10 min, doubling per failure in a row, up to 6 h. */
export const FAILURE_BACKOFF = { firstMs: 10 * MINUTE_MS, capMs: 6 * 60 * MINUTE_MS } as const;
/** At most this many runs per scope in any 24 hours, whatever their outcome. */
export const MAX_RUNS_PER_DAY = 5;
const BACKOFF_LOOKBACK = 12;

/**
 * The manual-report interval of the beta AI plan (ruling G1): one day divided by
 * `AI_BETA_MANUAL_REPORTS_PER_DAY` (default 1). The route and every `getReport` caller pass it.
 */
export function betaReportIntervalDays(): number {
  return 1 / getEnv().AI_BETA_MANUAL_REPORTS_PER_DAY;
}

export type ReportRequest =
  | { kind: "queued"; runId: string }
  | { kind: "building"; runId: string }
  | { kind: "interval"; availableAt: Date }
  | { kind: "throttled"; availableAt: Date }
  | { kind: "not_found" }
  | { kind: "not_verified" };

/**
 * "Build my report" / "Rebuild report" (ruling R4, ADR 0069). In ONE transaction, serialised per
 * scope by `pg_advisory_xact_lock(30030, hashtext(account_id || ':' || time_class))`:
 * - a live run (`queued`, `running`, `awaiting_retry`, `awaiting_batch`) is returned as is;
 * - at most `MAX_RUNS_PER_DAY` runs per scope in 24 hours, and after failed runs in a row a
 *   doubling backoff (`FAILURE_BACKOFF`), else `throttled` with the time it lifts;
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
    // Holistic F10: only a verified link spends the account's AI budget on a report.
    if (!(await isVerifiedLink(tx, userId, target.accountId))) {
      return { kind: "not_verified" } as const;
    }
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

    const recent = await tx
      .select({
        status: reportRuns.status,
        createdAt: reportRuns.createdAt,
        finishedAt: reportRuns.finishedAt,
      })
      .from(reportRuns)
      .where(and(eq(reportRuns.accountId, accountId), eq(reportRuns.timeClass, timeClass)))
      .orderBy(desc(reportRuns.createdAt))
      .limit(Math.max(BACKOFF_LOOKBACK, MAX_RUNS_PER_DAY));
    const throttledUntil = throttle(recent, now);
    if (throttledUntil) return { kind: "throttled", availableAt: throttledUntil } as const;

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

type RecentRun = { status: string; createdAt: Date; finishedAt: Date | null };

/** When the scope may request again, or null when it may now. `recent` is newest first. */
function throttle(recent: RecentRun[], now: Date): Date | null {
  let until: Date | null = null;
  const later = (d: Date) => {
    if (d > now && (until === null || d > until)) until = d;
  };
  const cap = recent[MAX_RUNS_PER_DAY - 1];
  if (cap) later(new Date(cap.createdAt.getTime() + DAY_MS));
  let failures = 0;
  while (failures < recent.length && recent[failures]?.status === "failed") failures++;
  const newest = recent[0];
  if (failures > 0 && newest) {
    const delay = Math.min(FAILURE_BACKOFF.firstMs * 2 ** (failures - 1), FAILURE_BACKOFF.capMs);
    later(new Date((newest.finishedAt ?? newest.createdAt).getTime() + delay));
  }
  return until;
}

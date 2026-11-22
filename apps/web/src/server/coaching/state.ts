import type { ReportRow, ReportRunRow, ReportRunStatus, SectionKey } from "./types";

/** `report_runs.last_error` of a failed run (Rust `report_job.rs`, `reconcile.rs`). */
export type ReportFailure =
  | "plan_interval"
  | "no_input"
  | "invalid_input"
  | "ai_unavailable"
  | "ai_auth"
  | "ai_error"
  | "budget"
  | "job_dead"
  | "reconcile_cap"
  | "unknown";

const FAILURES: ReadonlySet<string> = new Set<ReportFailure>([
  "plan_interval",
  "no_input",
  "invalid_input",
  "ai_unavailable",
  "ai_auth",
  "ai_error",
  "budget",
  "job_dead",
  "reconcile_cap",
]);

export type ReportState =
  | { kind: "none" }
  | {
      kind: "building";
      runId: string;
      since: Date;
      retryAt: Date | null;
      batch: boolean;
      ready: SectionKey[];
    }
  | {
      kind: "ready";
      status: "complete" | "partial" | "computed_only";
      aiOff: "no_api_key" | "budget" | null;
    }
  | { kind: "failed"; reason: ReportFailure; at: Date; hasPrevious: boolean };

/** The run statuses a worker is still on (`report_runs_live_idx`). */
export const LIVE_RUN_STATUSES: readonly ReportRunStatus[] = [
  "queued",
  "running",
  "awaiting_retry",
  "awaiting_batch",
];

function aiOff(reason: string | null): "no_api_key" | "budget" | null {
  return reason === "no_api_key" || reason === "budget" ? reason : null;
}

/**
 * The honest state of a scope's report (ADR 0069): `run` is the newest run, `report` the newest
 * report and `ready` the sections of the run's own report already written. A failed run keeps
 * the previous report on the page (`hasPrevious`); an unknown `last_error` is `unknown`.
 */
export function reportState(
  run: ReportRunRow | null,
  report: ReportRow | null,
  ready: SectionKey[],
): ReportState {
  if (run && LIVE_RUN_STATUSES.includes(run.status)) {
    return {
      kind: "building",
      runId: run.id,
      since: run.createdAt,
      retryAt: run.status === "awaiting_retry" ? run.nextAttemptAt : null,
      batch: run.status === "awaiting_batch",
      ready,
    };
  }
  if (run?.status === "failed") {
    const reason = run.lastError && FAILURES.has(run.lastError) ? run.lastError : "unknown";
    return {
      kind: "failed",
      reason: reason as ReportFailure,
      at: run.finishedAt ?? run.createdAt,
      hasPrevious: report !== null,
    };
  }
  if (!report) return { kind: "none" };
  // A done run is the newest report's run; without a run (deleted) nothing was switched off.
  const sameRun = run?.status === "done" && run.id === report.reportRunId;
  return {
    kind: "ready",
    status: report.status,
    aiOff: sameRun ? aiOff(run.aiDisabledReason) : null,
  };
}

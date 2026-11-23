import { describe, expect, it } from "vitest";
import { reportState } from "../../src/server/coaching";
import type { ReportRow, ReportRunRow } from "../../src/server/coaching/types";

const T0 = new Date("2026-11-20T08:00:00.000Z");

function run(over: Partial<ReportRunRow> = {}): ReportRunRow {
  return {
    id: "run-1",
    accountId: "acc",
    timeClass: "rapid",
    trigger: "manual",
    plan: "free",
    mode: "sync",
    status: "queued",
    retryCount: 0,
    lastError: null,
    nextAttemptAt: null,
    startedAt: null,
    heartbeatAt: null,
    finishedAt: null,
    promptVersion: "report@1",
    promptHash: null,
    inputHash: null,
    input: null,
    isFirstReport: false,
    reportId: null,
    aiDisabledReason: null,
    createdAt: T0,
    reconcileCount: 0,
    algorithmId: null,
    ...over,
  };
}

function report(over: Partial<ReportRow> = {}): ReportRow {
  return {
    id: "rep-1",
    accountId: "acc",
    timeClass: "rapid",
    version: 1,
    reportRunId: "run-1",
    schemaVersion: "report-payload-v1",
    computed: {},
    status: "complete",
    aiDisclosure: null,
    createdAt: T0,
    updatedAt: T0,
    ...over,
  };
}

describe("reportState", () => {
  it("none, none is none", () => {
    expect(reportState(null, null, [])).toEqual({ kind: "none" });
  });

  it("queued, no report is building with nothing ready", () => {
    expect(reportState(run(), null, [])).toEqual({
      kind: "building",
      runId: "run-1",
      since: T0,
      retryAt: null,
      batch: false,
      ready: [],
    });
  });

  it("running with overview ready lists it", () => {
    const s = reportState(run({ status: "running" }), report({ status: "partial" }), ["overview"]);
    expect(s).toMatchObject({ kind: "building", ready: ["overview"], batch: false, retryAt: null });
  });

  it("awaiting_retry carries its retry time", () => {
    const at = new Date("2026-11-20T08:05:00.000Z");
    const s = reportState(run({ status: "awaiting_retry", nextAttemptAt: at }), null, []);
    expect(s).toMatchObject({ kind: "building", retryAt: at, batch: false });
  });

  it("awaiting_batch is building in a batch", () => {
    const s = reportState(run({ status: "awaiting_batch" }), null, []);
    expect(s).toMatchObject({ kind: "building", batch: true, retryAt: null });
  });

  it("done with a complete report is ready", () => {
    expect(reportState(run({ status: "done" }), report(), [])).toEqual({
      kind: "ready",
      status: "complete",
      aiOff: null,
    });
  });

  it("done without an API key is computed only", () => {
    const s = reportState(
      run({ status: "done", aiDisabledReason: "no_api_key" }),
      report({ status: "computed_only" }),
      [],
    );
    expect(s).toEqual({ kind: "ready", status: "computed_only", aiOff: "no_api_key" });
  });

  it("done on budget says so", () => {
    const s = reportState(
      run({ status: "done", aiDisabledReason: "budget" }),
      report({ status: "partial" }),
      [],
    );
    expect(s).toEqual({ kind: "ready", status: "partial", aiOff: "budget" });
  });

  it("failed on the plan interval keeps the previous report", () => {
    const at = new Date("2026-11-20T08:01:00.000Z");
    const s = reportState(
      run({ status: "failed", lastError: "plan_interval", finishedAt: at }),
      report(),
      [],
    );
    expect(s).toEqual({ kind: "failed", reason: "plan_interval", at, hasPrevious: true });
  });

  it("failed at the reconcile cap with no report", () => {
    const s = reportState(run({ status: "failed", lastError: "reconcile_cap" }), null, []);
    expect(s).toEqual({ kind: "failed", reason: "reconcile_cap", at: T0, hasPrevious: false });
  });

  it("an unknown error is unknown", () => {
    const s = reportState(run({ status: "failed", lastError: "something_new" }), null, []);
    expect(s).toMatchObject({ kind: "failed", reason: "unknown" });
  });

  it("a report without a run is ready", () => {
    expect(reportState(null, report({ status: "partial" }), [])).toEqual({
      kind: "ready",
      status: "partial",
      aiOff: null,
    });
  });
});

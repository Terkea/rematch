import { jobs, reportRuns } from "@ci/db/generated";
import { eq } from "drizzle-orm";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { resetCoachingTables, seedReport } from "../helpers/coaching";
import { resetGameTables, seedAccount } from "../helpers/games";
import { createTestSession, resetAuthTables, webTestDb } from "../helpers/session";

// `failEnqueue` makes the enqueue throw after the run row is inserted.
const control = vi.hoisted(() => ({ failEnqueue: false }));
vi.mock("../../src/server/jobs", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../../src/server/jobs")>();
  return {
    ...actual,
    enqueueJob: async (...args: Parameters<typeof actual.enqueueJob>) => {
      if (control.failEnqueue) throw new Error("forced enqueue failure");
      return actual.enqueueJob(...args);
    },
  };
});

const { requestReport } = await import("../../src/server/coaching");

const db = webTestDb();
const DAY = 86_400_000;
let alice: string;
let bob: string;
let accountId: string;

beforeEach(async () => {
  control.failEnqueue = false;
  await resetAuthTables(db);
  await resetGameTables(db);
  await resetCoachingTables(db);
  alice = (await createTestSession(db, { email: "alice@example.invalid" })).user.id;
  bob = (await createTestSession(db, { email: "bob@example.invalid" })).user.id;
  ({ accountId } = await seedAccount(db, { userId: alice, username: "alice-request" }));
});

const target = () => ({ accountId, timeClass: "rapid" as const });

describe("requestReport", () => {
  it("first request is onboarding", async () => {
    const res = await requestReport(db, alice, target());
    expect(res.kind).toBe("queued");
    if (res.kind !== "queued") return;
    const [run] = await db.select().from(reportRuns).where(eq(reportRuns.id, res.runId));
    expect(run).toMatchObject({
      accountId,
      timeClass: "rapid",
      trigger: "onboarding",
      isFirstReport: true,
      plan: "free",
      mode: "sync",
      status: "queued",
      promptVersion: "report@1",
    });
    const rows = await db.select().from(jobs);
    expect(rows).toHaveLength(1);
    expect(rows[0]).toMatchObject({
      kind: "generate-report",
      payload: { reportRunId: res.runId },
      dedupeKey: `report:${res.runId}`,
      userKey: accountId,
      priority: 50,
      maxAttempts: 3,
      state: "queued",
    });
  });

  it("later request is manual", async () => {
    const now = new Date("2026-11-22T12:00:00.000Z");
    await seedReport(db, accountId, {
      timeClass: "rapid",
      payload: "example",
      createdAt: new Date(now.getTime() - 31 * DAY),
    });
    const res = await requestReport(db, alice, target(), now);
    expect(res.kind).toBe("queued");
    if (res.kind !== "queued") return;
    const [run] = await db.select().from(reportRuns).where(eq(reportRuns.id, res.runId));
    expect(run).toMatchObject({ trigger: "manual", isFirstReport: false });
  });

  it("concurrent requests create one run", async () => {
    const [a, b] = await Promise.all([
      requestReport(db, alice, target()),
      requestReport(db, alice, target()),
    ]);
    expect([a.kind, b.kind].sort()).toEqual(["building", "queued"]);
    expect("runId" in a && "runId" in b && a.runId === b.runId).toBe(true);
    expect(await db.select().from(reportRuns)).toHaveLength(1);
    expect(await db.select().from(jobs)).toHaveLength(1);
  });

  it("a live run is returned, not duplicated", async () => {
    const { runId } = await seedReport(db, accountId, {
      timeClass: "rapid",
      run: { status: "awaiting_retry" },
    });
    expect(await requestReport(db, alice, target())).toEqual({ kind: "building", runId });
    expect(await db.select().from(jobs)).toHaveLength(0);
  });

  it("a finished run does not block a new one", async () => {
    await seedReport(db, accountId, {
      timeClass: "rapid",
      run: { status: "failed", lastError: "no_input" },
    });
    expect((await requestReport(db, alice, target())).kind).toBe("queued");
  });

  it("manual request inside the free interval is refused", async () => {
    const now = new Date("2026-11-22T12:00:00.000Z");
    const createdAt = new Date(now.getTime() - 10 * DAY);
    await seedReport(db, accountId, { timeClass: "rapid", payload: "example", createdAt });
    const res = await requestReport(db, alice, target(), now);
    expect(res).toEqual({
      kind: "interval",
      availableAt: new Date(createdAt.getTime() + 30 * DAY),
    });
    expect(await db.select().from(reportRuns)).toHaveLength(1);
    expect(await db.select().from(jobs)).toHaveLength(0);
  });

  it("the interval is configurable (the beta plan)", async () => {
    const now = new Date("2026-11-22T12:00:00.000Z");
    await seedReport(db, accountId, {
      timeClass: "rapid",
      payload: "example",
      createdAt: new Date(now.getTime() - 2 * DAY),
    });
    const res = await requestReport(db, alice, target(), now, { intervalDays: 1 });
    expect(res.kind).toBe("queued");
  });

  it("a report of another time class does not count", async () => {
    await seedReport(db, accountId, { timeClass: "blitz", payload: "example" });
    const res = await requestReport(db, alice, target());
    expect(res.kind).toBe("queued");
  });

  it("run and job commit together", async () => {
    control.failEnqueue = true;
    await expect(requestReport(db, alice, target())).rejects.toThrow("forced enqueue failure");
    expect(await db.select().from(reportRuns)).toHaveLength(0);
    expect(await db.select().from(jobs)).toHaveLength(0);
  });

  it("foreign account is not found", async () => {
    expect(await requestReport(db, bob, target())).toEqual({ kind: "not_found" });
    expect(await requestReport(db, alice, { accountId: "nope", timeClass: "rapid" })).toEqual({
      kind: "not_found",
    });
    expect(await db.select().from(reportRuns)).toHaveLength(0);
  });
});

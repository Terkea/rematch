import { beforeEach, describe, expect, it } from "vitest";
import { getReport } from "../../src/server/coaching";
import { resetCoachingTables, seedReport } from "../helpers/coaching";
import { resetGameTables, seedAccount } from "../helpers/games";
import { createTestSession, resetAuthTables, webTestDb } from "../helpers/session";

const db = webTestDb();
const DAY = 86_400_000;
let alice: string;
let bob: string;
let accountId: string;

beforeEach(async () => {
  await resetAuthTables(db);
  await resetGameTables(db);
  await resetCoachingTables(db);
  alice = (await createTestSession(db, { email: "alice@example.invalid" })).user.id;
  bob = (await createTestSession(db, { email: "bob@example.invalid" })).user.id;
  ({ accountId } = await seedAccount(db, { userId: alice, username: "alice-report" }));
});

describe("getReport", () => {
  it("returns the newest report and run of the scope", async () => {
    const now = new Date("2026-11-22T12:00:00.000Z");
    await seedReport(db, accountId, {
      timeClass: "rapid",
      payload: "partial",
      createdAt: new Date(now.getTime() - 40 * DAY),
    });
    const { reportId } = await seedReport(db, accountId, {
      timeClass: "rapid",
      payload: "example",
      createdAt: new Date(now.getTime() - 35 * DAY),
    });
    const got = await getReport(db, alice, { accountId, timeClass: "rapid" }, now);
    expect(got.state).toEqual({ kind: "ready", status: "complete", aiOff: null });
    expect(got.payload?.reportId).toBe(reportId);
    expect(got.nextManualAt).toBeNull();
  });

  it("a building run keeps the previous report and lists its own ready sections", async () => {
    const { reportId } = await seedReport(db, accountId, {
      timeClass: "rapid",
      payload: "partial",
      createdAt: new Date(Date.now() - 40 * DAY),
    });
    const { runId } = await seedReport(db, accountId, {
      timeClass: "rapid",
      run: { status: "running", trigger: "manual" },
    });
    const got = await getReport(db, alice, { accountId, timeClass: "rapid" });
    expect(got.state).toMatchObject({ kind: "building", runId, ready: [] });
    expect(got.payload?.reportId).toBe(reportId);
  });

  it("foreign scope sees nothing", async () => {
    await seedReport(db, accountId, { timeClass: "rapid", payload: "example" });
    const got = await getReport(db, bob, { accountId, timeClass: "rapid" });
    expect(got).toEqual({
      state: { kind: "none" },
      payload: null,
      createdAt: null,
      nextManualAt: null,
    });
    const malformed = await getReport(db, alice, { accountId: "not-a-uuid", timeClass: "rapid" });
    expect(malformed.payload).toBeNull();
    const otherClass = await getReport(db, alice, { accountId, timeClass: "blitz" });
    expect(otherClass.state).toEqual({ kind: "none" });
  });

  it("next manual date is 30 days after the newest report", async () => {
    const now = new Date("2026-11-22T12:00:00.000Z");
    const createdAt = new Date(now.getTime() - 10 * DAY);
    await seedReport(db, accountId, { timeClass: "rapid", payload: "example", createdAt });
    const got = await getReport(db, alice, { accountId, timeClass: "rapid" }, now);
    expect(got.createdAt).toEqual(createdAt);
    expect(got.nextManualAt).toEqual(new Date(createdAt.getTime() + 30 * DAY));
  });
});

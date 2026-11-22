import { beforeEach, describe, expect, it } from "vitest";
import { listWeeklyReviews } from "../../src/server/coaching";
import { resetCoachingTables, seedWeekly, WEEKLY_CONTENT } from "../helpers/coaching";
import { resetGameTables, seedAccount } from "../helpers/games";
import { createTestSession, resetAuthTables, webTestDb } from "../helpers/session";

const db = webTestDb();
let alice: string;
let bob: string;
let accountId: string;

beforeEach(async () => {
  await resetAuthTables(db);
  await resetGameTables(db);
  await resetCoachingTables(db);
  alice = (await createTestSession(db, { email: "alice@example.invalid" })).user.id;
  bob = (await createTestSession(db, { email: "bob@example.invalid" })).user.id;
  ({ accountId } = await seedAccount(db, { userId: alice, username: "alice-weekly" }));
});

const scope = () => ({ accountId, timeClass: "rapid" as const });

describe("listWeeklyReviews", () => {
  it("ready review carries its content and disclosure", async () => {
    const { id } = await seedWeekly(db, accountId, {
      timeClass: "rapid",
      weekStart: "2026-11-09",
      dropped: 2,
    });
    const [review] = await listWeeklyReviews(db, alice, scope());
    expect(review).toMatchObject({
      id,
      weekStart: "2026-11-09",
      status: "ready",
      content: WEEKLY_CONTENT,
      droppedCount: 2,
    });
    expect(review?.aiDisclosure?.label).toBe("Written by AI");
  });

  it("pending review has no content", async () => {
    await seedWeekly(db, accountId, {
      timeClass: "rapid",
      weekStart: "2026-11-16",
      status: "pending",
    });
    await seedWeekly(db, accountId, { timeClass: "rapid", weekStart: "2026-11-09" });
    await seedWeekly(db, accountId, { timeClass: "blitz", weekStart: "2026-11-02" });
    const list = await listWeeklyReviews(db, alice, scope());
    expect(list.map((r) => r.weekStart)).toEqual(["2026-11-16", "2026-11-09"]);
    expect(list[0]).toMatchObject({ status: "pending", content: null, aiDisclosure: null });
    expect(await listWeeklyReviews(db, alice, scope(), 1)).toHaveLength(1);
  });

  it("foreign scope sees nothing", async () => {
    await seedWeekly(db, accountId, { timeClass: "rapid", weekStart: "2026-11-09" });
    expect(await listWeeklyReviews(db, bob, scope())).toEqual([]);
  });
});

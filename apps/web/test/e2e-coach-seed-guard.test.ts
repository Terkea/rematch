import { accountLinks, games, linkedAccounts } from "@ci/db";
import {
  coachConversations,
  kpiSnapshot,
  reportRuns,
  reports,
  weaknessEstimates,
  weeklyReviews,
} from "@ci/db/generated";
import { eq, sql } from "drizzle-orm";
import { beforeEach, describe, expect, it } from "vitest";
import { seedE2eCoach } from "../e2e/helpers/seed-coach";
import { E2E_USER } from "../e2e/helpers/seed-games";
import { resetCoachTables } from "./helpers/coach";
import { resetCoachingTables } from "./helpers/coaching";
import { resetGameTables } from "./helpers/games";
import { createTestSession, resetAuthTables, webTestDb } from "./helpers/session";

// The coach e2e seed (M5b Task 11) refuses a database that holds a real player's account, with
// M5a's message, and writes nothing then. The refused username is the owner's public test
// profile name only as a string: no owner data is in the repo.

const db = webTestDb();

type Table =
  | typeof games
  | typeof linkedAccounts
  | typeof accountLinks
  | typeof weaknessEstimates
  | typeof kpiSnapshot
  | typeof reports
  | typeof reportRuns
  | typeof weeklyReviews
  | typeof coachConversations;

async function count(table: Table) {
  const [row] = await db.select({ n: sql<number>`count(*)::int` }).from(table);
  return row?.n ?? 0;
}

beforeEach(async () => {
  await resetAuthTables(db);
  await resetGameTables(db);
  await resetCoachingTables(db);
  await resetCoachTables(db);
  await createTestSession(db, { email: E2E_USER });
});

describe("seedE2eCoach", () => {
  it("refuses, writing nothing, when a real account is linked", async () => {
    await db
      .insert(linkedAccounts)
      .values({ platform: "chesscom", username: "mtsgambit", displayName: "mtsgambit" });
    await expect(seedE2eCoach({ db })).rejects.toThrow(
      "Refusing to seed: this database holds real accounts",
    );
    for (const t of [games, accountLinks, weaknessEstimates, kpiSnapshot, reports, reportRuns]) {
      expect(await count(t)).toBe(0);
    }
    expect(await count(weeklyReviews)).toBe(0);
    expect(await count(coachConversations)).toBe(0);
    expect(await count(linkedAccounts)).toBe(1);
  });

  it("seeds the coach profile, two report versions, the reviews and a thread", async () => {
    const seeded = await seedE2eCoach({ db });
    const [account] = await db
      .select({ id: linkedAccounts.id })
      .from(linkedAccounts)
      .where(eq(linkedAccounts.username, "e2e-fixtures"));
    expect(seeded.accountId).toBe(account?.id);

    const states = await db
      .select({ key: weaknessEstimates.seriesKey, state: weaknessEstimates.state })
      .from(weaknessEstimates);
    expect(states).toHaveLength(8);
    expect(new Set(states.map((s) => s.state)).size).toBeGreaterThanOrEqual(5);

    const weeks = await db
      .select({ key: kpiSnapshot.windowKey, value: kpiSnapshot.value })
      .from(kpiSnapshot)
      .where(eq(kpiSnapshot.metric, "accuracy"));
    const weekly = weeks.filter((w) => /^\d{4}-W\d{2}$/.test(w.key));
    expect(weekly.length).toBeGreaterThanOrEqual(10);
    expect(weekly.filter((w) => w.value === null)).toHaveLength(1);

    const versions = await db
      .select({ v: reports.version, status: reports.status, at: reports.createdAt })
      .from(reports)
      .orderBy(reports.version);
    expect(versions.map((r) => r.v)).toEqual([1, 2]);
    expect(versions[1]?.status).toBe("complete");

    const reviews = await db.select({ status: weeklyReviews.status }).from(weeklyReviews);
    expect(reviews.map((r) => r.status).sort()).toEqual(["pending", "ready"]);
    expect(seeded.threadId).toMatch(/^[0-9a-f-]{36}$/);

    // Seeding again restores the same world (specs and visual projects each seed).
    const again = await seedE2eCoach({ db });
    expect(again.accountId).toBe(seeded.accountId);
    expect(await count(reports)).toBe(2);
    expect(await count(weaknessEstimates)).toBe(8);
    expect(await count(coachConversations)).toBe(2);
  });
});

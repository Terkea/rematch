import { beforeEach, describe, expect, it } from "vitest";
import { loadProgress } from "../../src/app/(app)/app/progress/load.js";
import { resolveCoachScope } from "../../src/server/profile/index.js";
import { resetCoachingTables, seedWeekly } from "../helpers/coaching.js";
import { resetGameTables, seedAccount } from "../helpers/games.js";
import { seedProfile } from "../helpers/profile.js";
import { createTestSession, resetAuthTables, webTestDb } from "../helpers/session.js";

// The progress page's loader (M5b Task 10) against seeded Postgres: every read goes through the
// read models, so another user's scope reads as empty. Synthetic rows only.

const NOW = new Date("2026-11-22T10:00:00Z");

describe("progress page reads through the read models", () => {
  const db = webTestDb();
  let userId: string;

  beforeEach(async () => {
    await resetAuthTables(db);
    await resetGameTables(db);
    await resetCoachingTables(db);
    userId = (await createTestSession(db)).user.id;
  });

  it("loads the scope's KPIs, weaknesses and weekly reviews", async () => {
    const { accountId } = await seedAccount(db, { userId, username: "someplayer" });
    await seedProfile(db, accountId, {
      timeClass: "rapid",
      games: 3,
      estimates: [{ seriesKey: "W1", state: "improving" }],
      history: [
        {
          seriesKey: "W1",
          kind: "transition",
          fromState: "active",
          toState: "improving",
          rateBefore: 0.041,
          rateAfter: 0.026,
        },
      ],
      kpis: [{ metric: "accuracy", windowKey: "2026-W45", value: 61.5, n: 22 }],
    });
    await seedWeekly(db, accountId, { timeClass: "rapid", weekStart: "2026-11-02" });
    const scope = await resolveCoachScope(db, userId, {});
    if (!scope) throw new Error("no scope");
    const data = await loadProgress(db, userId, scope, NOW);
    expect(data.trends.find((t) => t.metric === "accuracy")?.points).toHaveLength(1);
    expect(data.weaknesses.map((w) => w.seriesKey)).toEqual(["W1"]);
    expect(data.weaknesses[0]?.history[0]?.rateBefore).toBe(0.041);
    expect(data.weekly).toHaveLength(1);
  });

  it("another user's scope loads nothing", async () => {
    const { accountId } = await seedAccount(db, { userId, username: "someplayer" });
    await seedProfile(db, accountId, {
      timeClass: "rapid",
      games: 2,
      estimates: [{ seriesKey: "W1" }],
    });
    await seedWeekly(db, accountId, { timeClass: "rapid", weekStart: "2026-11-02" });
    const scope = await resolveCoachScope(db, userId, {});
    if (!scope) throw new Error("no scope");
    const stranger = (await createTestSession(db, { email: "stranger@example.test" })).user.id;
    const data = await loadProgress(db, stranger, scope, NOW);
    expect(data.trends.every((t) => t.points.length === 0)).toBe(true);
    expect(data.weaknesses).toEqual([]);
    expect(data.weekly).toEqual([]);
    expect(data.reportAt).toBeNull();
  });
});

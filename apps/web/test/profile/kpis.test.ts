import { beforeAll, beforeEach, describe, expect, it } from "vitest";
import { type CoachScope, kpiTrends, resolveCoachScope } from "../../src/server/profile";
import { resetGameTables, seedAccount } from "../helpers/games";
import { seedProfile } from "../helpers/profile";
import { createTestSession, resetAuthTables, webTestDb } from "../helpers/session";

const db = webTestDb();
let userId: string;
let accountId: string;

async function scopeOf(uid: string): Promise<CoachScope> {
  const scope = await resolveCoachScope(db, uid, {});
  if (!scope) throw new Error("no scope");
  return scope;
}

const t = (iso: string) => new Date(iso);

beforeAll(async () => {
  await resetAuthTables(db);
});

beforeEach(async () => {
  await resetAuthTables(db);
  await resetGameTables(db);
  userId = (await createTestSession(db)).user.id;
  accountId = (await seedAccount(db, { userId, username: "Hero" })).accountId;
});

describe("kpiTrends", () => {
  it("null kpi value stays null", async () => {
    await seedProfile(db, accountId, {
      timeClass: "rapid",
      games: 1,
      kpis: [
        { metric: "accuracy", windowKey: "2026-W45", value: null, n: 0 },
        { metric: "accuracy", windowKey: "last50", value: null, n: 4 },
      ],
    });
    const [trend] = await kpiTrends(db, userId, await scopeOf(userId), ["accuracy"]);
    expect(trend?.points).toEqual([{ week: "2026-W45", value: null, low: null, high: null, n: 0 }]);
    expect(trend?.current).toMatchObject({
      value: null,
      low: null,
      high: null,
      n: 4,
      minN: 20,
      sufficient: false,
      needMore: 16,
    });
  });

  it("newest snapshot per week wins", async () => {
    await seedProfile(db, accountId, {
      timeClass: "rapid",
      games: 1,
      kpis: [
        {
          metric: "accuracy",
          windowKey: "2026-W45",
          value: 80,
          n: 9,
          computedAt: t("2026-11-10T00:00:00Z"),
        },
        {
          metric: "accuracy",
          windowKey: "2026-W45",
          value: 70,
          ciLow: 66,
          ciHigh: 73,
          n: 10,
          computedAt: t("2026-11-12T00:00:00Z"),
        },
        {
          metric: "accuracy",
          windowKey: "2026-W45",
          value: 60,
          n: 8,
          computedAt: t("2026-11-11T00:00:00Z"),
        },
      ],
    });
    const [trend] = await kpiTrends(db, userId, await scopeOf(userId), ["accuracy"]);
    expect(trend?.points).toEqual([{ week: "2026-W45", value: 70, low: 66, high: 73, n: 10 }]);
    expect(trend?.current).toBeNull();
  });

  it("weeks are oldest first", async () => {
    await seedProfile(db, accountId, {
      timeClass: "rapid",
      games: 1,
      kpis: [
        { metric: "accuracy", windowKey: "2026-W45", value: 1, n: 1 },
        { metric: "accuracy", windowKey: "2025-W52", value: 2, n: 1 },
        { metric: "accuracy", windowKey: "2026-W02", value: 3, n: 1 },
        { metric: "blunders_per_game", windowKey: "2026-W01", value: 0.5, n: 1 },
      ],
    });
    const trends = await kpiTrends(db, userId, await scopeOf(userId), [
      "blunders_per_game",
      "accuracy",
    ]);
    expect(trends.map((tr) => tr.metric)).toEqual(["blunders_per_game", "accuracy"]);
    expect(trends[1]?.points.map((p) => p.week)).toEqual(["2025-W52", "2026-W02", "2026-W45"]);
    expect(trends[1]?.label).toBe("Accuracy");
    expect(trends[1]?.unit).toBe("pct");
  });

  it("current is the newest last50 row", async () => {
    await seedProfile(db, accountId, {
      timeClass: "rapid",
      games: 1,
      kpis: [
        { metric: "accuracy", value: 60, n: 50, computedAt: t("2026-11-01T00:00:00Z") },
        {
          metric: "accuracy",
          value: 65,
          ciLow: 63,
          ciHigh: 67,
          n: 50,
          computedAt: t("2026-11-20T00:00:00Z"),
        },
      ],
    });
    const [trend] = await kpiTrends(db, userId, await scopeOf(userId), ["accuracy"]);
    expect(trend?.current).toMatchObject({
      metric: "accuracy",
      value: 65,
      low: 63,
      high: 67,
      n: 50,
      sufficient: true,
      needMore: null,
      computedAt: t("2026-11-20T00:00:00Z"),
    });
    expect(trend?.points).toEqual([]);
  });

  it("defaults to every catalogue metric with a row, and skips unknown ones", async () => {
    await seedProfile(db, accountId, {
      timeClass: "rapid",
      games: 1,
      kpis: [
        { metric: "blunders_per_game", value: 1, n: 30 },
        { metric: "accuracy", value: 70, n: 30 },
        { metric: "area:tactics", value: 50, n: 30 },
      ],
    });
    const scope = await scopeOf(userId);
    expect((await kpiTrends(db, userId, scope)).map((tr) => tr.metric)).toEqual([
      "accuracy",
      "blunders_per_game",
    ]);
    expect(await kpiTrends(db, userId, scope, ["nope"])).toEqual([]);
    const mallory = (await createTestSession(db)).user.id;
    expect(await kpiTrends(db, mallory, scope, ["accuracy"])).toEqual([
      { metric: "accuracy", label: "Accuracy", unit: "pct", current: null, points: [] },
    ]);
  });
});

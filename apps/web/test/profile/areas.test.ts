import { beforeAll, beforeEach, describe, expect, it } from "vitest";
import {
  type CoachScope,
  difficultyStatus,
  getArea,
  listAreaScores,
  resolveCoachScope,
} from "../../src/server/profile";
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

beforeAll(async () => {
  await resetAuthTables(db);
});

beforeEach(async () => {
  await resetAuthTables(db);
  await resetGameTables(db);
  userId = (await createTestSession(db)).user.id;
  accountId = (await seedAccount(db, { userId, username: "Hero" })).accountId;
});

describe("areas", () => {
  it("null area score stays null", async () => {
    await seedProfile(db, accountId, {
      timeClass: "rapid",
      games: 1,
      kpis: [{ metric: "area:endgame", windowKey: "last50", value: null, n: 12 }],
    });
    const view = await getArea(db, userId, await scopeOf(userId), "endgame");
    expect(view.score).toMatchObject({ value: null, n: 12 });
    expect(view.score?.computedAt).toBeInstanceOf(Date);
    const scores = await listAreaScores(db, userId, await scopeOf(userId));
    expect(scores.find((s) => s.area === "endgame")).toEqual({
      area: "endgame",
      value: null,
      n: 12,
    });
  });

  // Bug hunt I1: a lane whose estimate has no benchmark says "not compared yet", not "not an issue".
  it("an uncompared weakness's lane is marked uncompared", async () => {
    await seedProfile(db, accountId, {
      timeClass: "rapid",
      games: 1,
      estimates: [
        { seriesKey: "W8.basics", state: "not_an_issue", bandMedian: null, excess: null },
        { seriesKey: "W8.advanced" },
      ],
      history: [
        { seriesKey: "W8.basics", toState: "not_an_issue", atPlayedAt: new Date("2026-06-01") },
        { seriesKey: "W8.advanced", toState: "active", atPlayedAt: new Date("2026-06-01") },
      ],
    });
    const view = await getArea(db, userId, await scopeOf(userId), "endgame");
    const lane = (id: string) => view.timeline.find((t) => t.id === id);
    expect(lane("W8.basics")?.compared).toBe(false);
    expect(lane("W8.advanced")?.compared).not.toBe(false);
  });

  it("area weaknesses are filtered by area", async () => {
    await seedProfile(db, accountId, {
      timeClass: "rapid",
      games: 1,
      estimates: [{ seriesKey: "W8.basics" }, { seriesKey: "W1" }, { seriesKey: "W8.advanced" }],
      history: [
        { seriesKey: "endgame:rook", toState: "emerging", atPlayedAt: new Date("2026-06-01") },
        { seriesKey: "W1", toState: "active", atPlayedAt: new Date("2026-06-01") },
      ],
      kpis: [
        { metric: "accuracy.endgame", value: 61.5, ciLow: 58, ciHigh: 64, n: 30 },
        { metric: "accuracy.opening", value: 70, n: 30 },
        { metric: "endgame_hold_rate", value: null, n: 0 },
      ],
    });
    const view = await getArea(db, userId, await scopeOf(userId), "endgame");
    expect(view.area).toBe("endgame");
    expect(view.score).toBeNull();
    expect(view.weaknesses.map((w) => w.seriesKey).sort()).toEqual(["W8.advanced", "W8.basics"]);
    expect(view.timeline.map((t) => t.id)).toEqual(["endgame:rook"]);
    expect(view.timeline[0]?.label).toBe("Rook endgames");
    expect(view.kpis.map((k) => k.metric)).toEqual(["accuracy.endgame", "endgame_hold_rate"]);
    expect(view.kpis[0]).toMatchObject({
      label: "Endgame accuracy",
      unit: "pct",
      value: 61.5,
      low: 58,
      high: 64,
      n: 30,
      minN: 20,
      sufficient: true,
      needMore: null,
    });
    expect(view.kpis[1]).toMatchObject({
      value: null,
      low: null,
      high: null,
      n: 0,
      minN: 15,
      sufficient: false,
      needMore: 15,
    });
  });

  it("listAreaScores returns all seven in order", async () => {
    await seedProfile(db, accountId, {
      timeClass: "rapid",
      games: 1,
      kpis: [
        { metric: "area:tactics", value: 41, n: 50 },
        { metric: "area:tactics", windowKey: "2026-W45", value: 99, n: 5 },
      ],
    });
    expect(await listAreaScores(db, userId, await scopeOf(userId))).toEqual([
      { area: "openings", value: null, n: 0 },
      { area: "middlegame", value: null, n: 0 },
      { area: "endgame", value: null, n: 0 },
      { area: "tactics", value: 41, n: 50 },
      { area: "calculation", value: null, n: 0 },
      { area: "time", value: null, n: 0 },
      { area: "mindset", value: null, n: 0 },
    ]);
  });

  it("another user's scope reads nothing", async () => {
    await seedProfile(db, accountId, {
      timeClass: "rapid",
      games: 1,
      estimates: [{ seriesKey: "W8.basics" }],
      kpis: [{ metric: "area:endgame", value: 50, n: 50 }],
      difficulty: ["ok", "pending_maia"],
    });
    const scope = await scopeOf(userId);
    const mallory = (await createTestSession(db)).user.id;
    const view = await getArea(db, mallory, scope, "endgame");
    expect(view).toEqual({ area: "endgame", score: null, kpis: [], weaknesses: [], timeline: [] });
    expect((await listAreaScores(db, mallory, scope)).every((s) => s.n === 0)).toBe(true);
    expect(await difficultyStatus(db, mallory, scope)).toEqual({
      positions: 0,
      pendingMaia: 0,
      ok: 0,
      notApplicable: 0,
    });
  });

  it("counts pending_maia positions", async () => {
    await seedProfile(db, accountId, {
      timeClass: "rapid",
      games: 1,
      difficulty: ["ok", "pending_maia", "ok", "pending_maia", "not_applicable", "pending_maia"],
    });
    // Another time class's positions are outside the scope.
    await seedProfile(db, accountId, { timeClass: "blitz", difficulty: ["pending_maia"] });
    expect(await difficultyStatus(db, userId, await scopeOf(userId))).toEqual({
      positions: 6,
      pendingMaia: 3,
      ok: 2,
      notApplicable: 1,
    });
  });
});

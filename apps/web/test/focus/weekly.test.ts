import { beforeAll, beforeEach, describe, expect, it } from "vitest";
import { listWeeklyReviews } from "../../src/server/coaching";
import { focusWeekLines, setFocus } from "../../src/server/focus";
import { resetCoachingTables, seedWeekly } from "../helpers/coaching";
import { DAY, play, START } from "../helpers/focus";
import { resetGameTables, seedAccount } from "../helpers/games";
import { seedProfile } from "../helpers/profile";
import { createTestSession, resetAuthTables, webTestDb } from "../helpers/session";

const db = webTestDb();
let userId: string;
let accountId: string;

beforeAll(async () => {
  await resetAuthTables(db);
});

beforeEach(async () => {
  await resetAuthTables(db);
  await resetGameTables(db);
  await resetCoachingTables(db);
  userId = (await createTestSession(db)).user.id;
  accountId = (await seedAccount(db, { userId, username: "Hero" })).accountId;
  await seedProfile(db, accountId, {
    timeClass: "rapid",
    estimates: [{ seriesKey: "W5", mean: 0.2, kUnweighted: 18, nUnweighted: 90 }],
  });
});

describe("focusWeekLines", () => {
  it("gives each reviewed week the focus's own counts in that week", async () => {
    // START is Thursday 1 October 2026: its ISO week starts on Monday 28 September.
    await setFocus(db, userId, { accountId, timeClass: "rapid", seriesKey: "W5" }, START);
    await play(db, accountId, 1, { W5: [1, 4] });
    await play(db, accountId, 2, { W5: [0, 2] });
    await play(db, accountId, 4, { W5: [2, 5] }); // Monday 5 October
    const target = { accountId, timeClass: "rapid" as const };
    const lines = await focusWeekLines(db, userId, target, [
      "2026-10-05",
      "2026-09-28",
      "2026-09-21",
    ]);
    expect(lines.get("2026-09-28")).toMatchObject({
      seriesKey: "W5",
      events: 1,
      chances: 6,
      games: 2,
      targetRate: 0.15,
    });
    expect(lines.get("2026-10-05")).toMatchObject({ events: 2, chances: 5, games: 1 });
    expect(lines.get("2026-10-05")?.interval?.level).toBe(0.8);
    // Before the focus started there is no line.
    expect(lines.has("2026-09-21")).toBe(false);
    expect(new Date(START.getTime() + 4 * DAY).getUTCDay()).toBe(1);
  });

  it("is empty for another user", async () => {
    await setFocus(db, userId, { accountId, timeClass: "rapid", seriesKey: "W5" }, START);
    await play(db, accountId, 1, { W5: [1, 4] });
    const other = (await createTestSession(db)).user.id;
    const lines = await focusWeekLines(db, other, { accountId, timeClass: "rapid" }, [
      "2026-09-28",
    ]);
    expect(lines.size).toBe(0);
  });

  it("rides on each weekly review, computed from the games", async () => {
    await setFocus(db, userId, { accountId, timeClass: "rapid", seriesKey: "W5" }, START);
    await play(db, accountId, 1, { W5: [1, 4] });
    await seedWeekly(db, accountId, { timeClass: "rapid", weekStart: "2026-09-28" });
    await seedWeekly(db, accountId, { timeClass: "rapid", weekStart: "2026-09-21" });
    const [late, early] = await listWeeklyReviews(db, userId, { accountId, timeClass: "rapid" });
    expect(late?.focus).toMatchObject({ seriesKey: "W5", events: 1, chances: 4, games: 1 });
    expect(early?.focus).toBeNull();
  });
});

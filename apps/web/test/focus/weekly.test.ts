import { sql } from "drizzle-orm";
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

  it("gives each week its own focus when the focus changed, with one query per kind", async () => {
    await setFocus(db, userId, { accountId, timeClass: "rapid", seriesKey: "W5" }, START);
    await play(db, accountId, 1, { W5: [1, 4] });
    await play(db, accountId, 2, { W5: [0, 2] });
    await play(db, accountId, 4, { W5: [2, 5] }); // Monday 5 October, the first focus's
    // A new focus on Tuesday 6 October, over 5 games.
    await setFocus(
      db,
      userId,
      { accountId, timeClass: "rapid", seriesKey: "W5", gamesN: 5 },
      new Date(START.getTime() + 5 * DAY),
    );
    await play(db, accountId, 6, { W5: [1, 3] });
    await play(db, accountId, 7, { W5: [0, 3] });
    for (let d = 15; d <= 21; d++) await play(db, accountId, d, { W5: [1, 1] });
    const target = { accountId, timeClass: "rapid" as const };
    const lines = await focusWeekLines(db, userId, target, [
      "2026-10-19",
      "2026-10-12",
      "2026-10-05",
      "2026-09-28",
      "not-a-date",
    ]);
    expect(lines.get("2026-09-28")).toMatchObject({ events: 1, chances: 6, games: 2 });
    // The week of 5 October is the second focus's: only its own games count.
    expect(lines.get("2026-10-05")).toMatchObject({ events: 1, chances: 6, games: 2 });
    // Its N of 5 games ends on 18 October: three of them in the week of 12 October.
    expect(lines.get("2026-10-12")).toMatchObject({ events: 3, chances: 3, games: 3 });
    expect(lines.has("2026-10-19")).toBe(false);
    expect(lines.size).toBe(3);
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

  it("counts no practice in a week without trainer attempts", async () => {
    await setFocus(db, userId, { accountId, timeClass: "rapid", seriesKey: "W5" }, START);
    await play(db, accountId, 1, { W5: [1, 4] });
    const lines = await focusWeekLines(db, userId, { accountId, timeClass: "rapid" }, [
      "2026-09-28",
    ]);
    expect(lines.get("2026-09-28")?.practiceSessions ?? 0).toBe(0);
  });
});

// The trainer's real tables (migration 20261123040000_drill_items, ADR 0090), now merged: this
// test used to create and drop stand-ins, which dropped the real tables under the other suites.
describe("focusWeekLines with the trainer's attempts", () => {
  it("counts the days the player practised in the week as sessions", async () => {
    await setFocus(db, userId, { accountId, timeClass: "rapid", seriesKey: "W5" }, START);
    await play(db, accountId, 1, { W5: [1, 4] });
    const [game] = await db.execute<{ id: string }>(
      sql`select id from games where account_id = ${accountId} limit 1`,
    );
    const [item] = await db.execute<{ id: string }>(
      sql`insert into drill_items (user_id, account_id, time_class, game_id, ply, source_key,
          source_keys, fen, played_uci, solution_uci, solution_line, accepted_uci, win_before,
          win_after, tags, held_out)
        values (${userId}, ${accountId}, 'rapid', ${game?.id}, 1, 'blunder', array['blunder'],
          'rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1', 'e2e4', 'd2d4',
          array['d2d4'], array['d2d4'], 50, 30, array[]::text[], false)
        returning id`,
    );
    const id = item?.id;
    const at = (days: number) => new Date(START.getTime() + days * DAY).toISOString();
    for (const d of [0, 0.1, 2, 9]) {
      await db.execute(
        sql`insert into drill_attempts (item_id, user_id, at, correct, confidence, move_uci)
          values (${id}, ${userId}, ${at(d)}, true, 'sure', 'd2d4')`,
      );
    }
    const lines = await focusWeekLines(db, userId, { accountId, timeClass: "rapid" }, [
      "2026-09-28",
    ]);
    // Thursday (twice) and Saturday: 2 days. Day 9 is the next week.
    expect(lines.get("2026-09-28")?.practiceSessions).toBe(2);
  });
});

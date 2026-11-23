import { focuses } from "@ci/db/generated";
import { eq } from "drizzle-orm";
import { beforeAll, beforeEach, describe, expect, it } from "vitest";
import { ALGORITHM_ID } from "../../src/server/algorithm";
import { clearFocus, getFocusCard, setFocus, setFocusLength } from "../../src/server/focus";
import { resolveCoachScope } from "../../src/server/profile";
import { DAY, play, START } from "../helpers/focus";
import { resetGameTables, seedAccount } from "../helpers/games";
import { seedProfile } from "../helpers/profile";
import { createTestSession, resetAuthTables, webTestDb } from "../helpers/session";

const db = webTestDb();
let userId: string;
let accountId: string;

/** A profile whose top costing weakness is W5 (20% now, peers 10%), then W4.allowed. */
async function profile(account: string) {
  await seedProfile(db, account, {
    timeClass: "rapid",
    games: 2,
    estimates: [
      {
        seriesKey: "W5",
        priority: 5,
        state: "active",
        mean: 0.2,
        ciLow: 0.15,
        ciHigh: 0.25,
        kUnweighted: 18,
        nUnweighted: 90,
        bandMedian: 0.1,
        confidence: 0.95,
      },
      {
        seriesKey: "W4.allowed",
        priority: 2,
        state: "active",
        mean: 0.08,
        ciLow: 0.06,
        ciHigh: 0.1,
        kUnweighted: 128,
        nUnweighted: 1600,
        bandMedian: 0.07,
        confidence: 0.81,
      },
    ],
  });
}

const scopeOf = async (u: string) => {
  const s = await resolveCoachScope(db, u, { tc: "rapid" });
  if (!s) throw new Error("no scope");
  return s;
};

beforeAll(async () => {
  await resetAuthTables(db);
});

beforeEach(async () => {
  await resetAuthTables(db);
  await resetGameTables(db);
  userId = (await createTestSession(db)).user.id;
  accountId = (await seedAccount(db, { userId, username: "Hero" })).accountId;
  await profile(accountId);
});

describe("the suggestion", () => {
  it("is the top costing weakness, with a target and N = 20", async () => {
    const card = await getFocusCard(db, userId, await scopeOf(userId));
    expect(card.kind).toBe("suggest");
    if (card.kind !== "suggest") return;
    expect(card.suggestion).toMatchObject({
      seriesKey: "W5",
      baseline: { rate: 0.2, events: 18, chances: 90 },
      // ADR 0101: W5 is compared with the evaluation, so its median is no peer target.
      peer: null,
      target: { rate: 0.15, basis: "relative" },
      gamesN: 20,
    });
    expect(card.suggestion.label.length).toBeGreaterThan(0);
  });
});

describe("setFocus and its progress", () => {
  it("counts only the focus series in games after it started, with n and an interval", async () => {
    await play(db, accountId, -1, { W5: [5, 5] }); // before the start: not counted
    const set = await setFocus(
      db,
      userId,
      { accountId, timeClass: "rapid", seriesKey: "W5" },
      START,
    );
    expect(set.kind).toBe("set");
    // ADR 0105: the focus names the algorithm that chose its target.
    const [stored] = await db.select().from(focuses).where(eq(focuses.userId, userId));
    expect(stored?.algorithmId).toBe(ALGORITHM_ID);
    await play(db, accountId, 1, { W5: [1, 6], W4: [3, 3] });
    await play(db, accountId, 2, { W5: [0, 6] });
    await play(db, accountId, 3, {}); // a game with no chance still counts as a game
    const card = await getFocusCard(db, userId, await scopeOf(userId));
    expect(card.kind).toBe("active");
    if (card.kind !== "active") return;
    expect(card.focus).toMatchObject({ seriesKey: "W5", targetRate: 0.15, gamesN: 20 });
    expect(card.progress).toMatchObject({
      events: 1,
      chances: 12,
      games: { played: 3, of: 20 },
      verdict: { kind: "on_track" },
      done: false,
    });
    expect(card.progress.interval?.level).toBe(0.8);
  });

  it("counts one colour's games for a colour focus", async () => {
    await seedProfile(db, accountId, {
      timeClass: "rapid",
      estimates: [{ seriesKey: "W5", side: "black", mean: 0.3, kUnweighted: 9, nUnweighted: 30 }],
    });
    const set = await setFocus(
      db,
      userId,
      { accountId, timeClass: "rapid", seriesKey: "W5", side: "black" },
      START,
    );
    expect(set.kind === "set" && set.focus.baseline.rate).toBe(0.3);
    await play(db, accountId, 1, { W5: [1, 4] }, "white");
    await play(db, accountId, 2, { W5: [0, 4] }, "black");
    const card = await getFocusCard(db, userId, await scopeOf(userId));
    expect(card.kind === "active" && card.progress).toMatchObject({
      events: 0,
      chances: 4,
      games: { played: 1 },
    });
  });

  it("refuses a weakness the scope does not have", async () => {
    expect(
      await setFocus(db, userId, { accountId, timeClass: "rapid", seriesKey: "W9" }, START),
    ).toEqual({ kind: "not_found" });
  });

  it("replacing a running focus ends it as cleared", async () => {
    await setFocus(db, userId, { accountId, timeClass: "rapid", seriesKey: "W5" }, START);
    await play(db, accountId, 1, { W5: [1, 2] });
    await setFocus(
      db,
      userId,
      { accountId, timeClass: "rapid", seriesKey: "W4.allowed", gamesN: 30 },
      new Date(START.getTime() + 2 * DAY),
    );
    const rows = await db.select().from(focuses).where(eq(focuses.userId, userId));
    expect(rows).toHaveLength(2);
    const old = rows.find((r) => r.seriesKey === "W5");
    expect(old).toMatchObject({ outcome: "cleared", resultK: 1, resultN: 2, resultGames: 1 });
    expect(old?.endedAt).not.toBeNull();
    const card = await getFocusCard(db, userId, await scopeOf(userId));
    expect(card.kind === "active" && card.focus.seriesKey).toBe("W4.allowed");
    expect(card.kind === "active" && card.focus.gamesN).toBe(30);
  });

  it("clearing the focus brings back the suggestion", async () => {
    await setFocus(db, userId, { accountId, timeClass: "rapid", seriesKey: "W5" }, START);
    expect(await clearFocus(db, userId, "rapid")).toBe(true);
    const card = await getFocusCard(db, userId, await scopeOf(userId));
    expect(card.kind).toBe("suggest");
  });

  it("changes N without restarting", async () => {
    await setFocus(db, userId, { accountId, timeClass: "rapid", seriesKey: "W5" }, START);
    await play(db, accountId, 1, { W5: [0, 4] });
    expect(await setFocusLength(db, userId, "rapid", 10)).toBe(true);
    const card = await getFocusCard(db, userId, await scopeOf(userId));
    expect(card.kind === "active" && card.progress.games).toEqual({ played: 1, of: 10 });
  });
});

describe("closing at N games", () => {
  it("reads as closed with the outcome and the next focus, writing nothing; a POST stores it", async () => {
    await setFocus(
      db,
      userId,
      { accountId, timeClass: "rapid", seriesKey: "W5", gamesN: 5 },
      START,
    );
    for (let i = 1; i <= 6; i++) await play(db, accountId, i, { W5: [i === 1 ? 1 : 0, 3] });
    const card = await getFocusCard(db, userId, await scopeOf(userId));
    expect(card.kind).toBe("closed");
    if (card.kind !== "closed") return;
    // Only the first 5 games count: 1 of 15 chances, under 15%.
    expect(card.progress).toMatchObject({
      events: 1,
      chances: 15,
      games: { played: 5, of: 5 },
      outcome: "met",
    });
    expect(card.next?.seriesKey).toBe("W5");
    // The fifth game's date closes it.
    const fifth = new Date(START.getTime() + 5 * DAY).toISOString();
    expect(card.focus.endedAt?.toISOString()).toBe(fifth);
    expect(card.focus.outcome).toBe("met");
    // A read writes nothing (R2-7): the row is still running.
    const [read] = await db.select().from(focuses).where(eq(focuses.userId, userId));
    expect(read).toMatchObject({ endedAt: null, outcome: null });
    // Read again, it is the same closed focus.
    const again = await getFocusCard(db, userId, await scopeOf(userId));
    expect(again.kind === "closed" && again.progress.outcome).toBe("met");
    // A POST settles it: changing N on a finished focus stores the close instead.
    expect(await setFocusLength(db, userId, "rapid", 30)).toBe(false);
    const [row] = await db.select().from(focuses).where(eq(focuses.userId, userId));
    expect(row).toMatchObject({
      outcome: "met",
      resultK: 1,
      resultN: 15,
      resultGames: 5,
      gamesN: 5,
    });
    expect(row?.endedAt?.toISOString()).toBe(fifth);
    const stored = await getFocusCard(db, userId, await scopeOf(userId));
    expect(stored.kind === "closed" && stored.progress).toMatchObject({
      outcome: "met",
      events: 1,
      chances: 15,
    });
  });

  it("is not met when the rate stays above the target", async () => {
    await setFocus(
      db,
      userId,
      { accountId, timeClass: "rapid", seriesKey: "W5", gamesN: 5 },
      START,
    );
    for (let i = 1; i <= 5; i++) await play(db, accountId, i, { W5: [3, 6] });
    const card = await getFocusCard(db, userId, await scopeOf(userId));
    expect(card.kind === "closed" && card.progress.outcome).toBe("not_met");
  });

  it("a new focus set over a finished one stores the finished one's outcome, not cleared", async () => {
    await setFocus(
      db,
      userId,
      { accountId, timeClass: "rapid", seriesKey: "W5", gamesN: 5 },
      START,
    );
    for (let i = 1; i <= 5; i++) await play(db, accountId, i, { W5: [0, 3] });
    await setFocus(
      db,
      userId,
      { accountId, timeClass: "rapid", seriesKey: "W4.allowed" },
      new Date(START.getTime() + 9 * DAY),
    );
    const rows = await db.select().from(focuses).where(eq(focuses.userId, userId));
    expect(rows.find((r) => r.seriesKey === "W5")).toMatchObject({
      outcome: "met",
      resultGames: 5,
    });
  });

  it("clearing a finished focus stores its outcome", async () => {
    await setFocus(
      db,
      userId,
      { accountId, timeClass: "rapid", seriesKey: "W5", gamesN: 5 },
      START,
    );
    for (let i = 1; i <= 5; i++) await play(db, accountId, i, { W5: [0, 1] });
    expect(await clearFocus(db, userId, "rapid")).toBe(true);
    const [row] = await db.select().from(focuses).where(eq(focuses.userId, userId));
    expect(row?.outcome).toBe("inconclusive");
  });

  it("is inconclusive with too few chances", async () => {
    await setFocus(
      db,
      userId,
      { accountId, timeClass: "rapid", seriesKey: "W5", gamesN: 5 },
      START,
    );
    for (let i = 1; i <= 5; i++) await play(db, accountId, i, { W5: [0, 1] });
    const card = await getFocusCard(db, userId, await scopeOf(userId));
    expect(card.kind === "closed" && card.progress.outcome).toBe("inconclusive");
  });
});

describe("per-user isolation", () => {
  it("never shows or changes another user's focus", async () => {
    await setFocus(db, userId, { accountId, timeClass: "rapid", seriesKey: "W5" }, START);
    const other = (await createTestSession(db)).user.id;
    const otherAccount = (await seedAccount(db, { userId: other, username: "Villain" })).accountId;
    await profile(otherAccount);
    const card = await getFocusCard(db, other, await scopeOf(other));
    expect(card.kind).toBe("suggest");
    // The other user cannot set a focus on this user's account, nor clear this user's focus.
    expect(
      await setFocus(db, other, { accountId, timeClass: "rapid", seriesKey: "W5" }, START),
    ).toEqual({ kind: "not_found" });
    expect(await clearFocus(db, other, "rapid")).toBe(false);
    const mine = await getFocusCard(db, userId, await scopeOf(userId));
    expect(mine.kind).toBe("active");
  });

  it("a scope of another account of the user shows where the focus is", async () => {
    await setFocus(db, userId, { accountId, timeClass: "rapid", seriesKey: "W5" }, START);
    const second = (await seedAccount(db, { userId, username: "Alt", platform: "lichess" }))
      .accountId;
    await profile(second);
    const scope = await resolveCoachScope(db, userId, { tc: "rapid", account: second });
    if (!scope) throw new Error("no scope");
    const card = await getFocusCard(db, userId, scope);
    expect(card.kind).toBe("elsewhere");
    if (card.kind !== "elsewhere") return;
    expect(card.focus).toMatchObject({ accountId, seriesKey: "W5", endedAt: null });
  });
});

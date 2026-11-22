import { beforeAll, beforeEach, describe, expect, it } from "vitest";
import type { TimeClass } from "../../src/server/games/types";
import { getOverview } from "../../src/server/overview";
import {
  resetGameTables,
  type SyntheticGame,
  seedAccount,
  seedSyntheticGame,
} from "../helpers/games";
import { createTestSession, resetAuthTables, webTestDb } from "../helpers/session";

const db = webTestDb();
let userId: string;
let accountId: string;

const day = (i: number) => new Date(Date.UTC(2026, 0, 1 + i, 12)).toISOString();

async function seed(n: number, over: (i: number) => Partial<SyntheticGame> = () => ({})) {
  for (let i = 0; i < n; i++) {
    await seedSyntheticGame(db, accountId, {
      playedAt: day(i),
      timeClass: "rapid",
      result: "1-0",
      accuracy: 60 + (i % 10),
      counts: { blunders: i % 3, mistakes: 1, inaccuracies: 2 },
      ...over(i),
    });
  }
}

beforeAll(async () => {
  await resetAuthTables(db);
});

beforeEach(async () => {
  await resetAuthTables(db);
  await resetGameTables(db);
  const s = await createTestSession(db);
  userId = s.user.id;
  accountId = (await seedAccount(db, { userId, username: "Hero" })).accountId;
});

describe("getOverview", () => {
  it("zero games gives no numbers", async () => {
    const o = await getOverview(db, userId, { timeClass: null });
    expect(o.accounts).toHaveLength(1);
    expect(o.status).toEqual({ total: 0, analysed: 0, waiting: 0, failed: 0, skipped: 0 });
    expect(o.accuracy.points).toEqual([]);
    expect(o.accuracy.overall).toEqual({ kind: "insufficient", n: 0, needed: 10 });
    for (const e of [o.judgements.blunders, o.judgements.mistakes, o.judgements.inaccuracies]) {
      expect(e).toEqual({ kind: "insufficient", n: 0, needed: 10 });
    }
    expect(o.byTimeClass).toEqual([]);
    expect(o.recent).toEqual([]);
  });

  it("accuracy ignores games not done", async () => {
    await seed(12);
    for (const [status, k] of [
      ["pending", 3],
      ["analysing", 1],
      ["failed", 2],
      ["skipped", 1],
    ] as const) {
      for (let i = 0; i < k; i++) {
        await seedSyntheticGame(db, accountId, {
          playedAt: day(100 + i),
          timeClass: "rapid",
          result: "1-0",
          status,
        });
      }
    }
    const o = await getOverview(db, userId, { timeClass: null });
    expect(o.status).toEqual({ total: 19, analysed: 12, waiting: 4, failed: 2, skipped: 1 });
    expect(o.accuracy.overall.kind === "estimate" && o.accuracy.overall.n).toBe(12);
    expect(o.accuracy.points).toHaveLength(12);
  });

  it("results by time class", async () => {
    const results: ("1-0" | "1/2-1/2" | "0-1")[] = [
      ...Array(8).fill("1-0"),
      ...Array(2).fill("1/2-1/2"),
      ...Array(4).fill("0-1"),
    ];
    await seed(14, (i) => ({ result: results[i] as "1-0" }));
    for (let i = 0; i < 3; i++) {
      await seedSyntheticGame(db, accountId, {
        playedAt: day(50 + i),
        timeClass: "blitz",
        result: "1-0",
      });
    }
    const o = await getOverview(db, userId, { timeClass: null });
    expect(o.byTimeClass.map((t) => t.timeClass)).toEqual(["blitz", "rapid"]);
    const rapid = o.byTimeClass.find((t) => t.timeClass === "rapid");
    const blitz = o.byTimeClass.find((t) => t.timeClass === "blitz");
    expect(rapid).toMatchObject({ games: 14, wins: 8, draws: 2, losses: 4 });
    expect(rapid?.score.kind === "estimate" && rapid.score.value).toBeCloseTo(9 / 14, 9);
    expect(blitz?.games).toBe(3);
    expect(blitz?.score).toEqual({ kind: "insufficient", n: 3, needed: 7 });
    expect(o.timeClasses).toEqual([
      { timeClass: "blitz", analysed: 3 },
      { timeClass: "rapid", analysed: 14 },
    ]);
  });

  it("unfinished games are excluded from score n", async () => {
    await seed(12, (i) => ({ result: i < 2 ? "*" : i % 2 ? "0-1" : "1-0" }));
    const o = await getOverview(db, userId, { timeClass: null });
    const rapid = o.byTimeClass[0];
    expect(rapid?.wins).toBe(5);
    expect(rapid?.losses).toBe(5);
    expect(rapid?.score.kind === "estimate" && rapid.score.n).toBe(10);
  });

  it("weekly trend ends at the latest game's week", async () => {
    await seed(15, (i) => ({ playedAt: i === 14 ? "2026-06-10T12:00:00Z" : day(i) }));
    const o = await getOverview(db, userId, { timeClass: null });
    expect(o.judgements.weekly).toHaveLength(12);
    expect(o.judgements.weekly.at(-1)?.week).toBe("2026-W24");
    expect(o.judgements.weekly.at(-1)?.blunders).not.toBeNull();
    expect(o.judgements.weekly.at(-2)?.blunders).toBeNull();
  });

  it("time class filter narrows accuracy and judgements but not status", async () => {
    await seed(11);
    for (let i = 0; i < 4; i++) {
      await seedSyntheticGame(db, accountId, {
        playedAt: day(60 + i),
        timeClass: "blitz" as TimeClass,
        result: "1-0",
      });
    }
    const o = await getOverview(db, userId, { timeClass: "rapid" });
    expect(o.status.analysed).toBe(15);
    expect(o.accuracy.points).toHaveLength(11);
    expect(o.judgements.columns).toHaveLength(11);
    expect(o.recent.every((r) => r.timeClass === "rapid")).toBe(true);
    expect(o.recent).toHaveLength(6);
    expect(o.byTimeClass).toHaveLength(2);
  });

  it("estimates from seeded rows match a hand computation", async () => {
    await seed(10, (i) => ({ accuracy: 50 + i * 2, counts: { blunders: i % 2 } }));
    const o = await getOverview(db, userId, { timeClass: null });
    const acc = o.accuracy.overall;
    expect(acc.kind).toBe("estimate");
    if (acc.kind === "estimate") {
      expect(acc.value).toBeCloseTo(59, 9);
      expect(acc.interval.low).toBeLessThan(59);
      expect(acc.interval.high).toBeGreaterThan(59);
    }
    const bl = o.judgements.blunders;
    expect(bl.kind === "estimate" && bl.value).toBeCloseTo(0.5, 9);
    expect(o.judgements.columns[0]?.blunders).toBe(0);
    expect(o.judgements.columns[1]?.blunders).toBe(1);
  });

  it("game where the player made no analysed move contributes no accuracy", async () => {
    await seed(10);
    await seedSyntheticGame(db, accountId, {
      playedAt: day(90),
      timeClass: "rapid",
      result: "1-0",
      accuracy: null,
      noMoves: true,
    });
    const o = await getOverview(db, userId, { timeClass: null });
    expect(o.status.analysed).toBe(11);
    expect(o.accuracy.points).toHaveLength(10);
    expect(o.judgements.columns).toHaveLength(10);
    expect(o.accuracy.overall.kind === "estimate" && o.accuracy.overall.n).toBe(10);
  });

  it("does not leak another user's games", async () => {
    const other = await createTestSession(db);
    const { accountId: theirs } = await seedAccount(db, {
      userId: other.user.id,
      username: "Rival",
    });
    await seedSyntheticGame(db, theirs, { playedAt: day(1), timeClass: "rapid", result: "1-0" });
    const o = await getOverview(db, userId, { timeClass: null });
    expect(o.status.total).toBe(0);
  });
});

import { gameMetrics } from "@ci/db/generated";
import { beforeAll, describe, expect, it } from "vitest";
import { listFormats } from "../../src/server/filters/formats";
import {
  type GameStatsScope,
  gameStatRows,
  gameStats,
  PHASE_METRICS_VERSION,
} from "../../src/server/stats/game-stats";
import { wilson } from "../../src/server/stats/wilson";
import { resetGameTables, seedAccount, seedSyntheticGame } from "../helpers/games";
import { createTestSession, resetAuthTables, webTestDb } from "../helpers/session";

// Synthetic games only (the seed helpers refuse any database but an empty *_test one).
const db = webTestDb();

let alice: string;
let aliceAccount: string;
const whiteAcc: number[] = [];

const ALL: GameStatsScope = {
  timeClass: "rapid",
  fmt: null,
  color: "both",
  period: { kind: "days", from: null, to: null },
};

async function metrics(gameId: string, playedAt: string, version: number, phases: object) {
  await db.insert(gameMetrics).values({
    gameId,
    accountId: aliceAccount,
    timeClass: "rapid",
    playedAt: new Date(playedAt),
    protocol: "test-1",
    metrics: { version, accuracyByPhase: phases },
  });
}

beforeAll(async () => {
  await resetAuthTables(db);
  await resetGameTables(db);
  alice = (await createTestSession(db)).user.id;
  const bob = (await createTestSession(db)).user.id;
  aliceAccount = (await seedAccount(db, { userId: alice, username: "alice-stats" })).accountId;
  const bobAccount = (await seedAccount(db, { userId: bob, username: "bob-stats" })).accountId;

  // 12 white rapid 10+0 games in January: 7 wins, 2 draws, 3 losses; one blunder each.
  const results = ["1-0", "1-0", "1-0", "1-0", "1-0", "1-0", "1-0", "1/2-1/2", "1/2-1/2"];
  results.push("0-1", "0-1", "0-1");
  for (let i = 0; i < 12; i++) {
    const playedAt = `2026-01-${String(i + 1).padStart(2, "0")}T12:00:00Z`;
    const accuracy = 60 + 2 * i;
    whiteAcc.push(accuracy);
    const { gameId } = await seedSyntheticGame(db, aliceAccount, {
      playedAt,
      timeClass: "rapid",
      result: results[i] as "1-0",
      playerColor: "white",
      accuracy,
      baseSeconds: 600,
      incrementSeconds: 0,
      counts: { blunders: 1, mistakes: i % 2, inaccuracies: 2 },
      opponentCounts: { blunders: 5 },
    });
    // 11 current-version phase rows (one without an endgame), 1 stale row that must not count.
    if (i === 0) await metrics(gameId, playedAt, PHASE_METRICS_VERSION - 1, { opening: 1 });
    else
      await metrics(gameId, playedAt, PHASE_METRICS_VERSION, {
        opening: 70 + i,
        middlegame: 65,
        endgame: i === 1 ? null : 50,
      });
  }
  // 4 black rapid 15+10 games in February (results are White-POV: "1-0" is a Black loss, "0-1"
  // a Black win): 3 losses and 1 win, one of them with no stored accuracy.
  for (let i = 0; i < 4; i++) {
    await seedSyntheticGame(db, aliceAccount, {
      playedAt: `2026-02-0${i + 1}T12:00:00Z`,
      timeClass: "rapid",
      result: i === 3 ? "0-1" : "1-0",
      playerColor: "black",
      accuracy: i === 0 ? null : 80,
      baseSeconds: 900,
      incrementSeconds: 10,
    });
  }
  // A pending game and an unfinished one (in n; no analysis values; no score for "*").
  await seedSyntheticGame(db, aliceAccount, {
    playedAt: "2026-02-10T12:00:00Z",
    timeClass: "rapid",
    result: "1-0",
    status: "pending",
    baseSeconds: 600,
    incrementSeconds: 0,
  });
  await seedSyntheticGame(db, aliceAccount, {
    playedAt: "2026-02-11T12:00:00Z",
    timeClass: "rapid",
    result: "*",
    status: "skipped",
    baseSeconds: 300,
    incrementSeconds: 0,
  });
  // Other classes and other users never count.
  await seedSyntheticGame(db, aliceAccount, {
    playedAt: "2026-01-05T12:00:00Z",
    timeClass: "blitz",
    result: "1-0",
  });
  await seedSyntheticGame(db, bobAccount, {
    playedAt: "2026-01-05T12:00:00Z",
    timeClass: "rapid",
    result: "1-0",
    baseSeconds: 600,
    incrementSeconds: 0,
  });
});

describe("gameStats", () => {
  it("counts n, analysed games and W-D-L with a Wilson score", async () => {
    const s = await gameStats(db, alice, ALL);
    expect(s.n).toBe(18);
    expect(s.analysed).toBe(16);
    // White: 7 W, 2 D, 3 L; black: 1 W, 3 L; pending game: a win; unfinished: no result.
    expect(s.score).toMatchObject({ n: 17, wins: 9, draws: 2, losses: 6 });
    expect(s.score.score).toEqual({
      kind: "estimate",
      value: 10 / 17,
      interval: wilson(10, 17),
      n: 17,
    });
  });

  it("averages accuracy over analysed games only, dropping unknown values", async () => {
    const s = await gameStats(db, alice, ALL);
    // 12 white values and 3 known black ones (the 4th black game has none).
    const values = [...whiteAcc, 80, 80, 80];
    expect(s.accuracy.n).toBe(15);
    expect(s.accuracy.kind).toBe("estimate");
    if (s.accuracy.kind !== "estimate") return;
    expect(s.accuracy.value).toBeCloseTo(values.reduce((a, b) => a + b, 0) / values.length, 9);
    expect(s.accuracy.interval?.low).toBeLessThan(s.accuracy.value);
    expect(s.accuracy.interval?.high).toBeGreaterThan(s.accuracy.value);
  });

  it("counts the player's own judgements per analysed game", async () => {
    const s = await gameStats(db, alice, ALL);
    expect(s.perGame.blunders).toMatchObject({ kind: "estimate", n: 16, value: 12 / 16 });
    expect(s.perGame.mistakes).toMatchObject({ kind: "estimate", n: 16, value: 6 / 16 });
    expect(s.perGame.inaccuracies).toMatchObject({ kind: "estimate", n: 16, value: 24 / 16 });
  });

  it("uses only current-version stored phase accuracies", async () => {
    const s = await gameStats(db, alice, ALL);
    expect(s.phaseAccuracy.opening).toMatchObject({ kind: "estimate", n: 11 });
    if (s.phaseAccuracy.opening.kind === "estimate")
      expect(s.phaseAccuracy.opening.value).toBeCloseTo(76, 9);
    expect(s.phaseAccuracy.middlegame).toMatchObject({ n: 11, value: 65 });
    // One game has no endgame: 10 values, all 50, so no spread and no interval.
    expect(s.phaseAccuracy.endgame).toEqual({ kind: "estimate", value: 50, interval: null, n: 10 });
  });

  it("splits by colour and says how many more games a side needs", async () => {
    const s = await gameStats(db, alice, ALL);
    expect(s.byColor.white.n).toBe(14);
    expect(s.byColor.black.n).toBe(4);
    expect(s.byColor.black.accuracy).toEqual({ kind: "insufficient", n: 3, needed: 7 });
    expect(s.byColor.black.score).toMatchObject({ n: 4, wins: 1, draws: 0, losses: 3 });
    expect(s.byColor.black.score.score).toEqual({ kind: "insufficient", n: 4, needed: 6 });
  });

  it("filters by colour", async () => {
    const s = await gameStats(db, alice, { ...ALL, color: "black" });
    expect(s.n).toBe(4);
    expect(s.byColor.white.n).toBe(0);
    expect(s.byColor.white.accuracy).toEqual({ kind: "insufficient", n: 0, needed: 10 });
  });

  it("filters by format, including Other", async () => {
    expect((await listFormats(db, [aliceAccount], "rapid")).map((o) => [o.key, o.games])).toEqual([
      ["600+0", 13],
      ["900+10", 4],
      ["300+0", 1],
    ]);
    expect((await gameStats(db, alice, { ...ALL, fmt: "600+0" })).n).toBe(13);
    expect((await gameStats(db, alice, { ...ALL, fmt: "900+10" })).n).toBe(4);
    expect((await gameStats(db, alice, { ...ALL, fmt: "60+0" })).n).toBe(0);
    // Three formats are all named, so Other is empty.
    expect((await gameStats(db, alice, { ...ALL, fmt: "other" })).n).toBe(0);
  });

  it("filters by an inclusive day range and by the newest games", async () => {
    const feb = await gameStats(db, alice, {
      ...ALL,
      period: { kind: "days", from: "2026-02-01", to: "2026-02-10" },
    });
    expect(feb.n).toBe(5);
    const newest = await gameStatRows(db, alice, {
      ...ALL,
      period: { kind: "games", skip: 0, take: 3 },
    });
    expect(newest.map((r) => r.playedAt.toISOString().slice(0, 10))).toEqual([
      "2026-02-11",
      "2026-02-10",
      "2026-02-04",
    ]);
    const before = await gameStatRows(db, alice, {
      ...ALL,
      period: { kind: "games", skip: 3, take: 2 },
    });
    expect(before.map((r) => r.playedAt.toISOString().slice(0, 10))).toEqual([
      "2026-02-03",
      "2026-02-02",
    ]);
  });

  it("has no analysis values for a game that is not analysed", async () => {
    const rows = await gameStatRows(db, alice, {
      ...ALL,
      period: { kind: "days", from: "2026-02-10", to: "2026-02-10" },
    });
    expect(rows).toHaveLength(1);
    expect(rows[0]).toMatchObject({
      analysed: false,
      accuracy: null,
      blunders: null,
      phaseAccuracy: { opening: null, middlegame: null, endgame: null },
    });
  });

  it("reads nothing for a stranger and is stable for a scope", async () => {
    const stranger = (await createTestSession(db)).user.id;
    expect((await gameStats(db, stranger, ALL)).n).toBe(0);
    expect(await gameStats(db, alice, ALL)).toEqual(await gameStats(db, alice, ALL));
    expect(JSON.stringify(await gameStats(db, alice, ALL))).not.toContain("NaN");
  });
});

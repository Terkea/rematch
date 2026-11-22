import {
  bandReference,
  gameMetrics,
  gameOpenings,
  moveFeatures,
  moves,
  openingLines,
  referenceBuilds,
} from "@ci/db/generated";
import { and, eq, sql } from "drizzle-orm";
import { beforeEach, describe, expect, it } from "vitest";
import { type CoachScope, resolveCoachScope } from "../../src/server/profile";
import { loadStrengths } from "../../src/server/profile/strengths";
import { resetGameTables, seedAccount, seedSyntheticGame } from "../helpers/games";
import { seedProfile } from "../helpers/profile";
import { createTestSession, resetAuthTables, webTestDb } from "../helpers/session";

// The strengths read model's loader against seeded synthetic rows: each section's reads, scoped to
// the user's own account and time class.

const db = webTestDb();
let userId: string;
let accountId: string;

const DAY = 86_400_000;
const BASE = Date.UTC(2026, 5, 1, 12);

async function scopeOf(uid: string): Promise<CoachScope> {
  const scope = await resolveCoachScope(db, uid, {});
  if (!scope) throw new Error("no scope");
  return scope;
}

/** A synthetic rapid game (player rated 1500 on chess.com: B3) with its metrics row. */
async function analysed(i: number, metrics: Record<string, unknown>): Promise<string> {
  const playedAt = new Date(BASE + i * DAY);
  const { gameId } = await seedSyntheticGame(db, accountId, {
    playedAt: playedAt.toISOString(),
    timeClass: "rapid",
    result: "1-0",
  });
  await db.insert(gameMetrics).values({
    gameId,
    accountId,
    timeClass: "rapid",
    playedAt,
    protocol: "test-1",
    metrics: { version: 1, ...metrics },
  });
  return gameId;
}

beforeEach(async () => {
  await resetAuthTables(db);
  await resetGameTables(db);
  await db.execute(sql`delete from reference_builds where kind = 'band_reference'`);
  userId = (await createTestSession(db)).user.id;
  accountId = (await seedAccount(db, { userId, username: "Hero" })).accountId;
});

describe("loadStrengths", () => {
  it("compares the last 50 games with earlier ones while there is no band reference", async () => {
    for (let i = 0; i < 70; i++) {
      await analysed(i, { accuracy: i < 20 ? 58 + (i % 5) : 74 + (i % 5) });
    }
    const m = await loadStrengths(db, userId, await scopeOf(userId));
    expect(m?.above.mode).toBe("own");
    if (m?.above.mode !== "own") return;
    expect(m.above.trends.claims.map((c) => c.metric)).toEqual(["accuracy"]);
    expect(m.above.trends.claims[0]?.recent.n).toBe(50);
    expect(m.above.trends.claims[0]?.earlier.n).toBe(20);
    expect(m.improving.kpis).toBeNull();
  });

  it("compares with the active build's non-low-sample band median when there is one", async () => {
    await analysed(0, { accuracy: 74 });
    const [build] = await db
      .insert(referenceBuilds)
      .values({
        kind: "band_reference",
        status: "active",
        sourceMonth: "2026-09",
        sourceSha256: "0".repeat(64),
      })
      .returning({ id: referenceBuilds.id });
    const row = {
      buildId: build?.id as string,
      timeClass: "rapid" as const,
      p25: 0,
      p40: 0,
      p75: 0,
    };
    await db.insert(bandReference).values([
      {
        ...row,
        band: "B3",
        metric: "accuracy",
        median: 70,
        nPlayers: 300,
        nGames: 9000,
        lowSample: false,
      },
      {
        ...row,
        band: "B3",
        metric: "blunders_per_game",
        median: 3,
        nPlayers: 4,
        nGames: 40,
        lowSample: true,
      },
    ]);
    await seedProfile(db, accountId, {
      timeClass: "rapid",
      kpis: [
        { metric: "accuracy", value: 74.2, ciLow: 72.8, ciHigh: 75.6, n: 40 },
        { metric: "blunders_per_game", value: 0.5, ciLow: 0.3, ciHigh: 0.7, n: 40 },
      ],
    });
    const m = await loadStrengths(db, userId, await scopeOf(userId));
    expect(m?.above).toMatchObject({ mode: "band", band: "B3", compared: 1 });
    if (m?.above.mode !== "band") return;
    expect(m.above.claims.map((c) => [c.metric, c.median, c.n])).toEqual([["accuracy", 70, 40]]);
    expect(m.improving.kpis).not.toBeNull();
  });

  it("lists a confirmed improving weakness with its transition's evidence", async () => {
    await analysed(0, {});
    await seedProfile(db, accountId, {
      timeClass: "rapid",
      estimates: [
        { seriesKey: "W1", state: "improving", nUnweighted: 2400 },
        { seriesKey: "W2", state: "active", nUnweighted: 5000 },
      ],
      history: [
        {
          seriesKey: "W1",
          kind: "transition",
          fromState: "active",
          toState: "improving",
          atPlayedAt: new Date("2026-10-14T12:00:00Z"),
          pChange: 0.9,
          rateBefore: 0.04,
          rateAfter: 0.026,
          evidence: { k: 21, n: 800 },
        },
      ],
    });
    const m = await loadStrengths(db, userId, await scopeOf(userId));
    expect(m?.improving.weaknesses).toMatchObject([
      {
        seriesKey: "W1",
        label: "Hung material",
        state: "improving",
        pChange: expect.closeTo(0.9, 5),
        afterCounts: { events: 21, chances: 800 },
      },
    ]);
  });

  it("finds an opening line scoring above its expected score, with the stored verdict", async () => {
    for (let i = 0; i < 12; i++) {
      const gameId = await analysed(i, { score: i < 11 ? 1 : 0, expectedScore: 0.5 });
      await db.insert(gameOpenings).values({
        gameId,
        accountId,
        timeClass: "rapid",
        color: "white",
        fenKeys: [],
        ucis: [],
        sans: [],
        lineKey: "italian",
        lineName: "Italian Game",
        lineKind: "chosen",
      });
    }
    await db.insert(openingLines).values({
      accountId,
      timeClass: "rapid",
      color: "white",
      lineKey: "italian",
      kind: "chosen",
      games: 12,
      verdict: "keep",
      summary: {},
    });
    const m = await loadStrengths(db, userId, await scopeOf(userId));
    expect(m?.openings.judged).toBe(1);
    expect(m?.openings.lines).toMatchObject([
      { name: "Italian Game", n: 12, expected: 0.5, verdict: "keep", kind: "chosen" },
    ]);
  });

  it("picks a critical strength ply while Maia is pending, from its move row", async () => {
    const gameId = await analysed(0, {
      strengths: [
        { kind: "only_move_found", ply: 3 },
        { kind: "conversion", ply: 5 },
      ],
    });
    await db.insert(moveFeatures).values({
      gameId,
      ply: 3,
      enrichmentProtocol: "test-1",
      difficultyStatus: "pending_maia",
      isCritical: true,
      multipvGap: 20,
    });
    const [move] = await db
      .select()
      .from(moves)
      .where(and(eq(moves.gameId, gameId), eq(moves.ply, 3)));
    const m = await loadStrengths(db, userId, await scopeOf(userId));
    expect(m?.hardSpots.mode).toBe("critical");
    expect(m?.hardSpots.spots).toMatchObject([
      { gameId, ply: 3, kind: "only_move_found", fen: move?.fenBefore, san: move?.san },
    ]);
    expect(m?.hardSpots.notes.only_move_found).toMatch(/^Keep doing this: /);
    expect(m?.difficulty).toMatchObject({ positions: 1, pendingMaia: 1 });
  });

  it("reads the newest report input's strengths", async () => {
    await seedProfile(db, accountId, {
      timeClass: "rapid",
      games: 1,
      input: {
        strengths: [{ area: "tactics", label: "Sharp eye", metricKey: null, exemplars: [] }],
      },
    });
    const m = await loadStrengths(db, userId, await scopeOf(userId));
    expect(m?.report?.strengths).toEqual([
      { area: "tactics", label: "Sharp eye", metricKey: null, examples: 0 },
    ]);
  });

  it("reads nothing for another user's scope", async () => {
    await analysed(0, {});
    const scope = await scopeOf(userId);
    const other = (await createTestSession(db, { email: "other@example.invalid" })).user.id;
    expect(await loadStrengths(db, other, scope)).toBeNull();
  });
});

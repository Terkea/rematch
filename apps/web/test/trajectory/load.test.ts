import { games } from "@ci/db";
import { eq } from "drizzle-orm";
import { beforeAll, describe, expect, it } from "vitest";
import { loadTrajectory, priorOf } from "../../src/server/trajectory/load";
import { resetGameTables, seedAccount, seedSyntheticGame } from "../helpers/games";
import { seedProfile } from "../helpers/profile";
import { createTestSession, resetAuthTables, webTestDb } from "../helpers/session";

// The trajectory loader (F6): rated games of one account × time class, the player's pre-game
// rating (on chess.com the rating after their previous rated game, ADR-FIX-C-1), the latest
// stored rating as the current one, and the top weaknesses' points. Synthetic games only.

const db = webTestDb();
const DAY = 86_400_000;
const BASE = Date.UTC(2026, 0, 1, 12);

let alice: string;
let bob: string;
let account: string;
const ids: string[] = [];
/** Stored (post-game, chess.com) ratings: up to 1,100 at game 12, then down. */
const stored = Array.from({ length: 24 }, (_, i) => (i <= 12 ? 1000 + i * 8 : 1096 - (i - 12) * 20));

beforeAll(async () => {
  await resetAuthTables(db);
  await resetGameTables(db);
  alice = (await createTestSession(db)).user.id;
  bob = (await createTestSession(db)).user.id;
  account = (await seedAccount(db, { userId: alice, username: "alice-traj" })).accountId;
  for (let i = 0; i < stored.length; i++) {
    const { gameId } = await seedSyntheticGame(db, account, {
      playedAt: new Date(BASE + i * DAY).toISOString(),
      timeClass: "rapid",
      result: i % 2 === 0 ? "1-0" : "0-1",
      playerColor: "white",
      accuracy: 70,
    });
    await db
      .update(games)
      .set({ whiteRating: stored[i] as number })
      .where(eq(games.id, gameId));
    ids.push(gameId);
  }
  // An unrated game and a blitz game never count.
  const unrated = await seedSyntheticGame(db, account, {
    playedAt: new Date(BASE + 30 * DAY).toISOString(),
    timeClass: "rapid",
    result: "1-0",
  });
  await db.update(games).set({ rated: false, whiteRating: 2000 }).where(eq(games.id, unrated.gameId));
  await seedSyntheticGame(db, account, {
    playedAt: new Date(BASE + 31 * DAY).toISOString(),
    timeClass: "blitz",
    result: "1-0",
  });
  await seedProfile(db, account, {
    timeClass: "rapid",
    estimates: [
      // Prior mu 0.2, n0 60: alpha = 12 + k, beta = 48 + n - k.
      { seriesKey: "W3.shallow", k: 30, n: 100, alpha: 42, beta: 118, priority: 2 },
      { seriesKey: "W5", k: 1, n: 10, alpha: 7, beta: 33, priority: 0 },
    ],
    points: ids.map((gameId, i) => ({
      seriesKey: "W3.shallow",
      gameId,
      playedAt: new Date(BASE + i * DAY),
      events: i >= 12 ? 3 : 1,
      opportunities: 5,
    })),
  });
});

describe("loadTrajectory", () => {
  it("reads pre-game ratings: the previous rated game's on chess.com", async () => {
    const t = await loadTrajectory(db, alice, { accountId: account, timeClass: "rapid" });
    if (!t || t.kind === "not_enough_games") throw new Error("expected a trajectory");
    expect(t.games).toBe(24);
    // The first game has no earlier one, so it keeps its stored rating.
    expect(t.series[0]?.rating).toBe(1000);
    expect(t.series[1]?.rating).toBe(1000);
    expect(t.series[13]?.rating).toBe(1096);
    // The current rating is the newest stored one, after the last game.
    expect(t.current.rating).toBe(stored.at(-1));
    expect(t.peak.rating).toBe(1096);
  });

  it("compares the top claimable weaknesses only, under their stored prior", async () => {
    const t = await loadTrajectory(db, alice, { accountId: account, timeClass: "rapid" });
    if (!t || t.kind === "not_enough_games") throw new Error("expected a trajectory");
    const weak = t.rows.filter((r) => r.unit === "rate").map((r) => r.key);
    expect(weak).toEqual(["weakness:W3.shallow"]);
  });

  it("reads nothing for another user's account", async () => {
    expect(await loadTrajectory(db, bob, { accountId: account, timeClass: "rapid" })).toBeNull();
  });
});

describe("priorOf", () => {
  it("recovers the estimate's prior from its posterior shapes", () => {
    const p = priorOf({ k: 30, n: 100, alpha: 42, beta: 118 });
    expect(p.n0).toBeCloseTo(60, 9);
    expect(p.mu).toBeCloseTo(0.2, 9);
  });

  it("falls back to the engine's no-benchmark prior for shapes that hold none", () => {
    expect(priorOf({ k: 5, n: 10, alpha: 5, beta: 5 })).toEqual({ mu: 0.5, n0: 2 });
  });
});

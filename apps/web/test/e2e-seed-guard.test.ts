import { accountLinks, games, linkedAccounts, moves } from "@ci/db";
import { and, eq, sql } from "drizzle-orm";
import { beforeEach, describe, expect, it } from "vitest";
import { E2E_USER, seedE2eGames } from "../e2e/helpers/seed-games";
import { resetGameTables } from "./helpers/games";
import { createTestSession, resetAuthTables, webTestDb } from "./helpers/session";

// The games e2e seed (M5a Task 13). It must refuse a database that holds a real player's account,
// in case the default compose project (the owner's games) is up when the suite runs, and it must
// write nothing when it refuses. The refused username is synthetic: no owner data in the repo.

const db = webTestDb();

async function count(table: typeof games | typeof linkedAccounts | typeof accountLinks) {
  const [row] = await db.select({ n: sql<number>`count(*)::int` }).from(table);
  return row?.n ?? 0;
}

beforeEach(async () => {
  await resetAuthTables(db);
  await resetGameTables(db);
  await createTestSession(db, { email: E2E_USER });
});

describe("seedE2eGames", () => {
  it("refuses, writing nothing, when a linked account is not an e2e or fake one", async () => {
    await db
      .insert(linkedAccounts)
      .values({ platform: "chesscom", username: "realplayer", displayName: "RealPlayer" });
    await expect(seedE2eGames({ db })).rejects.toThrow(
      "Refusing to seed: this database holds real accounts",
    );
    expect(await count(linkedAccounts)).toBe(1);
    expect(await count(games)).toBe(0);
    expect(await count(accountLinks)).toBe(0);
  });

  it("refuses a name that only contains the allowed prefixes", async () => {
    await db
      .insert(linkedAccounts)
      .values({ platform: "chesscom", username: "note2e-fake", displayName: "x" });
    await expect(seedE2eGames({ db })).rejects.toThrow(/holds real accounts/);
  });

  it("seeds the fixture games, 30 synthetic ones, a pending and a 300-ply game", async () => {
    // The accounts e2e leaves its fake player behind; that one is allowed.
    await db
      .insert(linkedAccounts)
      .values({ platform: "chesscom", username: "fakeplayer", displayName: "FakePlayer" });
    const ids = await seedE2eGames({ db });

    const [account] = await db
      .select({ id: linkedAccounts.id })
      .from(linkedAccounts)
      .where(eq(linkedAccounts.username, "e2e-fixtures"));
    expect(account).toBeDefined();
    const rows = await db
      .select({ id: games.id, status: games.analysisStatus, playedAt: games.playedAt })
      .from(games)
      .where(eq(games.accountId, account?.id as string));
    expect(rows).toHaveLength(35);
    expect(rows.filter((r) => r.status === "pending").map((r) => r.id)).toEqual([ids.pendingId]);
    for (const r of rows) {
      const month = r.playedAt.toISOString().slice(0, 7);
      expect(["2026-09", "2026-10"]).toContain(month);
    }

    const plies = async (id: string) =>
      (
        await db.select({ n: sql<number>`count(*)::int` }).from(moves).where(eq(moves.gameId, id))
      )[0]?.n;
    expect(await plies(ids.longId)).toBe(300);
    expect(await plies(ids.pendingId)).toBe(0);
    // The analysed game is a loss whose fourth ply carries an engine move ("Show the engine's
    // next move" at ply 3).
    const [analysed] = await db
      .select({ result: games.result, colour: games.playerColor })
      .from(games)
      .where(eq(games.id, ids.analysedId));
    expect(analysed).toEqual({ result: "1-0", colour: "black" });
    const [fourth] = await db
      .select({ best: moves.bestUci, uci: moves.uci })
      .from(moves)
      .where(and(eq(moves.gameId, ids.analysedId), eq(moves.ply, 4)));
    expect(fourth?.best).toMatch(/^[a-h][1-8][a-h][1-8]/);
    expect(fourth?.best).not.toBe(fourth?.uci);

    // Run again (several specs seed): the same games, nothing duplicated.
    expect(await seedE2eGames({ db })).toEqual(ids);
    expect(await count(games)).toBe(35);
  });
});

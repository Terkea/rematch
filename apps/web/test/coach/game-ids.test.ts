import { games } from "@ci/db/generated";
import { eq } from "drizzle-orm";
import { beforeEach, describe, expect, it } from "vitest";
import { reviewGameIds } from "../../src/server/profile/game-ids.js";
import { resetGameTables, seedAccount, seedSyntheticGame } from "../helpers/games";
import { createTestSession, resetAuthTables, webTestDb } from "../helpers/session";

// Chat segments and anchors name a game by its PLATFORM id (the coaching input's id); the review
// page takes `games.id`. The map is read for the signed-in user's own account only.

const db = webTestDb();

let a: string;
let b: string;
let mine: string;
let theirs: string;

async function platformId(gameId: string): Promise<string> {
  const [g] = await db.select({ p: games.platformGameId }).from(games).where(eq(games.id, gameId));
  return g?.p ?? "";
}

beforeEach(async () => {
  await resetAuthTables(db);
  await resetGameTables(db);
  a = (await createTestSession(db)).user.id;
  b = (await createTestSession(db)).user.id;
  mine = (await seedAccount(db, { userId: a, username: "mine" })).accountId;
  theirs = (await seedAccount(db, { userId: b, username: "theirs" })).accountId;
});

describe("reviewGameIds", () => {
  it("maps the account's platform ids to games.id and leaves others out", async () => {
    const { gameId } = await seedSyntheticGame(db, mine, {
      playedAt: "2026-11-01T10:00:00.000Z",
      timeClass: "rapid",
      result: "1-0",
    });
    const { gameId: other } = await seedSyntheticGame(db, theirs, {
      playedAt: "2026-11-01T10:00:00.000Z",
      timeClass: "rapid",
      result: "1-0",
    });
    const p = await platformId(gameId);
    const q = await platformId(other);
    const map = await reviewGameIds(db, a, { accountId: mine }, [p, q, "unknown", ""]);
    expect([...map]).toEqual([[p, gameId]]);
  });

  it("reads nothing for an account the user is not linked to", async () => {
    const { gameId } = await seedSyntheticGame(db, theirs, {
      playedAt: "2026-11-01T10:00:00.000Z",
      timeClass: "rapid",
      result: "1-0",
    });
    const map = await reviewGameIds(db, a, { accountId: theirs }, [await platformId(gameId)]);
    expect(map.size).toBe(0);
  });
});

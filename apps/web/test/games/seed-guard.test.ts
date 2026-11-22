import { createDb, linkedAccounts } from "@ci/db";
import { sql } from "drizzle-orm";
import { afterAll, beforeEach, describe, expect, it } from "vitest";
import { resetGameTables, seedAccount, seedFixtureGame, seedSyntheticGame } from "../helpers/games";
import { createTestSession, resetAuthTables, webTestDb } from "../helpers/session";

const db = webTestDb();

// The same server's maintenance database: a name without `_test`. The guard must refuse before
// any statement other than its own check runs there.
const otherUrl = new URL(process.env.WEB_DATABASE_URL_TEST as string);
otherUrl.pathname = "/postgres";
const other = createDb(otherUrl.toString());

beforeEach(async () => {
  await resetAuthTables(db);
  await db.execute(sql`truncate table linked_accounts, games, moves, account_links cascade`);
});

afterAll(async () => {
  await db.execute(sql`truncate table linked_accounts, games, moves, account_links cascade`);
});

describe("seed helper guard", () => {
  it("refuses a database whose name does not end in _test", async () => {
    await expect(resetGameTables(other)).rejects.toThrow(/does not end in _test/);
    await expect(seedAccount(other, { username: "x" })).rejects.toThrow(/_test/);
  });

  it("refuses when an imported account exists", async () => {
    await db.insert(linkedAccounts).values({
      platform: "chesscom",
      username: "real",
      displayName: "Real",
      lastSyncedAt: new Date(),
    });
    await expect(resetGameTables(db)).rejects.toThrow(/real or verified accounts/);
    await expect(seedAccount(db, { username: "x" })).rejects.toThrow(/real or verified/);
    expect(await db.select().from(linkedAccounts)).toHaveLength(1);
  });

  it("refuses when a link was verified by a real method", async () => {
    const { user } = await createTestSession(db);
    const [acct] = await db
      .insert(linkedAccounts)
      .values({ platform: "chesscom", username: "proven", displayName: "Proven" })
      .returning({ id: linkedAccounts.id });
    await db.execute(
      sql`insert into account_links (user_id, linked_account_id, method, verified_at)
          values (${user.id}, ${acct?.id}, 'location_code', now())`,
    );
    await expect(
      seedSyntheticGame(db, acct?.id as string, {
        playedAt: "2026-10-01T00:00:00Z",
        timeClass: "rapid",
        result: "1-0",
      }),
    ).rejects.toThrow(/real or verified/);
  });

  it("allows its own synthetic accounts, verified ones included", async () => {
    const { user } = await createTestSession(db);
    const { accountId } = await seedAccount(db, {
      userId: user.id,
      username: "Mine",
      verified: true,
    });
    await seedSyntheticGame(db, accountId, {
      playedAt: "2026-10-01T00:00:00Z",
      timeClass: "rapid",
      result: "1-0",
    });
    const { games: fx } = await import("@ci/ui/fixtures");
    await seedFixtureGame(db, accountId, fx.shortAttack);
    await expect(resetGameTables(db)).resolves.toBeUndefined();
  });
});

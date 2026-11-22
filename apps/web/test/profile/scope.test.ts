import { randomUUID } from "node:crypto";
import { beforeAll, beforeEach, describe, expect, it } from "vitest";
import { resolveCoachScope } from "../../src/server/profile";
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
  userId = (await createTestSession(db)).user.id;
  accountId = (await seedAccount(db, { userId, username: "Hero" })).accountId;
});

describe("resolveCoachScope", () => {
  it("defaults to the option with most analysed games", async () => {
    await seedProfile(db, accountId, { timeClass: "rapid", games: 30 });
    await seedProfile(db, accountId, { timeClass: "blitz", games: 12 });
    const scope = await resolveCoachScope(db, userId, {});
    expect(scope).toMatchObject({
      accountId,
      timeClass: "rapid",
      platform: "chesscom",
      username: "hero",
    });
    expect(scope?.options).toEqual([
      { accountId, platform: "chesscom", username: "hero", timeClass: "rapid", analysedGames: 30 },
      { accountId, platform: "chesscom", username: "hero", timeClass: "blitz", analysedGames: 12 },
    ]);
  });

  it("resolveCoachScope falls back", async () => {
    await seedProfile(db, accountId, { timeClass: "rapid", games: 3 });
    await seedProfile(db, accountId, { timeClass: "blitz", games: 2 });
    for (const params of [
      { account: randomUUID(), tc: "hyper" },
      { account: "x'; drop" },
      { account: "", tc: "" },
      { account: accountId, tc: "hyper" },
    ]) {
      const scope = await resolveCoachScope(db, userId, params);
      expect(scope?.timeClass, JSON.stringify(params)).toBe("rapid");
      expect(scope?.accountId).toBe(accountId);
    }
  });

  it("never picks another user's account", async () => {
    await seedProfile(db, accountId, { timeClass: "rapid", games: 1 });
    const other = (await createTestSession(db)).user.id;
    const foreign = (await seedAccount(db, { userId: other, username: "Villain" })).accountId;
    await seedProfile(db, foreign, { timeClass: "blitz", games: 40 });
    const scope = await resolveCoachScope(db, userId, { account: foreign, tc: "blitz" });
    expect(scope?.accountId).toBe(accountId);
    expect(scope?.options.map((o) => o.accountId)).toEqual([accountId]);
  });

  it("honours a valid choice", async () => {
    await seedProfile(db, accountId, { timeClass: "rapid", games: 3 });
    await seedProfile(db, accountId, { timeClass: "blitz", games: 2 });
    const scope = await resolveCoachScope(db, userId, { account: accountId, tc: "blitz" });
    expect(scope?.timeClass).toBe("blitz");
    expect((await resolveCoachScope(db, userId, { tc: "blitz" }))?.timeClass).toBe("blitz");
    // A foreign account is ignored, not a reason to drop a valid time class; arrays use the first.
    const mixed = await resolveCoachScope(db, userId, {
      account: [randomUUID(), accountId],
      tc: ["blitz", "rapid"],
    });
    expect(mixed).toMatchObject({ accountId, timeClass: "blitz" });
  });

  it("no analysed games gives null", async () => {
    expect(await resolveCoachScope(db, userId, {})).toBeNull();
    const lonely = (await createTestSession(db)).user.id;
    expect(await resolveCoachScope(db, lonely, {})).toBeNull();
  });
});

import { randomUUID } from "node:crypto";
import { games as fixtures } from "@ci/ui/fixtures";
import { beforeAll, describe, expect, it } from "vitest";
import { findLabReview } from "../src/app/lab/review/load.js";
import { resetGameTables, seedAccount, seedFixtureGame, seedSyntheticGame } from "./helpers/games";
import { createTestSession, resetAuthTables, webTestDb } from "./helpers/session";

// findLabReview (Task 7): the given game, or else the user's latest analysed game with the most
// moments. Scoped like getGameReview: another user's game is never picked or shown.

const db = webTestDb();

let alice: string;
let bob: string;
let fewMoments: string;
let mostOlder: string;
let mostNewer: string;
let pending: string;
let bobsGame: string;

beforeAll(async () => {
  await resetAuthTables(db);
  await resetGameTables(db);
  alice = (await createTestSession(db)).user.id;
  bob = (await createTestSession(db)).user.id;
  const aliceAccount = (await seedAccount(db, { userId: alice, username: "AliceLab" })).accountId;
  const bobAccount = (await seedAccount(db, { userId: bob, username: "BobLab" })).accountId;

  fewMoments = (
    await seedSyntheticGame(db, aliceAccount, {
      playedAt: "2026-11-10T10:00:00Z",
      timeClass: "rapid",
      result: "1-0",
      counts: { blunders: 1 },
    })
  ).gameId;
  mostOlder = (
    await seedSyntheticGame(db, aliceAccount, {
      playedAt: "2026-11-01T10:00:00Z",
      timeClass: "rapid",
      result: "0-1",
      counts: { blunders: 2, mistakes: 2 },
    })
  ).gameId;
  mostNewer = (
    await seedSyntheticGame(db, aliceAccount, {
      playedAt: "2026-11-05T10:00:00Z",
      timeClass: "blitz",
      result: "0-1",
      counts: { blunders: 1, mistakes: 1, inaccuracies: 2 },
    })
  ).gameId;
  // Not analysed, with stale judgements: never the default pick.
  pending = (
    await seedSyntheticGame(db, aliceAccount, {
      playedAt: "2026-11-20T10:00:00Z",
      timeClass: "rapid",
      result: "0-1",
      status: "pending",
      stale: true,
      counts: { blunders: 6 },
    })
  ).gameId;
  bobsGame = (
    await seedFixtureGame(db, bobAccount, fixtures.middlegameBlunder, { protocol: "lab-1" })
  ).gameId;
});

describe("findLabReview", () => {
  it("returns the given game", async () => {
    const r = await findLabReview(db, alice, fewMoments);
    expect(r?.id).toBe(fewMoments);
  });

  it("without a game, picks the latest analysed game with the most moments", async () => {
    const r = await findLabReview(db, alice);
    expect(r?.id).toBe(mostNewer);
    expect(r?.analysed).toBe(true);
    expect(r?.moments).toHaveLength(4);
    // An older game ties on moments: the latest one wins.
    expect((await findLabReview(db, alice, mostOlder))?.moments).toHaveLength(4);
  });

  it("never picks a game that is not analysed", async () => {
    const r = await findLabReview(db, alice);
    expect(r?.id).not.toBe(pending);
  });

  it("does not show another user's game, given or picked", async () => {
    expect(await findLabReview(db, alice, bobsGame)).toBeNull();
    expect((await findLabReview(db, bob))?.id).toBe(bobsGame);
  });

  it("is null for an unknown or malformed id, and for a user with no games", async () => {
    expect(await findLabReview(db, alice, randomUUID())).toBeNull();
    expect(await findLabReview(db, alice, "x'; drop")).toBeNull();
    const carol = (await createTestSession(db)).user.id;
    expect(await findLabReview(db, carol)).toBeNull();
  });

  it("an empty game param is the same as none", async () => {
    expect((await findLabReview(db, alice, ""))?.id).toBe(mostNewer);
  });
});

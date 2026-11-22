import { importRuns } from "@ci/db";
import { eq } from "drizzle-orm";
import { beforeAll, describe, expect, it } from "vitest";
import { loadExplorer } from "../../src/server/games/explorer";
import { resetGameTables, seedAccount, seedSyntheticGame } from "../helpers/games";
import { createTestSession, resetAuthTables, webTestDb } from "../helpers/session";

const db = webTestDb();

let nobody: string;
let empty: string;
let player: string;
let other: string;
let mainId: string;
let altId: string;
let foreignId: string;
let emptyAccountId: string;

beforeAll(async () => {
  await resetAuthTables(db);
  await resetGameTables(db);
  nobody = (await createTestSession(db)).user.id;
  empty = (await createTestSession(db)).user.id;
  player = (await createTestSession(db)).user.id;
  other = (await createTestSession(db)).user.id;
  emptyAccountId = (await seedAccount(db, { userId: empty, username: "EmptyOne" })).accountId;
  mainId = (await seedAccount(db, { userId: player, username: "MainHandle", verified: true }))
    .accountId;
  altId = (await seedAccount(db, { userId: player, username: "AltHandle" })).accountId;
  foreignId = (await seedAccount(db, { userId: other, username: "ForeignHandle" })).accountId;
  await seedSyntheticGame(db, foreignId, {
    playedAt: "2026-10-05T12:00:00Z",
    timeClass: "rapid",
    result: "1-0",
  });
  for (let i = 0; i < 3; i++) {
    await seedSyntheticGame(db, mainId, {
      playedAt: new Date(Date.UTC(2026, 9, 1 + i, 12)).toISOString(),
      timeClass: "rapid",
      result: "1-0",
      playerColor: "white",
    });
  }
});

describe("loadExplorer", () => {
  it("reports no links", async () => {
    const e = await loadExplorer(db, nobody, {});
    expect(e.state).toBe("no-links");
  });

  it("reports links with no games yet, importing while the account was never synced", async () => {
    const e = await loadExplorer(db, empty, {});
    expect(e.state).toBe("no-games");
    if (e.state !== "no-games") return;
    expect(e.importState).toBe("importing");
    expect(e.accounts.map((a) => a.label)).toEqual(["chess.com EmptyOne"]);
  });

  it("lists the games and the user's own accounts, labelled", async () => {
    const e = await loadExplorer(db, player, {});
    expect(e.state).toBe("games");
    if (e.state !== "games") return;
    expect(e.page.total).toBe(3);
    expect(e.accounts.map((a) => a.label).sort()).toEqual([
      "chess.com AltHandle",
      "chess.com MainHandle",
    ]);
    expect(e.accounts.map((a) => a.id).sort()).toEqual([mainId, altId].sort());
  });

  it("keeps games (not the empty state) when only the filters match nothing", async () => {
    const e = await loadExplorer(db, player, { tc: "blitz" });
    expect(e.state).toBe("games");
    if (e.state !== "games") return;
    expect(e.page.total).toBe(0);
    expect(e.query.filters.timeClass).toBe("blitz");
    expect(e.relax).toEqual([{ key: "timeClass", total: 3 }]);
  });

  it("never shows another user's account, even by id", async () => {
    const e = await loadExplorer(db, player, { account: foreignId });
    expect(e.state).toBe("games");
    if (e.state !== "games") return;
    expect(e.page.total).toBe(0);
    expect(e.page.rows).toEqual([]);
  });

  it("treats an account with no games, chosen by filter, as a filtered miss", async () => {
    const e = await loadExplorer(db, player, { account: altId });
    expect(e.state).toBe("games");
    if (e.state !== "games") return;
    expect(e.page.total).toBe(0);
  });

  describe("the import state of an empty account comes from its latest import run", () => {
    const MIN = 60_000;
    async function latestRun(
      status: "running" | "done" | "failed" | "paused",
      heartbeatAgoMs: number,
    ): Promise<void> {
      await db.delete(importRuns).where(eq(importRuns.accountId, emptyAccountId));
      const at = new Date(Date.now() - heartbeatAgoMs);
      // An older finished run first, so only the latest one may decide.
      await db.insert(importRuns).values({
        accountId: emptyAccountId,
        status: "done",
        startedAt: new Date(at.getTime() - 60 * MIN),
        heartbeatAt: new Date(at.getTime() - 60 * MIN),
        finishedAt: new Date(at.getTime() - 60 * MIN),
      });
      await db.insert(importRuns).values({
        accountId: emptyAccountId,
        status,
        startedAt: at,
        heartbeatAt: at,
        finishedAt: status === "running" ? null : at,
      });
    }
    async function state() {
      const e = await loadExplorer(db, empty, {});
      if (e.state !== "no-games") throw new Error(`expected no-games, got ${e.state}`);
      return e.importState;
    }

    it("is importing while a run beats", async () => {
      await latestRun("running", 1 * MIN);
      expect(await state()).toBe("importing");
    });

    it("is failed when the run stopped beating (stuck)", async () => {
      await latestRun("running", 30 * MIN);
      expect(await state()).toBe("failed");
    });

    it("is failed when the latest run failed, even though the account never synced", async () => {
      await latestRun("failed", 1 * MIN);
      expect(await state()).toBe("failed");
    });

    it("is done when the latest run finished with nothing", async () => {
      await latestRun("done", 1 * MIN);
      expect(await state()).toBe("done");
    });
  });
});

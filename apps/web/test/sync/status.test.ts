import { sql } from "drizzle-orm";
import { beforeEach, describe, expect, it } from "vitest";
import { getSyncStatus } from "../../src/server/sync";
import { resetGameTables, seedAccount } from "../helpers/games";
import { createTestSession, resetAuthTables, webTestDb } from "../helpers/session";
import {
  resetSyncTables,
  seedAnalyseMetrics,
  seedGames,
  seedImportRun,
  seedJob,
} from "../helpers/sync";

const db = webTestDb();
const MIN = 60_000;
let alice: string;
let bob: string;
let accountId: string;

beforeEach(async () => {
  await resetAuthTables(db);
  await resetGameTables(db);
  await resetSyncTables(db);
  alice = (await createTestSession(db, { email: "alice@example.invalid" })).user.id;
  bob = (await createTestSession(db, { email: "bob@example.invalid" })).user.id;
  ({ accountId } = await seedAccount(db, { userId: alice, username: "Alice-Status" }));
});

async function only(userId = alice) {
  const status = await getSyncStatus(db, userId);
  expect(status.accounts).toHaveLength(1);
  const [a] = status.accounts;
  if (!a) throw new Error("no account");
  return a;
}

const importJob = (over: Partial<Parameters<typeof seedJob>[1]>) =>
  seedJob(db, {
    kind: "import-account",
    dedupeKey: `import:${accountId}`,
    userKey: accountId,
    ...over,
  });

describe("getSyncStatus", () => {
  it("a fresh account is idle with zero counts and no ETA", async () => {
    const a = await only();
    expect(a).toMatchObject({
      accountId,
      platform: "chesscom",
      name: "Alice-Status",
      verified: false,
      phase: "idle",
      retryAt: null,
      lastCheck: null,
      lastRunFailed: false,
      nextManualAt: null,
      profileUpdating: false,
      eta: null,
      analysis: { waiting: 0, analysing: 0, done: 0, failed: 0, retrying: 0, notSelected: 0 },
    });
  });

  it("counts games by analysis status", async () => {
    await seedGames(db, accountId, "pending", 3);
    await seedGames(db, accountId, "analysing", 1);
    await seedGames(db, accountId, "done", 4);
    await seedGames(db, accountId, "failed", 2);
    await seedGames(db, accountId, "imported", 5);
    await seedGames(db, accountId, "skipped", 1);
    const a = await only();
    expect(a.analysis).toEqual({
      waiting: 3,
      analysing: 1,
      done: 4,
      failed: 2,
      retrying: 0,
      notSelected: 5,
      skipped: 1,
    });
  });

  it("a failed game with a live retry counts as retrying", async () => {
    const [g] = await seedGames(db, accountId, "failed", 2);
    await seedJob(db, { kind: "analyse-game", dedupeKey: `analyse:${g}`, userKey: accountId });
    const a = await only();
    expect(a.analysis.failed).toBe(1);
    expect(a.analysis.retrying).toBe(1);
  });

  it("a due or running import is checking; a held-back one is waiting; paused is paused", async () => {
    const id = await importJob({ state: "queued" });
    expect((await only()).phase).toBe("checking");
    await db.execute(sql`update jobs set state = 'running' where id = ${id}`);
    expect((await only()).phase).toBe("checking");
    await db.execute(sql`update jobs set state = 'paused' where id = ${id}`);
    expect((await only()).phase).toBe("paused");
    const runAt = new Date(Date.now() + 20 * MIN);
    await db.execute(
      sql`update jobs set state = 'queued', run_at = ${runAt.toISOString()}::timestamptz where id = ${id}`,
    );
    const a = await only();
    expect(a.phase).toBe("waiting");
    expect(a.retryAt).toBe(runAt.toISOString());
  });

  it("due mirrors Rust: run_at at or before created_at is due", async () => {
    await importJob({
      state: "queued",
      createdAt: new Date(Date.now() + 30 * MIN),
      runAt: new Date(Date.now() + 20 * MIN),
    });
    const a = await only();
    expect(a.phase).toBe("checking");
    expect(a.retryAt).toBeNull();
  });

  it("reports the last finished check and what it found", async () => {
    const finished = new Date(Date.now() - 3 * MIN);
    await seedImportRun(db, accountId, {
      status: "done",
      imported: 1,
      startedAt: new Date(Date.now() - 60 * MIN),
      finishedAt: new Date(Date.now() - 59 * MIN),
    });
    await seedImportRun(db, accountId, {
      status: "done",
      imported: 12,
      startedAt: new Date(finished.getTime() - 20_000),
      finishedAt: finished,
    });
    const a = await only();
    expect(a.lastCheck).toEqual({ at: finished.toISOString(), found: 12 });
    expect(a.lastRunFailed).toBe(false);
  });

  it("a newer failed run is reported, keeping the last good check", async () => {
    const finished = new Date(Date.now() - 30 * MIN);
    await seedImportRun(db, accountId, {
      status: "done",
      imported: 2,
      startedAt: new Date(finished.getTime() - 1000),
      finishedAt: finished,
    });
    await seedImportRun(db, accountId, {
      status: "failed",
      startedAt: new Date(Date.now() - 10 * MIN),
      finishedAt: new Date(Date.now() - 9 * MIN),
    });
    const a = await only();
    expect(a.lastRunFailed).toBe(true);
    expect(a.lastCheck?.found).toBe(2);
  });

  it("states when the next manual sync is allowed", async () => {
    const created = new Date(Date.now() - 30_000);
    await importJob({ state: "done", createdAt: created, runAt: created });
    const a = await only();
    expect(a.nextManualAt).toBe(new Date(created.getTime() + 2 * MIN).toISOString());
  });

  it("profile updating while build-profile is queued or running", async () => {
    await seedJob(db, {
      kind: "build-profile",
      dedupeKey: `profile:${accountId}:rapid`,
      userKey: accountId,
      runAt: new Date(Date.now() + 30_000),
    });
    expect((await only()).profileUpdating).toBe(true);
    await db.execute(sql`update jobs set state = 'done'`);
    expect((await only()).profileUpdating).toBe(false);
  });

  describe("ETA", () => {
    const queueAnalyses = async (ids: string[]) => {
      for (const g of ids) {
        await seedJob(db, { kind: "analyse-game", dedupeKey: `analyse:${g}`, userKey: accountId });
      }
    };

    it("none without a measured throughput", async () => {
      await queueAnalyses(await seedGames(db, accountId, "pending", 3));
      await seedAnalyseMetrics(db, 4);
      expect((await only()).eta).toBeNull();
    });

    it("old metrics are not a throughput", async () => {
      await queueAnalyses(await seedGames(db, accountId, "pending", 3));
      await seedAnalyseMetrics(db, 10, 11 * MIN);
      expect((await only()).eta).toBeNull();
    });

    it("queued jobs over the last 10 minutes' rate", async () => {
      await queueAnalyses(await seedGames(db, accountId, "pending", 3));
      await seedAnalyseMetrics(db, 6);
      // 6 analyses in 600 s is 0.01 a second; the last of 3 queued jobs is 300 s away.
      expect((await only()).eta).toEqual({ seconds: 300, basis: 6 });
    });

    it("none when a queued job has no exact place (delayed)", async () => {
      const [a, b] = await seedGames(db, accountId, "pending", 2);
      await queueAnalyses([a ?? ""]);
      await seedJob(db, {
        kind: "analyse-game",
        dedupeKey: `analyse:${b}`,
        userKey: accountId,
        runAt: new Date(Date.now() + 10 * MIN),
      });
      await seedAnalyseMetrics(db, 6);
      expect((await only()).eta).toBeNull();
    });

    it("none when nothing is queued", async () => {
      await seedGames(db, accountId, "done", 3);
      await seedAnalyseMetrics(db, 6);
      expect((await only()).eta).toBeNull();
    });
  });

  it("verified follows the user's own link", async () => {
    const { accountId: v } = await seedAccount(db, {
      userId: bob,
      username: "bob-verified",
      verified: true,
    });
    const status = await getSyncStatus(db, bob);
    expect(status.accounts.find((a) => a.accountId === v)?.verified).toBe(true);
  });

  it("lists only the user's own accounts", async () => {
    expect((await getSyncStatus(db, bob)).accounts).toEqual([]);
    const { accountId: other } = await seedAccount(db, { userId: bob, username: "bob-status" });
    await seedGames(db, other, "done", 2);
    const a = await only();
    expect(a.analysis.done).toBe(0);
  });
});

import { jobs } from "@ci/db/generated";
import { eq } from "drizzle-orm";
import { beforeEach, describe, expect, it } from "vitest";
import { requestRetry, requestSync, SYNC_COOLDOWN_MS } from "../../src/server/sync";
import { resetGameTables, seedAccount } from "../helpers/games";
import { createTestSession, resetAuthTables, webTestDb } from "../helpers/session";
import { resetSyncTables, seedGames, seedImportRun, seedJob } from "../helpers/sync";

const db = webTestDb();
const MIN = 60_000;
let alice: string;
let bob: string;
let accountId: string;
let second: string;

beforeEach(async () => {
  await resetAuthTables(db);
  await resetGameTables(db);
  await resetSyncTables(db);
  alice = (await createTestSession(db, { email: "alice@example.invalid" })).user.id;
  bob = (await createTestSession(db, { email: "bob@example.invalid" })).user.id;
  ({ accountId } = await seedAccount(db, { userId: alice, username: "alice-sync" }));
  ({ accountId: second } = await seedAccount(db, {
    userId: alice,
    platform: "lichess",
    username: "alice-sync-li",
  }));
});

const importJobs = () => db.select().from(jobs).where(eq(jobs.kind, "import-account"));

describe("requestSync", () => {
  it("enqueues an incremental import with the Rust kind, key and payload", async () => {
    const res = await requestSync(db, alice, accountId);
    expect(res).toEqual({ kind: "ok", accounts: [{ accountId, outcome: "queued" }] });
    const rows = await importJobs();
    expect(rows).toHaveLength(1);
    expect(rows[0]).toMatchObject({
      kind: "import-account",
      payload: { accountId, incremental: true },
      dedupeKey: `import:${accountId}`,
      userKey: accountId,
      priority: 100,
      maxAttempts: 3,
      state: "queued",
    });
  });

  it("a second sync while one is live is already checking", async () => {
    await requestSync(db, alice, accountId);
    const res = await requestSync(db, alice, accountId);
    expect(res).toEqual({ kind: "ok", accounts: [{ accountId, outcome: "already" }] });
    expect(await importJobs()).toHaveLength(1);
  });

  it("concurrent syncs create one job", async () => {
    const results = await Promise.all([
      requestSync(db, alice, accountId),
      requestSync(db, alice, accountId),
      requestSync(db, alice, accountId),
    ]);
    const outcomes = results.flatMap((r) => (r.kind === "ok" ? r.accounts : []));
    expect(outcomes.filter((o) => o.outcome === "queued")).toHaveLength(1);
    expect(await importJobs()).toHaveLength(1);
  });

  it("is throttled for 2 minutes after the last import started", async () => {
    const now = new Date();
    const started = new Date(now.getTime() - 1 * MIN);
    await seedJob(db, {
      kind: "import-account",
      dedupeKey: `import:${accountId}`,
      userKey: accountId,
      state: "done",
      createdAt: started,
      runAt: started,
    });
    const res = await requestSync(db, alice, accountId);
    expect(res).toEqual({
      kind: "ok",
      accounts: [
        {
          accountId,
          outcome: "throttled",
          availableAt: new Date(started.getTime() + SYNC_COOLDOWN_MS),
        },
      ],
    });
    expect(SYNC_COOLDOWN_MS).toBe(2 * MIN);
    expect(await importJobs()).toHaveLength(1);
  });

  it("a run started by the worker counts toward the 2 minutes", async () => {
    const now = new Date();
    await seedImportRun(db, accountId, {
      status: "done",
      imported: 4,
      startedAt: new Date(now.getTime() - 30_000),
      finishedAt: new Date(now.getTime() - 10_000),
    });
    const res = await requestSync(db, alice, accountId);
    expect(res.kind === "ok" && res.accounts[0]?.outcome).toBe("throttled");
  });

  it("is allowed again once 2 minutes have passed", async () => {
    const now = new Date();
    const started = new Date(now.getTime() - 2 * MIN - 1000);
    await seedJob(db, {
      kind: "import-account",
      dedupeKey: `import:${accountId}`,
      userKey: accountId,
      state: "done",
      createdAt: started,
      runAt: started,
    });
    const res = await requestSync(db, alice, accountId);
    expect(res.kind === "ok" && res.accounts[0]?.outcome).toBe("queued");
    expect(await importJobs()).toHaveLength(2);
  });

  it("never pulls a held-back import forward (backoff, breaker, busy)", async () => {
    const runAt = new Date(Date.now() + 10 * MIN);
    const id = await seedJob(db, {
      kind: "import-account",
      dedupeKey: `import:${accountId}`,
      userKey: accountId,
      state: "queued",
      createdAt: new Date(Date.now() - 20 * MIN),
      runAt,
    });
    const res = await requestSync(db, alice, accountId);
    expect(res).toEqual({ kind: "ok", accounts: [{ accountId, outcome: "already" }] });
    const rows = await importJobs();
    expect(rows).toHaveLength(1);
    expect(Number(rows[0]?.id)).toBe(id);
    expect(rows[0]?.runAt.getTime()).toBe(runAt.getTime());
  });

  it("syncs every linked account when no account is named", async () => {
    const res = await requestSync(db, alice, null);
    expect(res.kind).toBe("ok");
    if (res.kind !== "ok") return;
    expect(res.accounts.map((a) => a.accountId).sort()).toEqual([accountId, second].sort());
    expect(res.accounts.every((a) => a.outcome === "queued")).toBe(true);
    expect(await importJobs()).toHaveLength(2);
  });

  it("another user's account is not found and enqueues nothing", async () => {
    expect(await requestSync(db, bob, accountId)).toEqual({ kind: "not_found" });
    expect(await requestSync(db, bob, null)).toEqual({ kind: "not_found" });
    expect(await importJobs()).toHaveLength(0);
  });
});

describe("requestRetry", () => {
  const analyseJobs = () => db.select().from(jobs).where(eq(jobs.kind, "analyse-game"));

  it("enqueues an analysis for each failed game only", async () => {
    const failed = await seedGames(db, accountId, "failed", 2);
    await seedGames(db, accountId, "done", 1);
    await seedGames(db, accountId, "pending", 1);
    const res = await requestRetry(db, alice, accountId);
    expect(res).toEqual({ kind: "ok", queued: 2 });
    const rows = await analyseJobs();
    expect(rows.map((r) => r.dedupeKey).sort()).toEqual(failed.map((g) => `analyse:${g}`).sort());
    expect(rows[0]).toMatchObject({
      kind: "analyse-game",
      userKey: accountId,
      priority: 100,
      maxAttempts: 3,
      state: "queued",
    });
    expect(failed.map((g) => ({ gameId: g }))).toContainEqual(rows[0]?.payload);
  });

  it("does not retry a game again within 2 minutes", async () => {
    await seedGames(db, accountId, "failed", 1);
    expect(await requestRetry(db, alice, accountId)).toEqual({ kind: "ok", queued: 1 });
    await db.update(jobs).set({ state: "failed" });
    expect(await requestRetry(db, alice, accountId)).toEqual({ kind: "ok", queued: 0 });
    expect(await analyseJobs()).toHaveLength(1);
  });

  it("another user's account is not found", async () => {
    await seedGames(db, accountId, "failed", 1);
    expect(await requestRetry(db, bob, accountId)).toEqual({ kind: "not_found" });
    expect(await analyseJobs()).toHaveLength(0);
  });
});

import { type Db, games, importRuns, linkedAccounts } from "@ci/db";
import { eq } from "drizzle-orm";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { CircuitOpenError, createCircuitBreaker } from "../../src/http/circuit-breaker.js";
import {
  PlatformNotFoundError,
  PlatformRateLimitError,
  PlatformTimeoutError,
} from "../../src/http/platform-client.js";
import type { NormalizedGame } from "../../src/importers/types.js";
import { JobDeadlineError } from "../../src/jobs/deadline.js";
import {
  handleImportAccount,
  type ImportCursor,
  importErrorCode,
} from "../../src/jobs/import-account.js";
import { getWorkerTestDb, workerTestDbConfigured } from "../db.js";
import { insertAccount, makeGame } from "../jobs/helpers.js";

interface Opts {
  since?: Date;
  until?: Date;
}

/** A fake importer that honours since/until (both inclusive) and records what it was asked. */
function fakeImporter(list: NormalizedGame[], calls: Opts[], failAfter?: number) {
  return async function* (_client: unknown, _username: string, opts: Opts = {}) {
    calls.push({ ...opts });
    let n = 0;
    for (const g of list) {
      if (opts.since && g.playedAt < opts.since) continue;
      if (opts.until && g.playedAt > opts.until) continue;
      if (failAfter !== undefined && n === failAfter) throw new Error("connection reset");
      yield g;
      n++;
    }
  };
}

describe.skipIf(!workerTestDbConfigured())("import recovery", () => {
  let db: Db;
  beforeAll(async () => {
    db = (await getWorkerTestDb()) as Db;
  });
  afterAll(async () => {
    await db.$client.end();
  });

  // Newest first, as both platforms stream.
  const fourGames = (name: string) =>
    [4, 3, 2, 1].map((d) =>
      makeGame({
        white: { name, rating: 1 },
        playedAt: new Date(`2026-03-0${d}T12:00:00Z`),
      }),
    );

  function deps(importer: ReturnType<typeof fakeImporter>, enqueued: string[] = []) {
    return {
      db,
      importers: { lichess: importer, chesscom: importer } as never,
      clients: { lichess: {}, chesscom: {} } as never,
      enqueueAnalyse: async (id: string) => {
        enqueued.push(id);
      },
      // Exact cursors here; the Lichess look-back has its own test.
      lichessSinceLookbackDays: 0,
    };
  }

  const cursorOf = async (accountId: string) => {
    const [row] = await db.select().from(linkedAccounts).where(eq(linkedAccounts.id, accountId));
    return row?.importCursor as ImportCursor | null;
  };

  it("import resumes from cursor", async () => {
    const acct = await insertAccount(db, "resumer");
    const list = fourGames("Resumer");

    const firstCalls: Opts[] = [];
    await expect(
      handleImportAccount(deps(fakeImporter(list, firstCalls, 2)), { accountId: acct.id }),
    ).rejects.toThrow("connection reset");

    expect(await cursorOf(acct.id)).toEqual({
      oldestReached: list[1]?.playedAt.toISOString(),
      newestSeenAt: null,
    });
    expect(await db.select().from(games).where(eq(games.accountId, acct.id))).toHaveLength(2);
    const [failedRun] = await db.select().from(importRuns).where(eq(importRuns.accountId, acct.id));
    expect(failedRun).toMatchObject({
      errorCode: "unknown",
      status: "failed",
      imported: 2,
      lastError: "connection reset",
    });
    expect(failedRun?.finishedAt).toBeInstanceOf(Date);

    const calls: Opts[] = [];
    const res = await handleImportAccount(deps(fakeImporter(list, calls)), { accountId: acct.id });
    expect(res.imported).toBe(2);
    // First the unfinished range below the cursor, then new games above everything imported.
    expect(calls[0]).toEqual({ until: list[1]?.playedAt });
    expect(calls[1]).toEqual({ since: list[0]?.playedAt });
    expect(calls).toHaveLength(2);

    const rows = await db.select().from(games).where(eq(games.accountId, acct.id));
    expect(rows).toHaveLength(4);
    expect(new Set(rows.map((r) => r.platformGameId)).size).toBe(4);
    expect(await cursorOf(acct.id)).toEqual({
      oldestReached: null,
      newestSeenAt: list[0]?.playedAt.toISOString(),
    });

    const runs = await db.select().from(importRuns).where(eq(importRuns.accountId, acct.id));
    expect(runs.map((r) => r.status).sort()).toEqual(["done", "failed"]);

    // A later sync only asks for games since the newest one seen.
    const later: Opts[] = [];
    await handleImportAccount(deps(fakeImporter(list, later)), { accountId: acct.id });
    expect(later).toEqual([{ since: list[0]?.playedAt }]);
  });

  it("checkpoints the cursor and the run every batch", async () => {
    const acct = await insertAccount(db, "batcher");
    const list = fourGames("Batcher");
    let seen: { cursor: ImportCursor | null; imported: number | undefined } | undefined;
    const importer = async function* () {
      yield list[0] as NormalizedGame;
      yield list[1] as NormalizedGame;
      // Before the 3rd game the first batch of 2 must already be durable.
      const [run] = await db.select().from(importRuns).where(eq(importRuns.accountId, acct.id));
      seen = { cursor: await cursorOf(acct.id), imported: run?.imported };
      yield list[2] as NormalizedGame;
    };
    await handleImportAccount(
      { ...deps(importer as never), checkpointEvery: 2 },
      { accountId: acct.id },
    );
    expect(seen).toEqual({
      cursor: { oldestReached: list[1]?.playedAt.toISOString(), newestSeenAt: null },
      imported: 2,
    });
  });

  it("re-import enqueues every game that is still pending", async () => {
    const acct = await insertAccount(db, "requeue");
    const list = fourGames("Requeue").slice(0, 2);
    const first: string[] = [];
    await handleImportAccount(deps(fakeImporter(list, []), first), { accountId: acct.id });
    // Simulate a crash between insert and enqueue: reset the cursor and re-run.
    await db
      .update(linkedAccounts)
      .set({ importCursor: null })
      .where(eq(linkedAccounts.id, acct.id));
    const second: string[] = [];
    const res = await handleImportAccount(deps(fakeImporter(list, []), second), {
      accountId: acct.id,
    });
    expect(res.imported).toBe(0);
    expect(second.sort()).toEqual(first.sort());
  });

  it("calls the platform through the account's circuit breaker", async () => {
    const acct = await insertAccount(db, "tripped");
    const breaker = createCircuitBreaker({ failureThreshold: 1, cooldownMs: 60_000 });
    await breaker.exec(() => Promise.reject(new Error("503"))).catch(() => {});
    expect(breaker.state()).toBe("open");

    let platformCalls = 0;
    const client = {
      getJson: async () => {
        platformCalls++;
        return {};
      },
      getNdjson: async function* () {
        platformCalls++;
        yield* [];
      },
    };
    const importer = async function* (c: typeof client) {
      await c.getJson();
      yield* [] as NormalizedGame[];
    };
    await expect(
      handleImportAccount(
        {
          ...deps(importer as never),
          clients: { lichess: client, chesscom: client } as never,
          breakers: { lichess: breaker, chesscom: breaker },
        },
        { accountId: acct.id },
      ),
    ).rejects.toBeInstanceOf(CircuitOpenError);
    expect(platformCalls).toBe(0);
  });

  it("re-fetches the last 30 days on Lichess, so late-finishing correspondence games land", async () => {
    const acct = await insertAccount(db, "corres");
    const newer = makeGame({
      white: { name: "Corres", rating: 1 },
      playedAt: new Date("2026-03-20T00:00:00Z"),
    });
    // Created earlier, but only finished (and so only listed) after "newer" was stored.
    const correspondence = makeGame({
      white: { name: "Corres", rating: 1 },
      playedAt: new Date("2026-03-01T00:00:00Z"),
      timeClass: "daily",
    });
    const base = { ...deps(fakeImporter([newer], [])) };
    const { lichessSinceLookbackDays: _, ...defaults } = base;
    await handleImportAccount(defaults, { accountId: acct.id });

    const calls: Opts[] = [];
    const next = { ...deps(fakeImporter([newer, correspondence], calls)) };
    const { lichessSinceLookbackDays: __, ...nextDefaults } = next;
    const res = await handleImportAccount(nextDefaults, { accountId: acct.id });
    expect(calls).toEqual([{ since: new Date("2026-02-18T00:00:00Z") }]);
    expect(res.imported).toBe(1);
    expect(await db.select().from(games).where(eq(games.accountId, acct.id))).toHaveLength(2);
    // The watermark itself still records the newest game, not the look-back.
    expect((await cursorOf(acct.id))?.newestSeenAt).toBe(newer.playedAt.toISOString());

    // The look-back is configurable.
    const custom: Opts[] = [];
    await handleImportAccount(
      { ...deps(fakeImporter([], custom)), lichessSinceLookbackDays: 1 },
      { accountId: acct.id },
    );
    expect(custom).toEqual([{ since: new Date("2026-03-19T00:00:00Z") }]);
  });

  it("records an error code from the typed platform error", async () => {
    expect(importErrorCode(new PlatformNotFoundError("u"))).toBe("not_found");
    expect(importErrorCode(new PlatformRateLimitError("u", 3))).toBe("rate_limited");
    expect(importErrorCode(new PlatformTimeoutError("u", 10))).toBe("timeout");
    expect(importErrorCode(new JobDeadlineError("import-account", 10))).toBe("timeout");
    expect(importErrorCode(new CircuitOpenError(10))).toBe("circuit_open");
    expect(importErrorCode(new Error("?"))).toBe("unknown");

    const acct = await insertAccount(db, "limited");
    const importer = async function* (): AsyncIterable<NormalizedGame> {
      yield* [];
      throw new PlatformRateLimitError("https://lichess.org/api/games/user/limited", 4);
    };
    await expect(
      handleImportAccount(deps(importer as never), { accountId: acct.id }),
    ).rejects.toBeInstanceOf(PlatformRateLimitError);
    const [run] = await db.select().from(importRuns).where(eq(importRuns.accountId, acct.id));
    expect(run).toMatchObject({ status: "failed", errorCode: "rate_limited" });
  });
});

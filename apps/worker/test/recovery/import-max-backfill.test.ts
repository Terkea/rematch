import { type Db, games, linkedAccounts } from "@ci/db";
import { eq } from "drizzle-orm";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import type { NormalizedGame } from "../../src/importers/types.js";
import { handleImportAccount } from "../../src/jobs/import-account.js";
import { getWorkerTestDb, workerTestDbConfigured } from "../db.js";
import { insertAccount, makeGame } from "../jobs/helpers.js";

interface Opts {
  since?: Date;
  until?: Date;
  max?: number;
}

/** Honours since/until (inclusive) and max, like the real importers. */
function fakeImporter(list: NormalizedGame[]) {
  return async function* (_client: unknown, _username: string, opts: Opts = {}) {
    let n = 0;
    for (const g of list) {
      if (opts.max !== undefined && n >= opts.max) return;
      if (opts.since && g.playedAt < opts.since) continue;
      if (opts.until && g.playedAt > opts.until) continue;
      yield g;
      n++;
    }
  };
}

describe.skipIf(!workerTestDbConfigured())("import --max backfill", () => {
  let db: Db;
  beforeAll(async () => {
    db = (await getWorkerTestDb()) as Db;
  });
  afterAll(async () => {
    await db.$client.end();
  });

  const history = (name: string, count: number) =>
    Array.from({ length: count }, (_, i) =>
      makeGame({
        white: { name, rating: 1 },
        playedAt: new Date(Date.UTC(2026, 2, 1) - i * 3600_000),
      }),
    );

  const run = (list: NormalizedGame[], accountId: string, max: number) => {
    const importer = fakeImporter(list);
    return handleImportAccount(
      {
        db,
        importers: { lichess: importer, chesscom: importer },
        clients: {} as never,
        enqueueAnalyse: async () => {},
      },
      { accountId, max },
    );
  };

  it("a later, larger --max continues below the games a smaller run imported", async () => {
    const acct = await insertAccount(db, "maxbackfill", "chesscom");
    const list = history("maxbackfill", 10);

    expect((await run(list, acct.id, 3)).imported).toBe(3);
    expect((await run(list, acct.id, 8)).imported).toBe(5); // 8 in total
    expect((await run(list, acct.id, 50)).imported).toBe(2); // history ends

    const rows = await db.select().from(games).where(eq(games.accountId, acct.id));
    expect(rows).toHaveLength(10);

    // History ended: the frontier is released and the watermark is the newest game.
    const [after] = await db.select().from(linkedAccounts).where(eq(linkedAccounts.id, acct.id));
    expect(after?.importCursor?.oldestReached).toBeNull();
    expect(after?.importCursor?.newestSeenAt).toBe(list[0]?.playedAt.toISOString());
  });

  // Games played after the first run, newest first, above the whole history.
  const newGames = (name: string, count: number) =>
    Array.from({ length: count }, (_, i) =>
      makeGame({
        white: { name, rating: 1 },
        playedAt: new Date(Date.UTC(2026, 2, 2) + (count - i) * 3600_000),
      }),
    );

  it("a pending backfill does not starve newly played games", async () => {
    const acct = await insertAccount(db, "maxnewfirst", "chesscom");
    const old = history("maxnewfirst", 10);
    expect((await run(old, acct.id, 3)).imported).toBe(3);

    const list = [...newGames("maxnewfirst", 2), ...old];
    expect((await run(list, acct.id, 3)).imported).toBe(2);

    const rows = await db.select().from(games).where(eq(games.accountId, acct.id));
    expect(rows).toHaveLength(5);
  });

  it("new games that use up the budget leave the backfill frontier for the next run", async () => {
    const acct = await insertAccount(db, "maxwaits", "chesscom");
    const old = history("maxwaits", 10);
    expect((await run(old, acct.id, 3)).imported).toBe(3);
    const [before] = await db.select().from(linkedAccounts).where(eq(linkedAccounts.id, acct.id));

    const list = [...newGames("maxwaits", 5), ...old];
    expect((await run(list, acct.id, 3)).imported).toBe(3);
    const [mid] = await db.select().from(linkedAccounts).where(eq(linkedAccounts.id, acct.id));
    expect(mid?.importCursor?.oldestReached).toBe(before?.importCursor?.oldestReached);
    expect(mid?.importCursor?.backfill).toBe(true);

    // 2 new games left, then the 7 older ones.
    expect((await run(list, acct.id, 20)).imported).toBe(9);
    const rows = await db.select().from(games).where(eq(games.accountId, acct.id));
    expect(rows).toHaveLength(15);
    const [after] = await db.select().from(linkedAccounts).where(eq(linkedAccounts.id, acct.id));
    expect(after?.importCursor?.oldestReached).toBeNull();
    expect(after?.importCursor?.backfill).toBeFalsy();
  });
});

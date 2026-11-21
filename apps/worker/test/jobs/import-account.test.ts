import { type Db, games, linkedAccounts } from "@ci/db";
import { UnrecoverableError } from "bullmq";
import { eq } from "drizzle-orm";
import { beforeAll, describe, expect, it } from "vitest";
import type { NormalizedGame } from "../../src/importers/types.js";
import { handleImportAccount } from "../../src/jobs/import-account.js";
import { getWorkerTestDb, workerTestDbConfigured } from "../db.js";
import { insertAccount, makeGame } from "./helpers.js";

describe.skipIf(!workerTestDbConfigured())("handleImportAccount", () => {
  let db: Db;
  beforeAll(async () => {
    db = (await getWorkerTestDb()) as Db;
  });

  function deps(list: NormalizedGame[], enqueued: string[]) {
    const fake = async function* () {
      for (const g of list) yield g;
    };
    return {
      db,
      importers: { lichess: fake, chesscom: fake },
      clients: {} as never,
      enqueueAnalyse: async (id: string) => {
        enqueued.push(id);
      },
    };
  }

  it("import twice yields same rows; still-pending games are re-enqueued (jobId dedupes)", async () => {
    const acct = await insertAccount(db, "twice");
    const list = [
      makeGame({ white: { name: "Twice", rating: 1 } }),
      makeGame({ black: { name: "TWICE", rating: 1 } }),
    ];
    const first: string[] = [];
    const second: string[] = [];
    expect(await handleImportAccount(deps(list, first), { accountId: acct.id })).toEqual({
      imported: 2,
      skipped: 0,
    });
    expect(await handleImportAccount(deps(list, second), { accountId: acct.id })).toEqual({
      imported: 0,
      skipped: 0,
    });
    const rows = await db.select().from(games).where(eq(games.accountId, acct.id));
    expect(rows).toHaveLength(2);
    expect(first).toHaveLength(2);
    expect(second.sort()).toEqual(first.sort());
    expect(rows.every((r) => r.analysisStatus === "pending")).toBe(true);
  });

  it("sets playerColor case-insensitively and skips games without the user", async () => {
    const acct = await insertAccount(db, "colour");
    const list = [
      makeGame({ white: { name: "COLOUR", rating: 1 } }),
      makeGame({ black: { name: "Colour", rating: 1 } }),
      makeGame({}),
    ];
    const res = await handleImportAccount(deps(list, []), { accountId: acct.id });
    expect(res).toEqual({ imported: 2, skipped: 1 });
    const rows = await db.select().from(games).where(eq(games.accountId, acct.id));
    expect(rows.map((r) => r.playerColor).sort()).toEqual(["black", "white"]);
  });

  it("updates lastSyncedAt", async () => {
    const acct = await insertAccount(db, "synced");
    expect(acct.lastSyncedAt).toBeNull();
    await handleImportAccount(deps([], []), { accountId: acct.id });
    const [row] = await db.select().from(linkedAccounts).where(eq(linkedAccounts.id, acct.id));
    expect(row?.lastSyncedAt).toBeInstanceOf(Date);
  });

  it("a missing account throws UnrecoverableError", async () => {
    await expect(
      handleImportAccount(deps([], []), { accountId: "00000000-0000-4000-8000-000000000000" }),
    ).rejects.toBeInstanceOf(UnrecoverableError);
  });
});

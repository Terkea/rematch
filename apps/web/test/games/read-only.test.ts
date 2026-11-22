import { linkedAccounts } from "@ci/db";
import { beforeAll, describe, expect, it } from "vitest";
import { readOnly } from "../../src/server/games";
import { resetGameTables } from "../helpers/games";
import { resetAuthTables, webTestDb } from "../helpers/session";

const db = webTestDb();

beforeAll(async () => {
  await resetAuthTables(db);
  await resetGameTables(db);
});

describe("readOnly", () => {
  it("rejects a write inside the transaction", async () => {
    const write = readOnly(db, (tx) =>
      tx.insert(linkedAccounts).values({ platform: "chesscom", username: "x", displayName: "x" }),
    );
    await expect(write).rejects.toMatchObject({ cause: { code: "25006" } });
    expect(await db.select().from(linkedAccounts)).toEqual([]);
  });

  it("returns what the read returns", async () => {
    expect(await readOnly(db, async (tx) => (await tx.select().from(linkedAccounts)).length)).toBe(
      0,
    );
  });
});

import { accountLinks, games, linkedAccounts } from "@ci/db";
import { eq, sql } from "drizzle-orm";
import { beforeEach, describe, expect, it } from "vitest";
import {
  assertLocal,
  devLinkAccount,
  isLocalServerAddress,
  linkedMessage,
} from "../../scripts/dev-link-account";
import { resetGameTables, seedAccount, seedSyntheticGame } from "../helpers/games";
import { createTestSession, resetAuthTables, webTestDb } from "../helpers/session";

const db = webTestDb();
const LOCAL = { DATABASE_URL: "postgres://postgres:postgres@postgres:5432/chess" };
const EMAIL = "dev-link@example.invalid";

async function count(table: typeof games | typeof linkedAccounts | typeof accountLinks) {
  const [row] = await db.select({ n: sql<number>`count(*)::int` }).from(table);
  return row?.n ?? 0;
}

let accountId: string;
let userId: string;

beforeEach(async () => {
  await resetAuthTables(db);
  await resetGameTables(db);
  ({ accountId } = await seedAccount(db, { username: "SomePlayer" }));
  for (const day of [1, 2, 3]) {
    await seedSyntheticGame(db, accountId, {
      playedAt: `2026-10-0${day}T10:00:00.000Z`,
      timeClass: "rapid",
      result: "1-0",
    });
  }
  ({
    user: { id: userId },
  } = await createTestSession(db, { email: EMAIL }));
});

describe("devLinkAccount guards", () => {
  const input = { email: EMAIL, platform: "chesscom" as const, username: "someplayer" };

  it("refuses in production", async () => {
    await expect(devLinkAccount(db, input, { ...LOCAL, NODE_ENV: "production" })).rejects.toThrow(
      "dev:link is for local development only",
    );
    expect(await count(accountLinks)).toBe(0);
  });

  it("refuses a database on a non-local host", async () => {
    await expect(
      devLinkAccount(db, input, { DATABASE_URL: "postgres://u@db.example.com/x" }),
    ).rejects.toThrow("dev:link is for local development only");
    await expect(
      devLinkAccount(db, input, { DATABASE_URL: "postgres://u@postgres.example.com/x" }),
    ).rejects.toThrow("dev:link is for local development only");
    await expect(devLinkAccount(db, input, { DATABASE_URL: "not a url" })).rejects.toThrow(
      "dev:link is for local development only",
    );
    expect(await count(accountLinks)).toBe(0);
  });

  // Review I1: the guard must judge the host postgres.js will actually connect to.
  it.each([
    "postgres://u@localhost/x?host=evil.com",
    "postgres://u@localhost#@evil.com/x",
    "postgres://u@localhost/x#fragment",
    "postgres://u@127.0.0.1/x?host=db.example.com&port=6543",
    "postgres://u@localhost/x?host=/cloudsql/p:r:i",
    "postgres://u@localhost/x?hostaddr=203.0.113.7",
    "postgres://u@postgres:5432,evil.com/x",
    "postgres://localhost@evil.com/x",
    "postgres://u@notlocalhost/x",
    "postgres://u@evilpostgres/x",
    "postgres://u@my127.0.0.1/x",
    "postgres://u@localhost.evil.com/x",
  ])("refuses the driver-level bypass %s", async (url) => {
    await expect(devLinkAccount(db, input, { DATABASE_URL: url })).rejects.toThrow(
      "dev:link is for local development only",
    );
    expect(() => assertLocal({ DATABASE_URL: url })).toThrow(
      "dev:link is for local development only",
    );
    expect(await count(accountLinks)).toBe(0);
  });

  it("accepts a password with an @ only when the driver still connects locally", () => {
    // Percent-encoded userinfo is fine; a raw second @ is refused outright.
    expect(() => assertLocal({ DATABASE_URL: "postgres://u:p%40ss@localhost/x" })).not.toThrow();
    expect(() => assertLocal({ DATABASE_URL: "postgres://u:p@ss@localhost/x" })).toThrow(
      "dev:link is for local development only",
    );
  });

  it("accepts postgres, localhost and 127.0.0.1", async () => {
    for (const host of ["postgres:5432", "localhost:55432", "127.0.0.1"]) {
      const env = { NODE_ENV: "development", DATABASE_URL: `postgres://u:p@${host}/chess` };
      await expect(devLinkAccount(db, input, env)).resolves.toMatchObject({ games: 3 });
    }
  });

  it("names a missing user", async () => {
    await expect(
      devLinkAccount(db, { ...input, email: "nobody@example.invalid" }, LOCAL),
    ).rejects.toThrow(/^No user with that email\. Sign in once first\.$/);
  });

  it("names a missing imported account", async () => {
    await expect(devLinkAccount(db, { ...input, username: "otherplayer" }, LOCAL)).rejects.toThrow(
      /^No imported account chesscom\/otherplayer\. Import it with the worker first\.$/,
    );
  });
});

describe("the connected server", () => {
  it("counts loopback, private and socket addresses as local", () => {
    for (const addr of [
      null,
      "127.0.0.1",
      "127.1.2.3",
      "::1",
      "10.0.0.5",
      "172.18.0.3",
      "192.168.1.9",
      "fd12::1",
      "::ffff:172.18.0.3",
    ]) {
      expect(isLocalServerAddress(addr), String(addr)).toBe(true);
    }
  });

  it("counts public addresses as remote", () => {
    for (const addr of [
      "203.0.113.7",
      "8.8.8.8",
      "172.32.0.1",
      "172.15.255.255",
      "11.0.0.1",
      "2001:db8::1",
      "::ffff:8.8.8.8",
      "100.64.0.1",
    ]) {
      expect(isLocalServerAddress(addr), addr).toBe(false);
    }
  });

  it("refuses after connecting when the server itself is remote, before any read or write", async () => {
    const calls: string[] = [];
    const remote = {
      execute: async () => {
        calls.push("execute");
        return [{ address: "203.0.113.7" }];
      },
    } as unknown as typeof db;
    await expect(
      devLinkAccount(remote, { email: EMAIL, platform: "chesscom", username: "someplayer" }, LOCAL),
    ).rejects.toThrow("dev:link is for local development only");
    expect(calls).toEqual(["execute"]);
  });

  it("checks the server it is connected to (the compose service here)", async () => {
    // The test database is the compose `postgres` service on a private bridge network.
    await expect(
      devLinkAccount(db, { email: EMAIL, platform: "chesscom", username: "someplayer" }, LOCAL),
    ).resolves.toMatchObject({ linked: true });
  });
});

describe("devLinkAccount", () => {
  it("links an existing account, unverified, once, and writes nothing else", async () => {
    const before = { games: await count(games), accounts: await count(linkedAccounts) };
    const input = {
      email: "Dev-Link@Example.invalid",
      platform: "chesscom" as const,
      username: "SomePlayer",
    };

    expect(await devLinkAccount(db, input, LOCAL)).toEqual({ linked: true, games: 3 });
    const rows = await db.select().from(accountLinks).where(eq(accountLinks.userId, userId));
    expect(rows).toHaveLength(1);
    expect(rows[0]).toMatchObject({
      linkedAccountId: accountId,
      method: "username",
      verifiedAt: null,
    });

    expect(await devLinkAccount(db, input, LOCAL)).toEqual({ linked: false, games: 3 });
    expect(await count(accountLinks)).toBe(1);
    expect({ games: await count(games), accounts: await count(linkedAccounts) }).toEqual(before);
    const [account] = await db
      .select()
      .from(linkedAccounts)
      .where(eq(linkedAccounts.id, accountId));
    expect(account?.userId).toBeNull();
    expect(account?.verifiedAt).toBeNull();
  });

  it("prints the CLI line", () => {
    expect(linkedMessage("chesscom", "otherplayer", 500)).toBe(
      "Linked chesscom/otherplayer (500 games) to your account.",
    );
    expect(linkedMessage("chesscom", "otherplayer", 1)).toBe(
      "Linked chesscom/otherplayer (1 game) to your account.",
    );
  });
});

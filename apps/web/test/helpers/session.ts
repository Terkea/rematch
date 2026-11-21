import { randomBytes, randomUUID } from "node:crypto";
import {
  assertSafeTestDatabase,
  createDb,
  type Db,
  invites,
  runMigrations,
  session,
  user,
} from "@ci/db";
import { makeSignature } from "better-auth/crypto";
import { sql } from "drizzle-orm";
import { resetEnvCache } from "../../src/env";
import { resetAuth } from "../../src/server/auth";
import type { AppUser } from "../../src/server/auth/session";

let testDb: Db | undefined;

/**
 * The web test database (`WEB_DATABASE_URL_TEST`, guarded by `assertSafeTestDatabase`). It also
 * points the app's own `DATABASE_URL` at it, so `getAuth()` and every route under test use it.
 * Call it at the top level of a test file, before the first request.
 */
export function webTestDb(): Db {
  if (testDb) return testDb;
  const url = assertSafeTestDatabase(process.env.WEB_DATABASE_URL_TEST, process.env.DATABASE_URL);
  process.env.DATABASE_URL = url;
  resetEnvCache();
  resetAuth();
  testDb = createDb(url);
  return testDb;
}

let migrated = false;

/** Migrates once per file (db/migrations, ADR 0033) and empties the auth tables. */
export async function resetAuthTables(db: Db): Promise<void> {
  if (!migrated) {
    await runMigrations(db);
    migrated = true;
  }
  await db.execute(sql`truncate table "user", verification, invites, magic_link_sends cascade`);
}

/** Adds an address to the invite allow-list (ADR-WEB-3). */
export async function inviteEmail(db: Db, email: string): Promise<void> {
  await db.insert(invites).values({ email: email.toLowerCase() }).onConflictDoNothing();
}

/**
 * A signed-in user without going through a sign-in flow. Defaults to an adult who has answered
 * the age screen (`birthMonth` "1990-01"); pass `{ birthMonth: null }` for a fresh account.
 * `cookie` is a ready `Cookie` header value for the Better Auth session (`ci.session_token`).
 */
export async function createTestSession(
  db: Db,
  overrides: Partial<AppUser> = {},
): Promise<{ user: AppUser; cookie: string }> {
  const secret = process.env.BETTER_AUTH_SECRET;
  if (!secret) throw new Error("createTestSession: BETTER_AUTH_SECRET is not set");
  const [row] = await db
    .insert(user)
    .values({
      ...(overrides.id ? { id: overrides.id } : {}),
      name: overrides.name ?? "Test player",
      email: overrides.email ?? `player-${randomUUID()}@example.invalid`,
      emailVerified: true,
      birthMonth: overrides.birthMonth === undefined ? "1990-01" : overrides.birthMonth,
      marketingEmail: overrides.marketingEmail ?? false,
      publicShareDefault: overrides.publicShareDefault ?? false,
    })
    .returning();
  if (!row) throw new Error("createTestSession: user insert returned no row");
  const token = randomBytes(24).toString("base64url");
  await db.insert(session).values({
    token,
    userId: row.id,
    expiresAt: new Date(Date.now() + 7 * 24 * 3600 * 1000),
  });
  const signature = await makeSignature(token, secret);
  return {
    user: {
      id: row.id,
      email: row.email,
      name: row.name,
      birthMonth: row.birthMonth,
      marketingEmail: row.marketingEmail,
      publicShareDefault: row.publicShareDefault,
    },
    cookie: `ci.session_token=${encodeURIComponent(`${token}.${signature}`)}`,
  };
}

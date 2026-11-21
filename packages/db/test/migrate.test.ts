import { createHash } from "node:crypto";
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { sql } from "drizzle-orm";
import postgres from "postgres";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { createDb, migrationsDir, runMigrations } from "../src/index.js";
import { assertSafeTestDatabase } from "../src/test-guard.js";

// A scratch *_test database of its own: chess_test is migrated by the Drizzle files in setup.ts.
const base = new URL(
  assertSafeTestDatabase(process.env.DATABASE_URL_TEST, process.env.DATABASE_URL),
);
const name = `chess_migrate_${process.pid}_test`;
const admin = postgres(base.toString(), { onnotice: () => {}, max: 1 });
const url = new URL(base);
url.pathname = `/${name}`;
let db: ReturnType<typeof createDb>;

beforeAll(async () => {
  await admin.unsafe(`DROP DATABASE IF EXISTS ${name}`);
  await admin.unsafe(`CREATE DATABASE ${name}`);
  db = createDb(url.toString());
});

afterAll(async () => {
  await db.$client.end();
  await admin.unsafe(`DROP DATABASE IF EXISTS ${name}`);
  await admin.end();
});

describe("runMigrations", () => {
  it("applies db/migrations, records them like sqlx and is idempotent", async () => {
    await runMigrations(db);
    await runMigrations(db);

    const rows = await db.$client<{ version: string; description: string; checksum: Buffer }[]>`
      select version::text as version, description, checksum from _sqlx_migrations order by version`;
    expect(rows.map((r) => r.version)).toContain("20261121010000");
    const auth = rows.find((r) => r.version === "20261121010000");
    expect(auth?.description).toBe("web auth");
    const file = readFileSync(`${migrationsDir()}/20261121010000_web_auth.sql`, "utf8");
    expect(
      Buffer.from(auth?.checksum ?? []).equals(createHash("sha384").update(file).digest()),
    ).toBe(true);

    const tables = await db.execute<{ t: string }>(
      sql`select table_name as t from information_schema.tables where table_schema = 'public'`,
    );
    const names = tables.map((r) => r.t);
    for (const t of ["user", "session", "account", "verification", "invites", "games"]) {
      expect(names).toContain(t);
    }
  });

  it("gives auth rows uuid ids and opt-in privacy defaults", async () => {
    const [u] = await db.$client<
      { id: string; marketing_email: boolean; public_share_default: boolean }[]
    >`insert into "user" (name, email) values ('', 'dev@example.invalid')
      returning id, marketing_email, public_share_default`;
    expect(u?.id).toMatch(/^[0-9a-f-]{36}$/);
    expect(u?.marketing_email).toBe(false);
    expect(u?.public_share_default).toBe(false);
  });

  it("cascades a deleted user's sessions and accounts", async () => {
    const [u] = await db.$client<{ id: string }[]>`
      insert into "user" (name, email) values ('', 'cascade@example.invalid') returning id`;
    const id = u?.id ?? "";
    await db.$client`insert into session (expires_at, token, user_id) values (now(), 't-cascade', ${id})`;
    await db.$client`insert into account (account_id, provider_id, user_id) values ('g1', 'google', ${id})`;
    await db.$client`delete from "user" where id = ${id}`;
    const [s] = await db.$client<{ n: number }[]>`
      select (select count(*) from session where user_id = ${id})::int
           + (select count(*) from account where user_id = ${id})::int as n`;
    expect(s?.n).toBe(0);
  });

  it("refuses a migration that changed after it was applied", async () => {
    const dir = mkdtempSync(`${tmpdir()}/mig-`);
    writeFileSync(`${dir}/20261121010000_web_auth.sql`, "select 1;");
    await expect(runMigrations(db, dir)).rejects.toThrow(/changed after it was applied/);
  });
});

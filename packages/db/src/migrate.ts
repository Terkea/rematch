import { createHash } from "node:crypto";
import { readdirSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import type { Db } from "./client.js";

/**
 * `db/migrations/` at the repository root (ADR 0033), from both `src/` and `dist/`. Built with
 * `join`, not `new URL(literal, import.meta.url)`, which bundlers would treat as an asset import.
 */
export function migrationsDir(): string {
  return join(dirname(fileURLToPath(import.meta.url)), "..", "..", "..", "db", "migrations");
}

/** Advisory lock key that serialises concurrent `runMigrations` calls (arbitrary, fixed). */
const MIGRATE_LOCK_KEY = 7_262_012;

type Migration = { version: bigint; description: string; sql: string; checksum: Buffer };

function loadMigrations(dir: string): Migration[] {
  return readdirSync(dir)
    .filter((f) => /^\d+_.+\.sql$/.test(f))
    .sort()
    .map((file) => {
      const match = /^(\d+)_(.+)\.sql$/.exec(file);
      if (!match?.[1] || !match[2]) throw new Error(`runMigrations: bad file name ${file}`);
      const sql = readFileSync(`${dir}/${file}`, "utf8");
      return {
        version: BigInt(match[1]),
        description: match[2].replaceAll("_", " "),
        sql,
        checksum: createHash("sha384").update(sql).digest(),
      };
    });
}

/**
 * Applies every `db/migrations/*.sql` not yet applied, in version order, in one
 * transaction (all or nothing). It keeps sqlx's own bookkeeping (`_sqlx_migrations`, same DDL, SHA-384 checksum,
 * description with `_` as spaces), so a database migrated here is accepted by `db::MIGRATOR` and
 * vice versa. A recorded migration whose checksum differs from the file is an error, as in sqlx.
 * Used for the web test database; the dev and production databases are migrated by the backend.
 */
export async function runMigrations(db: Db, dir: string = migrationsDir()): Promise<void> {
  const migrations = loadMigrations(dir);
  await db.$client.begin(async (tx) => {
    await tx`select pg_advisory_xact_lock(${MIGRATE_LOCK_KEY})`;
    await tx`CREATE TABLE IF NOT EXISTS _sqlx_migrations (
      version BIGINT PRIMARY KEY,
      description TEXT NOT NULL,
      installed_on TIMESTAMPTZ NOT NULL DEFAULT now(),
      success BOOLEAN NOT NULL,
      checksum BYTEA NOT NULL,
      execution_time BIGINT NOT NULL)`;
    const applied = new Map<string, { success: boolean; checksum: Buffer }>();
    for (const row of await tx<
      { version: string; success: boolean; checksum: Buffer }[]
    >`select version::text as version, success, checksum from _sqlx_migrations`) {
      applied.set(row.version, row);
    }
    for (const m of migrations) {
      const done = applied.get(m.version.toString());
      if (done) {
        if (!done.success) throw new Error(`runMigrations: migration ${m.version} is dirty`);
        if (!Buffer.from(done.checksum).equals(m.checksum)) {
          throw new Error(`runMigrations: migration ${m.version} was changed after it was applied`);
        }
        continue;
      }
      const started = process.hrtime.bigint();
      await tx.unsafe(m.sql).simple();
      const elapsed = process.hrtime.bigint() - started;
      await tx`INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time)
        VALUES (${m.version.toString()}, ${m.description}, TRUE, ${m.checksum}, ${elapsed.toString()})`;
    }
  });
}

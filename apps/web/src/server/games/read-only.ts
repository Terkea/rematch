import type { Db } from "@ci/db";

/**
 * Runs `fn` in a READ ONLY, REPEATABLE READ transaction: a stray write fails with Postgres code
 * 25006, and every statement in `fn` sees one snapshot (a count and its page agree while an
 * import is running). Every exported read in `server/games`, `server/stats` and `server/overview`
 * goes through it (ADR 0046).
 */
export function readOnly<T>(db: Db, fn: (tx: Db) => Promise<T>): Promise<T> {
  return db.transaction((tx) => fn(tx as unknown as Db), {
    accessMode: "read only",
    isolationLevel: "repeatable read",
  });
}

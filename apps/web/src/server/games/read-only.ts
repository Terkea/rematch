import type { Db } from "@ci/db";

/** Runs `fn` in a READ ONLY transaction. Every exported read in the games read model uses it. */
export function readOnly<T>(db: Db, fn: (tx: Db) => Promise<T>): Promise<T> {
  return db.transaction((tx) => fn(tx as unknown as Db), { accessMode: "read only" });
}

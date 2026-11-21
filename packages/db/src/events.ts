import { and, asc, gt, inArray, sql } from "drizzle-orm";
import type { Db } from "./client.js";
import { userEvents } from "./schema.js";

/** Advisory lock key that serialises user-event inserts (arbitrary, fixed). */
export const USER_EVENTS_LOCK_KEY = 7_262_011;

export type UserEventRow = typeof userEvents.$inferSelect;

/**
 * Inserts one user event and returns its id. The id is taken under a transaction-scoped advisory
 * lock, which is released only after the enclosing transaction commits, so ids become visible in
 * id order: a reader that has seen id N will never later find a committed id below N. Pass a
 * transaction to make the event part of it (the lock is then held until that transaction ends).
 */
export async function insertUserEvent(
  db: Pick<Db, "transaction">,
  event: { accountId: string; type: string; payload: object },
): Promise<number> {
  return db.transaction(async (tx) => {
    await tx.execute(sql`select pg_advisory_xact_lock(${USER_EVENTS_LOCK_KEY})`);
    const [row] = await tx.insert(userEvents).values(event).returning({ id: userEvents.id });
    if (!row) throw new Error("insertUserEvent: insert returned no row");
    return row.id;
  });
}

/**
 * Events for the given accounts with `id > afterId`, oldest first: the replay query behind SSE
 * `Last-Event-ID` and the polling fallback. Exactly-once in order relies on every writer using
 * `insertUserEvent`.
 */
export async function listEventsAfter(
  db: Db,
  accountIds: string[],
  afterId: number,
  limit: number,
): Promise<UserEventRow[]> {
  if (accountIds.length === 0) return [];
  return db
    .select()
    .from(userEvents)
    .where(and(inArray(userEvents.accountId, accountIds), gt(userEvents.id, afterId)))
    .orderBy(asc(userEvents.id))
    .limit(limit);
}

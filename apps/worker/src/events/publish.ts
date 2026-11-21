import { type Db, userEvents } from "@ci/db";

export type UserEventType = "import.progress" | "game.analysed" | "job.failed";

export type PublishFn = (
  accountId: string,
  type: UserEventType,
  payload: object,
) => Promise<number>;

export const userEventsChannel = (accountId: string): string => `user-events:${accountId}`;

/**
 * Appends an event to `user_events` and fans it out on `user-events:<accountId>`. Call it only
 * after the state change it describes has committed. The row is the source of truth: a failed
 * PUBLISH is swallowed because SSE clients replay from the table (`id > Last-Event-ID`).
 */
export async function publishEvent(
  deps: { db: Db; redis: { publish(channel: string, message: string): Promise<unknown> } },
  accountId: string,
  type: UserEventType,
  payload: object,
): Promise<number> {
  const [row] = await deps.db
    .insert(userEvents)
    .values({ accountId, type, payload })
    .returning({ id: userEvents.id, createdAt: userEvents.createdAt });
  if (!row) throw new Error("publishEvent: insert returned no row");
  try {
    await deps.redis.publish(
      userEventsChannel(accountId),
      JSON.stringify({ id: row.id, type, payload, createdAt: row.createdAt.toISOString() }),
    );
  } catch {
    // Live subscribers miss it; replay from the table delivers it.
  }
  return row.id;
}

import { type Db, insertUserEvent, userEventsChannel } from "@ci/db";

export { userEventsChannel };

export type UserEventType = "import.progress" | "game.analysed" | "job.failed";

export type PublishFn = (
  accountId: string,
  type: UserEventType,
  payload: object,
) => Promise<number>;

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
  // Committed (in id order, see insertUserEvent) before anyone is told about it.
  const id = await insertUserEvent(deps.db, { accountId, type, payload });
  try {
    await deps.redis.publish(
      userEventsChannel(accountId),
      JSON.stringify({ id, type, payload, createdAt: new Date().toISOString() }),
    );
  } catch {
    // Live subscribers miss it; replay from the table delivers it.
  }
  return id;
}

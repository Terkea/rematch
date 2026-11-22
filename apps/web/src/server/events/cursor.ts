import { type Db, listEventsPage } from "@ci/db";
import { userAccountIds } from "./user-accounts";

/**
 * The newest event id the user can see (0 with none), for `RealtimeProvider`'s `initialEventId`
 * (M4 T23). Read it in the signed-in layout BEFORE any page data: the client stream then replays
 * every event stored after it, so one that lands between the render and the stream opening is
 * not lost. Reading it early only ever re-delivers an event the page already shows.
 */
export async function eventCursor(db: Db, userId: string): Promise<number> {
  const page = await listEventsPage(
    db,
    await userAccountIds(db, userId),
    Number.MAX_SAFE_INTEGER,
    1,
  );
  return page.headId;
}

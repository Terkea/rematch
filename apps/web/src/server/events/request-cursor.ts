import { cache } from "react";
import { getDb } from "../db";
import { eventCursor } from "./cursor";

/**
 * `eventCursor` once per request (M4 T27). The App Router renders the layout and the page in
 * parallel, so the layout alone cannot read the cursor "before the page data": every signed-in
 * page awaits this first, and the layout reads the same memoised value for `RealtimeProvider`.
 * Whoever calls first starts the read, so it always precedes the page's own queries.
 */
export const requestEventCursor = cache(
  (userId: string): Promise<number> => eventCursor(getDb(), userId),
);

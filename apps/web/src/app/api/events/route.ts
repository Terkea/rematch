import { listEventsPage, type StreamEvent } from "@ci/db";
import { rejectCrossSite } from "../../../server/accounts/http";
import { requireApiUser } from "../../../server/auth/session";
import { getDb } from "../../../server/db";
import { getEventHub } from "../../../server/events/hub";
import { openEventStream, parseCursor } from "../../../server/events/sse";
import { userAccountIds } from "../../../server/events/user-accounts";

export const runtime = "nodejs";
export const dynamic = "force-dynamic";

const POLL_LIMIT = 500;

const invalidCursor = () => Response.json({ error: "invalid_cursor" }, { status: 400 });

/**
 * The signed-in user's live events (ADR-WEB-8): an SSE stream with `Last-Event-ID` (or `?since=`)
 * replay, or, with `?after=<id>`, one JSON page for the polling fallback
 * `{ events, lastId, reset }`. Only events of the user's own linked accounts are ever read.
 */
export async function GET(req: Request): Promise<Response> {
  const crossSite = rejectCrossSite(req);
  if (crossSite) return crossSite;
  // A top-level cross-site navigation carries the Lax cookie but no Origin (defence in depth).
  if (req.headers.get("sec-fetch-site") === "cross-site") {
    return Response.json({ error: "invalid_origin" }, { status: 403 });
  }
  const user = await requireApiUser(req);
  if (user instanceof Response) return user;
  const url = new URL(req.url);
  const db = getDb();

  if (url.searchParams.has("after")) {
    const after = parseCursor(url.searchParams.get("after"));
    if (after === "invalid" || after === null) return invalidCursor();
    const accountIds = await userAccountIds(db, user.id);
    const page = await listEventsPage(db, accountIds, after, POLL_LIMIT);
    const events: StreamEvent[] = page.reset
      ? []
      : page.events.map((e) => ({
          id: e.id,
          accountId: e.accountId,
          type: e.type,
          payload: e.payload,
        }));
    const lastId = page.reset ? Math.max(after, page.headId) : (events.at(-1)?.id ?? after);
    return Response.json(
      { events, lastId, reset: page.reset },
      { headers: { "cache-control": "no-store" } },
    );
  }

  const cursor = parseCursor(req.headers.get("last-event-id") || url.searchParams.get("since"));
  if (cursor === "invalid") return invalidCursor();
  return openEventStream({
    db,
    hub: getEventHub(),
    userId: user.id,
    accountIds: await userAccountIds(db, user.id),
    cursor,
    signal: req.signal,
  });
}

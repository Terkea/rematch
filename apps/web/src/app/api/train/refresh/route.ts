import { z } from "zod";
import { TIME_CLASS_ORDER } from "../../../../lib/time-class";
import { rejectCrossSite } from "../../../../server/accounts/http";
import { requireApiUser } from "../../../../server/auth/session";
import { getDb } from "../../../../server/db";
import { scopedAccountIds } from "../../../../server/games";
import type { TimeClass } from "../../../../server/games/types";
import { refreshItems } from "../../../../server/train/store";

export const dynamic = "force-dynamic";

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const Body = z.object({
  accountId: z.string().regex(UUID),
  timeClass: z.enum(TIME_CLASS_ORDER as [TimeClass, ...TimeClass[]]),
});

/**
 * Makes the trainer's new positions for one of the user's scopes (ADR 0090 Amendment 1): the
 * pages never write, so the trainer and the overview ask for this when a read says new analysed
 * games landed. Idempotent, and a no-op for a game set already seen: 200 `{ added }`. 400 for a
 * bad body, 404 for an account that is not the user's.
 */
export async function POST(req: Request): Promise<Response> {
  const crossSite = rejectCrossSite(req);
  if (crossSite) return crossSite;
  const user = await requireApiUser(req);
  if (user instanceof Response) return user;
  let json: unknown;
  try {
    json = await req.json();
  } catch {
    json = undefined;
  }
  const body = Body.safeParse(json);
  if (!body.success) return Response.json({ error: "invalid_body" }, { status: 400 });
  const db = getDb();
  const accountId = body.data.accountId.toLowerCase();
  if ((await scopedAccountIds(db, user.id, accountId)).length === 0) {
    return Response.json({ error: "not_found" }, { status: 404 });
  }
  const added = await refreshItems(db, user.id, { accountId, timeClass: body.data.timeClass });
  return Response.json({ added }, { headers: { "cache-control": "no-store" } });
}

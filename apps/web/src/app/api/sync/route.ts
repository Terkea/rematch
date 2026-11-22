import { requireApiUser } from "../../../server/auth/session";
import { getDb } from "../../../server/db";
import { getSyncStatus } from "../../../server/sync";

export const dynamic = "force-dynamic";

/**
 * The user's sync and analysis progress (`SyncStatus`, Task P2), never cached. "Sync now" is
 * `POST /api/accounts/<accountId>/sync`; "Retry" is `POST /api/sync/retry`.
 */
export async function GET(req: Request): Promise<Response> {
  const user = await requireApiUser(req);
  if (user instanceof Response) return user;
  const status = await getSyncStatus(getDb(), user.id);
  return Response.json(status, { headers: { "cache-control": "no-store" } });
}

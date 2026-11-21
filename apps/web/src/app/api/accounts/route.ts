import { getEnv } from "../../../env";
import { rejectCrossSite } from "../../../server/accounts/http";
import { listAccounts } from "../../../server/accounts/list";
import { requireApiUser } from "../../../server/auth/session";
import { getDb } from "../../../server/db";

export const dynamic = "force-dynamic";

/** The signed-in user's linked accounts (chess.com links only while `CHESSCOM_ENABLED`). */
export async function GET(req: Request): Promise<Response> {
  const crossSite = rejectCrossSite(req);
  if (crossSite) return crossSite;
  const user = await requireApiUser(req);
  if (user instanceof Response) return user;
  const accounts = await listAccounts(getDb(), user.id);
  // While chess.com linking is off, its links are hidden too.
  const visible = getEnv().CHESSCOM_ENABLED
    ? accounts
    : accounts.filter((account) => account.platform !== "chesscom");
  return Response.json(visible);
}

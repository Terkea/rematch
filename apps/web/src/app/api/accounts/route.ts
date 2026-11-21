import { listAccounts } from "../../../server/accounts/list";
import { requireApiUser } from "../../../server/auth/session";
import { getDb } from "../../../server/db";

export const dynamic = "force-dynamic";

/** The signed-in user's linked accounts. */
export async function GET(req: Request): Promise<Response> {
  const user = await requireApiUser(req);
  if (user instanceof Response) return user;
  return Response.json(await listAccounts(getDb(), user.id));
}

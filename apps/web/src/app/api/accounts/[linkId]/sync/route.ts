import { linkIdFrom, notFound, rejectCrossSite } from "../../../../../server/accounts/http";
import { requireApiUser } from "../../../../../server/auth/session";
import { getDb } from "../../../../../server/db";
import { requestSync } from "../../../../../server/sync";

export const dynamic = "force-dynamic";

/**
 * "Sync now" for one linked account (Task P2, ADR 0077): `POST /api/accounts/<accountId>/sync`.
 * The segment is the **linked account id** (`linked_accounts.id`); Next names it `linkId` only
 * because it shares the folder with `DELETE /api/accounts/<linkId>`. The user must hold a link to
 * the account. 202 `{ accountId, outcome }` (`queued`, or `already` when an import is due or
 * running); 429 `{ error: "throttled", availableAt }` inside the 2-minute limit; 404 for a
 * foreign or malformed id.
 */
export async function POST(
  req: Request,
  ctx: { params: Promise<{ linkId: string }> },
): Promise<Response> {
  const crossSite = rejectCrossSite(req);
  if (crossSite) return crossSite;
  const user = await requireApiUser(req);
  if (user instanceof Response) return user;
  const accountId = await linkIdFrom(ctx);
  if (!accountId) return notFound();

  const res = await requestSync(getDb(), user.id, accountId.toLowerCase());
  const [one] = res.kind === "ok" ? res.accounts : [];
  if (!one) return notFound();
  if (one.outcome === "throttled") {
    return Response.json(
      { error: "throttled", availableAt: one.availableAt.toISOString() },
      { status: 429 },
    );
  }
  return Response.json({ accountId: one.accountId, outcome: one.outcome }, { status: 202 });
}

import {
  linkErrorResponse,
  linkIdFrom,
  notFound,
  rejectCrossSite,
} from "../../../../server/accounts/http";
import { removeLink } from "../../../../server/accounts/remove";
import { requireApiUser } from "../../../../server/auth/session";
import { getDb } from "../../../../server/db";

export const dynamic = "force-dynamic";

/** Removes one of the user's own links; the shared account and its games stay. */
export async function DELETE(
  req: Request,
  ctx: { params: Promise<{ linkId: string }> },
): Promise<Response> {
  const crossSite = rejectCrossSite(req);
  if (crossSite) return crossSite;
  const user = await requireApiUser(req);
  if (user instanceof Response) return user;
  const linkId = await linkIdFrom(ctx);
  if (!linkId) return notFound();
  try {
    await removeLink(getDb(), user.id, linkId);
    return new Response(null, { status: 204 });
  } catch (error) {
    return linkErrorResponse(error);
  }
}

import { rejectCrossSite } from "../../../../../server/accounts/http";
import { requireApiUser } from "../../../../../server/auth/session";
import { deleteConversation, getThread } from "../../../../../server/coach";
import { notFound } from "../../../../../server/coach/http";
import { getDb } from "../../../../../server/db";

export const dynamic = "force-dynamic";

type Ctx = { params: Promise<{ threadId: string }> };

/** One of the user's own threads with its messages and any chunks already streamed; else 404. */
export async function GET(req: Request, ctx: Ctx): Promise<Response> {
  const crossSite = rejectCrossSite(req);
  if (crossSite) return crossSite;
  const user = await requireApiUser(req);
  if (user instanceof Response) return user;
  const { threadId } = await ctx.params;
  const thread = await getThread(getDb(), user.id, threadId);
  return thread ? Response.json(thread) : notFound();
}

/** Hard-deletes one of the user's own threads (204), else 404. */
export async function DELETE(req: Request, ctx: Ctx): Promise<Response> {
  const crossSite = rejectCrossSite(req);
  if (crossSite) return crossSite;
  const user = await requireApiUser(req);
  if (user instanceof Response) return user;
  const { threadId } = await ctx.params;
  const deleted = await deleteConversation(getDb(), user.id, threadId);
  return deleted ? new Response(null, { status: 204 }) : notFound();
}

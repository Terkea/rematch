import { rejectCrossSite } from "../../../../../../server/accounts/http";
import { requireApiUser } from "../../../../../../server/auth/session";
import { parseAnchor, postUserMessage } from "../../../../../../server/coach";
import { jsonError, postErrorResponse, readJsonBody } from "../../../../../../server/coach/http";
import { getDb } from "../../../../../../server/db";

export const dynamic = "force-dynamic";

/**
 * Posts `{ question, anchor? }` to one of the user's threads: 202 `{ userMessageId,
 * assistantMessageId }`; the answer streams over `/api/events` (R5). 404 for another user's
 * thread, 403 `not_linked`, 409 `busy`, 400 `empty` / `too_long`, 429 `rate_limited`.
 */
export async function POST(
  req: Request,
  ctx: { params: Promise<{ threadId: string }> },
): Promise<Response> {
  const crossSite = rejectCrossSite(req);
  if (crossSite) return crossSite;
  const user = await requireApiUser(req);
  if (user instanceof Response) return user;
  const body = await readJsonBody(req);
  if (body instanceof Response) return body;
  if (typeof body.question !== "string") return jsonError("invalid_body", 400);
  const { threadId } = await ctx.params;
  const r = await postUserMessage(
    getDb(),
    user.id,
    threadId,
    body.question,
    parseAnchor(body.anchor),
  );
  const failed = postErrorResponse(r);
  if (failed || r.kind !== "posted") return failed ?? jsonError("not_found", 404);
  return Response.json(
    { userMessageId: r.userMessageId, assistantMessageId: r.assistantMessageId },
    { status: 202 },
  );
}

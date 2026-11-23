import { rejectCrossSite } from "../../../../server/accounts/http";
import { requireApiUser } from "../../../../server/auth/session";
import {
  colourQuestion,
  createConversation,
  isLinked,
  isUuid,
  listConversations,
  parseAnchor,
  startConversation,
} from "../../../../server/coach";
import {
  jsonError,
  notFound,
  postErrorResponse,
  rateLimited,
  readJsonBody,
} from "../../../../server/coach/http";
import { getDb } from "../../../../server/db";
import type { TimeClass } from "../../../../server/games/types";

export const dynamic = "force-dynamic";

const TIME_CLASSES: readonly TimeClass[] = ["bullet", "blitz", "rapid", "classical", "daily"];

/**
 * The signed-in user's own conversations on `?account=<id>`, most recently active first: 200
 * (`[]` when there are none) for an account the user is linked to, 404 for any other id, as
 * create answers. Single threads stay readable by id after unlinking.
 */
export async function GET(req: Request): Promise<Response> {
  const crossSite = rejectCrossSite(req);
  if (crossSite) return crossSite;
  const user = await requireApiUser(req);
  if (user instanceof Response) return user;
  const account = new URL(req.url).searchParams.get("account") ?? "";
  const db = getDb();
  if (!isUuid(account) || !(await isLinked(db, user.id, account))) return notFound();
  return Response.json(await listConversations(db, user.id, account));
}

/**
 * Starts a conversation `{ accountId, timeClass, anchor?, color?, question? }` and, with a
 * question, posts it (R5: the answer arrives on `/api/events`). 201 `{ id, assistantMessageId? }`.
 * An account the user is not linked to is not found; a malformed anchor is dropped. `color`
 * ("white" or "black", from a colour's page) puts the colour in the question's words.
 */
export async function POST(req: Request): Promise<Response> {
  const crossSite = rejectCrossSite(req);
  if (crossSite) return crossSite;
  const user = await requireApiUser(req);
  if (user instanceof Response) return user;
  const body = await readJsonBody(req);
  if (body instanceof Response) return body;
  const { accountId, timeClass, question } = body;
  if (typeof accountId !== "string" || !isUuid(accountId)) return notFound();
  if (typeof timeClass !== "string" || !TIME_CLASSES.includes(timeClass as TimeClass)) {
    return jsonError("invalid_time_class", 400);
  }
  if (question !== undefined && question !== null && typeof question !== "string") {
    return jsonError("invalid_body", 400);
  }
  const anchor = parseAnchor(body.anchor);
  const db = getDb();
  const target = { accountId, timeClass: timeClass as TimeClass, anchor };
  if (typeof question !== "string") {
    const made = await createConversation(db, user.id, target);
    if (made.kind === "not_linked") return notFound();
    if (made.kind === "rate_limited") return rateLimited(made.retryAt);
    return Response.json({ id: made.id }, { status: 201 });
  }
  // The create, the first post and its job in one transaction; a repeat answers the first.
  const started = await startConversation(db, user.id, {
    ...target,
    question: colourQuestion(question, body.color),
  });
  if (started.kind !== "started") {
    return started.kind === "not_linked"
      ? notFound()
      : (postErrorResponse(started) ?? jsonError("not_found", 404));
  }
  return Response.json(
    { id: started.id, assistantMessageId: started.assistantMessageId },
    { status: 201 },
  );
}

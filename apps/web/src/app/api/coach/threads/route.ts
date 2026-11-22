import { rejectCrossSite } from "../../../../server/accounts/http";
import { requireApiUser } from "../../../../server/auth/session";
import {
  checkQuestion,
  createConversation,
  isUuid,
  listConversations,
  parseAnchor,
  postUserMessage,
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

/** The signed-in user's own conversations on `?account=<id>`, most recently active first. */
export async function GET(req: Request): Promise<Response> {
  const crossSite = rejectCrossSite(req);
  if (crossSite) return crossSite;
  const user = await requireApiUser(req);
  if (user instanceof Response) return user;
  const account = new URL(req.url).searchParams.get("account") ?? "";
  if (!isUuid(account)) return notFound();
  return Response.json(await listConversations(getDb(), user.id, account));
}

/**
 * Starts a conversation `{ accountId, timeClass, anchor?, question? }` and, with a question,
 * posts it (R5: the answer arrives on `/api/events`). 201 `{ id, assistantMessageId? }`. An
 * account the user is not linked to is not found; a malformed anchor is dropped.
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
  if (typeof question === "string") {
    const invalid = checkQuestion(question);
    if (invalid) return jsonError(invalid, 400);
  }
  const anchor = parseAnchor(body.anchor);
  const db = getDb();
  const made = await createConversation(db, user.id, {
    accountId,
    timeClass: timeClass as TimeClass,
    anchor,
  });
  if (made.kind === "not_linked") return notFound();
  if (made.kind === "rate_limited") return rateLimited(made.retryAt);
  if (typeof question !== "string") return Response.json({ id: made.id }, { status: 201 });
  const posted = await postUserMessage(db, user.id, made.id, question, anchor);
  const failed = postErrorResponse(posted);
  if (failed || posted.kind !== "posted") return failed ?? notFound();
  return Response.json(
    { id: made.id, assistantMessageId: posted.assistantMessageId },
    { status: 201 },
  );
}

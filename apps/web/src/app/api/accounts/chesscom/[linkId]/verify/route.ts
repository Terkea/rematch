import { getEnv } from "../../../../../../env";
import {
  checkChesscomVerification,
  linkDeps,
} from "../../../../../../server/accounts/chesscom-link";
import {
  chesscomDisabled,
  linkErrorResponse,
  linkIdFrom,
  notFound,
} from "../../../../../../server/accounts/http";
import { requireApiUser } from "../../../../../../server/auth/session";
import { getDb } from "../../../../../../server/db";

export const dynamic = "force-dynamic";

/** Checks the profile's location for the code (at most once a minute per link). */
export async function POST(
  req: Request,
  ctx: { params: Promise<{ linkId: string }> },
): Promise<Response> {
  const disabled = chesscomDisabled();
  if (disabled) return disabled;
  const user = await requireApiUser(req);
  if (user instanceof Response) return user;
  const linkId = await linkIdFrom(ctx);
  if (!linkId) return notFound();
  try {
    const result = await checkChesscomVerification(
      linkDeps(getDb(), getEnv()),
      user.id,
      linkId,
      new Date(),
    );
    return Response.json({ status: result.status, nextCheckAt: result.nextCheckAt.toISOString() });
  } catch (error) {
    return linkErrorResponse(error);
  }
}

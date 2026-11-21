import { getEnv } from "../../../../env";
import { linkDeps, startChesscomLink } from "../../../../server/accounts/chesscom-link";
import { chesscomDisabled, linkErrorResponse } from "../../../../server/accounts/http";
import { requireApiUser } from "../../../../server/auth/session";
import { getDb } from "../../../../server/db";

export const dynamic = "force-dynamic";

/** Starts linking a chess.com username: `{ username }` gives 201 with the verification code. */
export async function POST(req: Request): Promise<Response> {
  const disabled = chesscomDisabled();
  if (disabled) return disabled;
  const user = await requireApiUser(req);
  if (user instanceof Response) return user;
  const body = (await req.json().catch(() => null)) as { username?: unknown } | null;
  const username = typeof body?.username === "string" ? body.username : "";
  try {
    const link = await startChesscomLink(linkDeps(getDb(), getEnv()), user.id, username);
    return Response.json(link, { status: 201 });
  } catch (error) {
    return linkErrorResponse(error);
  }
}

import { rejectCrossSite } from "../../../../../server/accounts/http";
import { requireApiUser } from "../../../../../server/auth/session";
import { getDb } from "../../../../../server/db";
import { positionLine } from "../../../../../server/games/position-line";

export const dynamic = "force-dynamic";

type Ctx = { params: Promise<{ gameId: string }> };

/**
 * The position explorer's expanded data for `?ply=N` of one of the user's games
 * (feat/position-explorer): the game around the ply and the engine's stored line. 404 for a game
 * the user may not read or a ply not in it, so no id is confirmed to exist.
 */
export async function GET(req: Request, ctx: Ctx): Promise<Response> {
  const crossSite = rejectCrossSite(req);
  if (crossSite) return crossSite;
  const user = await requireApiUser(req);
  if (user instanceof Response) return user;
  const { gameId } = await ctx.params;
  const ply = Number(new URL(req.url).searchParams.get("ply"));
  const line = await positionLine(getDb(), user.id, gameId, ply);
  return line
    ? Response.json(line, { headers: { "cache-control": "private, no-store" } })
    : Response.json({ error: "not_found" }, { status: 404 });
}

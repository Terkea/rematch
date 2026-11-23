import { z } from "zod";
import { rejectCrossSite } from "../../../../server/accounts/http";
import { requireApiUser } from "../../../../server/auth/session";
import { getDb } from "../../../../server/db";
import { recordAttempt } from "../../../../server/train/store";

export const dynamic = "force-dynamic";

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const Body = z.object({
  itemId: z.string().max(64),
  uci: z.string().regex(/^[a-hA-H][1-8][a-hA-H][1-8][qrbnQRBN]?$/),
  confidence: z.enum(["sure", "guess"]),
  ms: z.number().int().min(0).max(86_400_000).nullish(),
});

/**
 * One attempt at a trainer item (ADR 0090): the confidence is chosen before the move, the move is
 * graded against the item's stored accepted set (no engine call), and the reveal comes back:
 * 200 with the verdict, the engine's line and the game at that ply; 400 for a bad body; 404 for
 * an item that is not the user's, is held out or retired, or an illegal move.
 */
export async function POST(req: Request): Promise<Response> {
  const crossSite = rejectCrossSite(req);
  if (crossSite) return crossSite;
  const user = await requireApiUser(req);
  if (user instanceof Response) return user;
  let json: unknown;
  try {
    json = await req.json();
  } catch {
    json = undefined;
  }
  const body = Body.safeParse(json);
  if (!body.success) return Response.json({ error: "invalid_body" }, { status: 400 });
  const notFound = Response.json({ error: "not_found" }, { status: 404 });
  if (!UUID.test(body.data.itemId)) return notFound;
  const res = await recordAttempt(
    getDb(),
    user.id,
    {
      itemId: body.data.itemId.toLowerCase(),
      uci: body.data.uci,
      confidence: body.data.confidence,
      ms: body.data.ms ?? null,
    },
    new Date(),
  );
  if (!res) return notFound;
  const href = `/app/games/${encodeURIComponent(res.game.id)}?ply=${res.game.ply}`;
  return Response.json(
    { ...res, game: { ...res.game, href } },
    { headers: { "cache-control": "no-store" } },
  );
}

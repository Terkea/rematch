import { z } from "zod";
import { TIME_CLASS_ORDER } from "../../../lib/time-class";
import { rejectCrossSite } from "../../../server/accounts/http";
import { requireApiUser } from "../../../server/auth/session";
import { getDb } from "../../../server/db";
import {
  clearFocus,
  MAX_GAMES_N,
  MIN_GAMES_N,
  setFocus,
  setFocusLength,
} from "../../../server/focus";
import type { TimeClass } from "../../../server/games/types";

export const dynamic = "force-dynamic";

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const Tc = z.enum(TIME_CLASS_ORDER as [TimeClass, ...TimeClass[]]);
const GamesN = z.number().int().min(MIN_GAMES_N).max(MAX_GAMES_N);
const SetBody = z.object({
  accountId: z.string().regex(UUID),
  timeClass: Tc,
  seriesKey: z.string().min(1).max(200),
  side: z.enum(["both", "white", "black"]).optional(),
  gamesN: GamesN.optional(),
});
const LengthBody = z.object({ timeClass: Tc, gamesN: GamesN });
const ClearBody = z.object({ timeClass: Tc });

const notFound = () => Response.json({ error: "not_found" }, { status: 404 });
const invalid = () => Response.json({ error: "invalid_body" }, { status: 400 });

/** The signed-in user and the parsed body, or the response that refuses the request. */
async function read<T>(req: Request, schema: z.ZodType<T>) {
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
  const body = schema.safeParse(json);
  if (!body.success) return invalid();
  return { user, body: body.data };
}

/**
 * "Make this my focus" (ADR 0091): 201 `{ focus }`. The target is chosen here from the
 * weakness's current estimate; a client's own target is ignored. 404 for an account that is not
 * the user's or a weakness its scope has no estimate for, 422 when its rate cannot go down.
 */
export async function POST(req: Request): Promise<Response> {
  const r = await read(req, SetBody);
  if (r instanceof Response) return r;
  const res = await setFocus(getDb(), r.user.id, {
    ...r.body,
    accountId: r.body.accountId.toLowerCase(),
  });
  switch (res.kind) {
    case "set":
      return Response.json({ focus: res.focus }, { status: 201 });
    case "not_found":
      return notFound();
    case "no_target":
      return Response.json({ error: "no_target" }, { status: 422 });
    case "invalid":
      return invalid();
  }
}

/** A new N for the running focus, keeping its start: 200, or 404 with no running focus. */
export async function PATCH(req: Request): Promise<Response> {
  const r = await read(req, LengthBody);
  if (r instanceof Response) return r;
  const ok = await setFocusLength(getDb(), r.user.id, r.body.timeClass, r.body.gamesN);
  return ok ? Response.json({ ok: true }) : notFound();
}

/** Clears the running focus: 200, or 404 with none. */
export async function DELETE(req: Request): Promise<Response> {
  const r = await read(req, ClearBody);
  if (r instanceof Response) return r;
  const ok = await clearFocus(getDb(), r.user.id, r.body.timeClass);
  return ok ? Response.json({ ok: true }) : notFound();
}

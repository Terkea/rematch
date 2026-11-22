import { z } from "zod";
import { rejectCrossSite } from "../../../../server/accounts/http";
import { requireApiUser } from "../../../../server/auth/session";
import { getDb } from "../../../../server/db";
import { requestRetry } from "../../../../server/sync";

export const dynamic = "force-dynamic";

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const Body = z.object({ accountId: z.string() });

/**
 * "Retry" for an account's games whose analysis failed (Task P2, ADR 0077): 202 `{ queued }`,
 * 404 for a foreign or malformed account, 400 without `{ accountId }`.
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
  if (!UUID.test(body.data.accountId)) return notFound;
  const res = await requestRetry(getDb(), user.id, body.data.accountId.toLowerCase());
  if (res.kind === "not_found") return notFound;
  return Response.json({ queued: res.queued }, { status: 202 });
}

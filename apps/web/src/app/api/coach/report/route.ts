import { z } from "zod";
import { rejectCrossSite } from "../../../../server/accounts/http";
import { requireApiUser } from "../../../../server/auth/session";
import { betaReportIntervalDays, requestReport } from "../../../../server/coaching";
import { getDb } from "../../../../server/db";

export const dynamic = "force-dynamic";

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const Body = z.object({
  accountId: z.string(),
  timeClass: z.enum(["bullet", "blitz", "rapid", "classical", "daily"]),
});

const notFound = () => Response.json({ error: "not_found" }, { status: 404 });

/**
 * "Build my report" / "Rebuild report" (ruling R4, ADR 0069). 202 `{ runId }` when queued,
 * 202 `{ runId, building: true }` when a run is already live, 429 `plan_interval` inside the
 * beta interval or `throttled` after failures or too many runs (both with `availableAt`),
 * 403 `not_verified` while the user's link to the account is not verified (holistic F10),
 * 404 for a foreign or malformed account, 400 for a body that is not `{ accountId, timeClass }`.
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
  if (!UUID.test(body.data.accountId)) return notFound();

  const res = await requestReport(getDb(), user.id, body.data, new Date(), {
    intervalDays: betaReportIntervalDays(),
  });
  switch (res.kind) {
    case "queued":
      return Response.json({ runId: res.runId }, { status: 202 });
    case "building":
      return Response.json({ runId: res.runId, building: true }, { status: 202 });
    case "interval":
      return Response.json(
        { error: "plan_interval", availableAt: res.availableAt.toISOString() },
        { status: 429 },
      );
    case "throttled":
      return Response.json(
        { error: "throttled", availableAt: res.availableAt.toISOString() },
        { status: 429 },
      );
    case "not_found":
      return notFound();
    case "not_verified":
      return Response.json({ error: "not_verified" }, { status: 403 });
  }
}

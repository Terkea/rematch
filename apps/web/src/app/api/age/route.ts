import { getEnv } from "../../../env";
import { rejectCrossSite } from "../../../server/accounts/http";
import {
  AGE_BLOCK_COOKIE,
  AGE_BLOCK_MAX_AGE,
  AgeAlreadyRecordedError,
  InvalidBirthMonthError,
  recordAge,
} from "../../../server/age";
import { secureCookies } from "../../../server/auth/policy";
import { getSession } from "../../../server/auth/session";
import { getDb } from "../../../server/db";

export const dynamic = "force-dynamic";

const invalid = () => Response.json({ error: "invalid_birth_month" }, { status: 400 });

/**
 * Records the answer to the age screen: `{ year, month }` gives 200 `{ redirectTo }`.
 * This is the one route a signed-in user may call before the age is known, so it reads the session
 * itself (401 `unauthenticated`, like `requireApiUser`) instead of `requireApiUser`, which answers
 * 403 `age_required` until it is. An age is recorded once (409 `age_already_recorded`).
 * For an under-13 the account is deleted, the 30-day block cookie is set and the session cookie
 * cleared (ADR-WEB-6).
 */
export async function POST(req: Request): Promise<Response> {
  const crossSite = rejectCrossSite(req);
  if (crossSite) return crossSite;
  const current = await getSession(req.headers);
  if (!current) return Response.json({ error: "unauthenticated" }, { status: 401 });
  if (current.user.birthMonth !== null) {
    return Response.json({ error: "age_already_recorded" }, { status: 409 });
  }
  const body = (await req.json().catch(() => null)) as { year?: unknown; month?: unknown } | null;
  if (typeof body?.year !== "number" || typeof body.month !== "number") return invalid();
  let result: Awaited<ReturnType<typeof recordAge>>;
  try {
    result = await recordAge(
      { db: getDb() },
      current.user.id,
      { year: body.year, month: body.month },
      new Date(),
    );
  } catch (error) {
    if (error instanceof InvalidBirthMonthError) return invalid();
    if (error instanceof AgeAlreadyRecordedError) {
      return Response.json({ error: "age_already_recorded" }, { status: 409 });
    }
    throw error;
  }
  const res = Response.json({ redirectTo: result.redirectTo });
  if (result.band === "under13") {
    const secure = secureCookies(getEnv());
    const attrs = `Path=/; HttpOnly; SameSite=Lax${secure ? "; Secure" : ""}`;
    res.headers.append(
      "set-cookie",
      `${AGE_BLOCK_COOKIE}=1; Max-Age=${AGE_BLOCK_MAX_AGE}; ${attrs}`,
    );
    const prefix = secure ? "__Secure-ci" : "ci";
    for (const name of ["session_token", "session_data", "dont_remember"]) {
      res.headers.append("set-cookie", `${prefix}.${name}=; Max-Age=0; ${attrs}`);
    }
  }
  return res;
}

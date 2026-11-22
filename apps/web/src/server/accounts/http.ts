import { getEnv } from "../../env";
import { ChesscomUnavailableError } from "../chesscom/profile";
import { LinkError } from "./chesscom-link";

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

/** Origins allowed to call the mutating routes: `BETTER_AUTH_URL` plus the trusted origins. */
function allowedOrigins(): Set<string> {
  const env = getEnv();
  const origins = env.BETTER_AUTH_TRUSTED_ORIGINS.split(",")
    .map((origin) => origin.trim())
    .filter(Boolean)
    .map((origin) => new URL(origin).origin);
  return new Set([new URL(env.BETTER_AUTH_URL).origin, ...origins]);
}

/**
 * CSRF defence for the account routes that change state (on top of the `SameSite=Lax` session
 * cookie): the `Origin` must be the app's own (403 `invalid_origin`, also when it is missing), and
 * the body must be declared JSON (415 `unsupported_media_type`), which a cross-site HTML form
 * cannot send without a CORS preflight. Returns null when the request may proceed.
 * For reads (`GET`), browsers may omit `Origin` on same-origin requests, so only a foreign
 * `Origin` is refused and no body type is required.
 */
export function rejectCrossSite(req: Request): Response | null {
  const origin = req.headers.get("origin");
  const read = req.method === "GET" || req.method === "HEAD";
  if (
    read
      ? origin !== null && !allowedOrigins().has(origin)
      : !origin || !allowedOrigins().has(origin)
  ) {
    return Response.json({ error: "invalid_origin" }, { status: 403 });
  }
  if (read) return null;
  const type = req.headers.get("content-type")?.split(";")[0]?.trim().toLowerCase();
  if (type !== "application/json") {
    return Response.json({ error: "unsupported_media_type" }, { status: 415 });
  }
  return null;
}

export const notFound = () => Response.json({ error: "not_found" }, { status: 404 });

/** A 404 while chess.com linking is switched off (`CHESSCOM_ENABLED`), else null. */
export function chesscomDisabled(): Response | null {
  return getEnv().CHESSCOM_ENABLED ? null : notFound();
}

/** The link id from the route, or null when it cannot be one (answered as 404). */
export async function linkIdFrom(ctx: { params: Promise<{ linkId: string }> }) {
  const { linkId } = await ctx.params;
  return UUID.test(linkId) ? linkId : null;
}

const STATUS: Record<LinkError["code"], number> = {
  invalid_username: 400,
  not_found: 404,
  limit_reached: 409,
  link_not_found: 404,
  rate_limited: 429,
  last_sign_in_method: 409,
};

/** Maps the linking errors to their JSON responses; anything else is rethrown. */
export function linkErrorResponse(error: unknown): Response {
  if (error instanceof LinkError) {
    const code = error.code === "link_not_found" ? "not_found" : error.code;
    const status = STATUS[error.code];
    if (error.code === "rate_limited" && error.retryAt) {
      const seconds = Math.max(1, Math.ceil((error.retryAt.getTime() - Date.now()) / 1000));
      return Response.json(
        { error: code, retryAt: error.retryAt.toISOString() },
        { status, headers: { "retry-after": String(seconds) } },
      );
    }
    return Response.json({ error: code }, { status });
  }
  if (error instanceof ChesscomUnavailableError) {
    return Response.json({ error: "chesscom_unavailable" }, { status: 503 });
  }
  throw error;
}

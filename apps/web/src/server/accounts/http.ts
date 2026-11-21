import { getEnv } from "../../env";
import { ChesscomUnavailableError } from "../chesscom/profile";
import { LinkError } from "./chesscom-link";

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

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
};

/** Maps the linking errors to their JSON responses; anything else is rethrown. */
export function linkErrorResponse(error: unknown): Response {
  if (error instanceof LinkError) {
    const code = error.code === "link_not_found" ? "not_found" : error.code;
    return Response.json({ error: code }, { status: STATUS[error.code] });
  }
  if (error instanceof ChesscomUnavailableError) {
    return Response.json({ error: "chesscom_unavailable" }, { status: 503 });
  }
  throw error;
}

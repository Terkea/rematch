import { decryptOAuthToken } from "better-auth/oauth2";
import { getEnv } from "../../../../env";
import {
  linkErrorResponse,
  linkIdFrom,
  notFound,
  rejectCrossSite,
} from "../../../../server/accounts/http";
import { removeLink } from "../../../../server/accounts/remove";
import { getAuth } from "../../../../server/auth";
import { runAfterResponse } from "../../../../server/auth/background";
import { revokeLichessToken } from "../../../../server/auth/lichess";
import { requireApiUser } from "../../../../server/auth/session";
import { getDb } from "../../../../server/db";

type DecryptContext = Parameters<typeof decryptOAuthToken>[1];

export const dynamic = "force-dynamic";

/** Removes one of the user's own links; the shared account and its games stay. */
export async function DELETE(
  req: Request,
  ctx: { params: Promise<{ linkId: string }> },
): Promise<Response> {
  const crossSite = rejectCrossSite(req);
  if (crossSite) return crossSite;
  const user = await requireApiUser(req);
  if (user instanceof Response) return user;
  const linkId = await linkIdFrom(ctx);
  if (!linkId) return notFound();
  try {
    await removeLink(getDb(), user.id, linkId, {
      // Revoked at Lichess after the response, best effort (ADR-WEB-4, review I2).
      revokeLichessToken: (stored) => {
        runAfterResponse("lichess token revoke", async () => {
          // The instance's context is an AuthContext; its inferred options type is narrower.
          const context = (await getAuth().$context) as unknown as DecryptContext;
          const token = await decryptOAuthToken(stored, context);
          if (token) await revokeLichessToken(getEnv(), token);
        });
      },
    });
    return new Response(null, { status: 204 });
  } catch (error) {
    return linkErrorResponse(error);
  }
}

import type { BetterAuthOptions } from "better-auth";
import { APIError, createAuthMiddleware } from "better-auth/api";
import { hasAgeBlock, isAgeGatedPath } from "../age";

export const AGE_BLOCKED = "AGE_BLOCKED";

/**
 * Better Auth request hooks. `before` refuses every sign-in, sign-up, magic-link and OAuth path
 * while the browser carries the age block cookie (ADR-WEB-6), so a blocked under-13 cannot retry
 * with the back button or another provider. It runs before the endpoint, so no mail is sent.
 */
export const authHooks: NonNullable<BetterAuthOptions["hooks"]> = {
  before: createAuthMiddleware(async (ctx) => {
    if (!isAgeGatedPath(ctx.path)) return;
    if (!hasAgeBlock(ctx.headers ?? ctx.request?.headers)) return;
    throw new APIError("FORBIDDEN", {
      code: AGE_BLOCKED,
      message: "You need to be 13 or older to use chess-improver.",
    });
  }),
};

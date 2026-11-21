import { magicLinkClient } from "better-auth/client/plugins";
import { createAuthClient } from "better-auth/react";

/**
 * Browser client for `/api/auth` (same origin). Generic OAuth providers (Lichess, chess.com) are
 * social providers in Better Auth 1.7, so they use `signIn.social` and need no client plugin.
 */
export const authClient = createAuthClient({ plugins: [magicLinkClient()] });

export const { signIn, signOut, useSession } = authClient;

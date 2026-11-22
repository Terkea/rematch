import type { GenericOAuthConfig } from "better-auth/plugins/generic-oauth";
import type { WebEnv } from "../../env";

export const CHESSCOM_DISCOVERY_URL = "https://oauth.chess.com/.well-known/openid-configuration";

type ChesscomOAuthEnv = Pick<
  WebEnv,
  "CHESSCOM_OAUTH_ENABLED" | "CHESSCOM_CLIENT_ID" | "CHESSCOM_CLIENT_SECRET"
>;

/**
 * chess.com OAuth (OIDC discovery). Registered only when `CHESSCOM_OAUTH_ENABLED` is on and both
 * client credentials are set; its endpoints are unverified until chess.com approves the
 * application (spec §18.1).
 *
 * chess.com's email is never treated as verified (ADR-WEB-3, ADR-WEB-5). chess.com is not a
 * trusted linking provider either, so Better Auth never links it to an account by email, and the
 * `NON_EMAIL_PROVIDERS` exemption in the admission policy stays safe. Proving ownership of a
 * chess.com account goes through `account_links` (location code today, `chesscom_oauth` later).
 */
export function chesscomOAuthProvider(env: ChesscomOAuthEnv): GenericOAuthConfig | null {
  if (!env.CHESSCOM_OAUTH_ENABLED || !env.CHESSCOM_CLIENT_ID || !env.CHESSCOM_CLIENT_SECRET) {
    return null;
  }
  return {
    providerId: "chesscom",
    name: "chess.com",
    discoveryUrl: CHESSCOM_DISCOVERY_URL,
    clientId: env.CHESSCOM_CLIENT_ID,
    clientSecret: env.CHESSCOM_CLIENT_SECRET,
    scopes: ["openid", "profile"],
    pkce: true,
    accountSubject: ({ profile }) => {
      const subject = (profile as { sub?: unknown; id?: unknown }).sub ?? profile.id;
      return subject == null ? "" : String(subject);
    },
    mapProfileToUser: () => ({ emailVerified: false }),
  };
}

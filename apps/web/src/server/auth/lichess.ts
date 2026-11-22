import type { GenericOAuthConfig } from "better-auth/plugins/generic-oauth";
import type { WebEnv } from "../../env";

/** ADR 0043: the project User-Agent, with no email. Must equal the worker's `USER_AGENT`. */
export const LICHESS_USER_AGENT = "chess-improver/0.1 (+https://github.com/chess-improver)";

/** A token this close to its expiry shows "Reconnect Lichess" (Lichess has no refresh token). */
export const LICHESS_EXPIRING_WINDOW_MS = 14 * 24 * 3600 * 1000;
const REQUEST_TIMEOUT_MS = 5_000;
const MAX_BODY_BYTES = 64 * 1024;

type LichessEnv = Pick<
  WebEnv,
  "LICHESS_CLIENT_ID" | "LICHESS_AUTHORIZE_BASE_URL" | "LICHESS_API_BASE_URL"
>;

/**
 * Display names seen in `getUserInfo`, by Lichess id, for the account hook that runs right after
 * in the same request (the hook only sees the account row). Public data only; bounded.
 */
const seenUsernames = new Map<string, string>();
const MAX_SEEN = 500;

export function lichessUsername(lichessId: string): string | undefined {
  return seenUsernames.get(lichessId);
}

function rememberUsername(lichessId: string, username: string): void {
  seenUsernames.delete(lichessId);
  seenUsernames.set(lichessId, username);
  while (seenUsernames.size > MAX_SEEN) {
    const oldest = seenUsernames.keys().next().value;
    if (oldest === undefined) break;
    seenUsernames.delete(oldest);
  }
}

const USERNAME = /^[A-Za-z0-9][A-Za-z0-9_-]{0,29}$/;

/**
 * GET a Lichess API endpoint with the access token. Failures return null (Better Auth then
 * answers `unable_to_get_user_info`); nothing from the token or the response is logged.
 */
async function lichessGet(baseUrl: string, path: string, accessToken: string): Promise<unknown> {
  try {
    const res = await fetch(`${baseUrl}${path}`, {
      headers: {
        Authorization: `Bearer ${accessToken}`,
        Accept: "application/json",
        "User-Agent": LICHESS_USER_AGENT,
      },
      redirect: "error",
      signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS),
    });
    if (!res.ok) return null;
    const text = await res.text();
    if (text.length > MAX_BODY_BYTES) return null;
    return JSON.parse(text) as unknown;
  } catch {
    return null;
  }
}

/**
 * Lichess OAuth (ADR-WEB-4): a public PKCE client (S256, no client secret; Lichess needs no app
 * registration), scope `email:read` because Better Auth needs an email. The email is never
 * treated as verified: it cannot prove an invite or link to an account by itself. Ownership of
 * the Lichess account is proven by the OAuth exchange, and recorded in `account_links` by the
 * account hook (`onLichessAccount`).
 *
 * The authorize and API bases are separate so e2e can send the browser and the server to the
 * same fake by different hostnames.
 */
export function lichessProvider(env: LichessEnv): GenericOAuthConfig {
  const api = env.LICHESS_API_BASE_URL.replace(/\/+$/, "");
  const authorize = env.LICHESS_AUTHORIZE_BASE_URL.replace(/\/+$/, "");
  return {
    providerId: "lichess",
    name: "Lichess",
    clientId: env.LICHESS_CLIENT_ID,
    authorizationUrl: `${authorize}/oauth`,
    tokenUrl: `${api}/api/token`,
    scopes: ["email:read"],
    pkce: true,
    authorizationHeaders: { "User-Agent": LICHESS_USER_AGENT },
    async getUserInfo(tokens) {
      const accessToken = tokens.accessToken;
      if (!accessToken) return null;
      const [account, email] = (await Promise.all([
        lichessGet(api, "/api/account", accessToken),
        lichessGet(api, "/api/account/email", accessToken),
      ])) as [{ id?: unknown; username?: unknown } | null, { email?: unknown } | null];
      const id = typeof account?.id === "string" ? account.id : null;
      const username = typeof account?.username === "string" ? account.username : null;
      if (!id || !username || !USERNAME.test(username) || id !== username.toLowerCase()) {
        return null;
      }
      const address = typeof email?.email === "string" ? email.email : null;
      if (!address) return null;
      rememberUsername(id, username);
      return { id, name: username, email: address, emailVerified: false, image: undefined };
    },
    // Whatever the profile says, a Lichess email is never verified (T19 review).
    mapProfileToUser: () => ({ emailVerified: false }),
  };
}

/**
 * The state of a Lichess link's token: `expiring` within 14 days of expiry, `expired` after it.
 * Lichess has no refresh token, so both offer "Reconnect Lichess".
 */
export function lichessLinkStatus(
  account: { accessTokenExpiresAt: Date | null },
  now: Date,
): "ok" | "expiring" | "expired" {
  const expiresAt = account.accessTokenExpiresAt?.getTime();
  if (expiresAt === undefined) return "ok";
  if (expiresAt <= now.getTime()) return "expired";
  if (expiresAt - now.getTime() <= LICHESS_EXPIRING_WINDOW_MS) return "expiring";
  return "ok";
}

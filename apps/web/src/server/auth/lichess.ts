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
 * Users created by a Lichess sign-up get this non-routable address (RFC 2606 `.invalid`) instead of
 * any email from Lichess, which is never verified (ADR-WEB-4, review I5): a Lichess holder cannot
 * pre-claim someone else's address, and no magic link is ever sent here.
 */
export const LICHESS_PLACEHOLDER_DOMAIN = "lichess.chess-improver.invalid";

export function lichessPlaceholderEmail(lichessId: string): string {
  return `${lichessId.toLowerCase()}@${LICHESS_PLACEHOLDER_DOMAIN}`;
}

export function isLichessPlaceholderEmail(email: string): boolean {
  return email.toLowerCase().endsWith(`@${LICHESS_PLACEHOLDER_DOMAIN}`);
}

/**
 * The Lichess base URLs must be https, except in development and tests (the fakes are plain
 * http): bearer tokens travel to the API base.
 */
export function assertLichessBaseUrl(baseUrl: string, nodeEnv = process.env.NODE_ENV): void {
  const protocol = new URL(baseUrl).protocol;
  if (protocol === "https:") return;
  if (protocol === "http:" && (nodeEnv === "test" || nodeEnv === "development")) return;
  throw new Error("LICHESS_AUTHORIZE_BASE_URL and LICHESS_API_BASE_URL must be https");
}

/** Reads a JSON body, giving up beyond `MAX_BODY_BYTES` (declared or counted while reading). */
async function readCappedJson(res: Response): Promise<unknown> {
  const declared = Number(res.headers.get("content-length"));
  if (Number.isFinite(declared) && declared > MAX_BODY_BYTES) {
    await res.body?.cancel().catch(() => {});
    return null;
  }
  if (!res.body) return null;
  const reader = res.body.getReader();
  const chunks: Uint8Array[] = [];
  let size = 0;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    size += value.byteLength;
    if (size > MAX_BODY_BYTES) {
      await reader.cancel().catch(() => {});
      return null;
    }
    chunks.push(value);
  }
  return JSON.parse(Buffer.concat(chunks).toString("utf8")) as unknown;
}

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
    if (!res.ok) {
      await res.body?.cancel().catch(() => {});
      return null;
    }
    return await readCappedJson(res);
  } catch {
    return null;
  }
}

/**
 * Revokes a Lichess access token (`DELETE /api/token`), best effort: failures are swallowed and
 * nothing about the token is logged.
 */
export async function revokeLichessToken(env: LichessEnv, accessToken: string): Promise<void> {
  const api = env.LICHESS_API_BASE_URL.replace(/\/+$/, "");
  assertLichessBaseUrl(api);
  try {
    const res = await fetch(`${api}/api/token`, {
      method: "DELETE",
      headers: { Authorization: `Bearer ${accessToken}`, "User-Agent": LICHESS_USER_AGENT },
      redirect: "error",
      signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS),
    });
    await res.body?.cancel().catch(() => {});
  } catch {
    // Best effort: the token is already deleted here, and it expires on its own.
  }
}

/**
 * Lichess OAuth (ADR-WEB-4): a public PKCE client (S256, no client secret; Lichess needs no app
 * registration) that asks for no scope: only `/api/account` is read. Better Auth needs an email,
 * so the user gets `lichessPlaceholderEmail(id)`, never the Lichess address (review I5). Ownership
 * of the Lichess account is proven by the OAuth exchange, and recorded in `account_links` by the
 * account hook (`onLichessAccount`).
 *
 * The authorize and API bases are separate so e2e can send the browser and the server to the
 * same fake by different hostnames.
 */
export function lichessProvider(env: LichessEnv): GenericOAuthConfig {
  const api = env.LICHESS_API_BASE_URL.replace(/\/+$/, "");
  const authorize = env.LICHESS_AUTHORIZE_BASE_URL.replace(/\/+$/, "");
  assertLichessBaseUrl(api);
  assertLichessBaseUrl(authorize);
  return {
    providerId: "lichess",
    name: "Lichess",
    clientId: env.LICHESS_CLIENT_ID,
    authorizationUrl: `${authorize}/oauth`,
    tokenUrl: `${api}/api/token`,
    scopes: [],
    pkce: true,
    authorizationHeaders: { "User-Agent": LICHESS_USER_AGENT },
    async getUserInfo(tokens) {
      const accessToken = tokens.accessToken;
      if (!accessToken) return null;
      const account = (await lichessGet(api, "/api/account", accessToken)) as {
        id?: unknown;
        username?: unknown;
      } | null;
      const id = typeof account?.id === "string" ? account.id : null;
      const username = typeof account?.username === "string" ? account.username : null;
      if (!id || !username || !USERNAME.test(username) || id !== username.toLowerCase()) {
        return null;
      }
      rememberUsername(id, username);
      return {
        id,
        name: username,
        email: lichessPlaceholderEmail(id),
        emailVerified: false,
        image: undefined,
      };
    },
    // Whatever the profile says: no Lichess email, and never verified (T19 review, review I5).
    mapProfileToUser: (profile) => ({
      email: lichessPlaceholderEmail(String(profile.id ?? "")),
      emailVerified: false,
    }),
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

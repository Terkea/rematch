import { account, user } from "@ci/db";
import { eq } from "drizzle-orm";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createTestSession, inviteEmail, resetAuthTables, webTestDb } from "../helpers/session";

/**
 * The verified-email rule and its live-session exemption (ADR-WEB-3), driven through Better Auth's
 * real OAuth redirect and callback endpoints. Only the provider side is faked: the token,
 * userinfo and discovery requests that Better Auth makes are answered by a stubbed global fetch,
 * so nothing leaves the process.
 */

const db = webTestDb();
const { GET, POST } = await import("../../src/app/api/auth/[...all]/route");
const { resetAuth } = await import("../../src/server/auth");
const { resetEnvCache } = await import("../../src/env");

const BASE = "http://localhost:3100";
/** The harness plays a trusted proxy, so each test is its own client for Better Auth's rate limit. */
const PROXY_CIDR = "172.16.0.0/12";
let clientCounter = 0;
let client = "198.51.100.1";
const realFetch = globalThis.fetch;

function setEnv(values: Record<string, string>): void {
  for (const [key, value] of Object.entries(values)) vi.stubEnv(key, value);
  resetEnvCache();
  resetAuth();
}

const b64 = (value: unknown) => Buffer.from(JSON.stringify(value)).toString("base64url");
/** An id_token-shaped JWT. Better Auth reads Google's id_token from its own token request, unverified. */
const idToken = (claims: Record<string, unknown>) =>
  `${b64({ alg: "RS256", kid: "test" })}.${b64(claims)}.c2ln`;

/** What the fake providers say about the person signing in. */
let identity: { sub: string; email: string; emailVerified: boolean };

function stubProviders(): void {
  vi.stubGlobal("fetch", async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input instanceof Request ? input.url : input);
    const now = Math.floor(Date.now() / 1000);
    if (url.startsWith("https://oauth2.googleapis.com/token")) {
      return Response.json({
        access_token: "google-access",
        token_type: "Bearer",
        expires_in: 3600,
        id_token: idToken({
          iss: "https://accounts.google.com",
          aud: "google-client",
          sub: identity.sub,
          email: identity.email,
          email_verified: identity.emailVerified,
          name: "Ana",
          iat: now,
          exp: now + 3600,
        }),
      });
    }
    if (url === "https://oauth.chess.com/.well-known/openid-configuration") {
      return Response.json({
        issuer: "https://oauth.chess.com",
        authorization_endpoint: "https://oauth.chess.com/authorize",
        token_endpoint: "https://oauth.chess.com/token",
        userinfo_endpoint: "https://oauth.chess.com/userinfo",
      });
    }
    if (url.startsWith("https://oauth.chess.com/token")) {
      return Response.json({ access_token: "chesscom-access", token_type: "Bearer" });
    }
    if (url.startsWith("https://oauth.chess.com/userinfo")) {
      return Response.json({
        sub: identity.sub,
        email: identity.email,
        email_verified: identity.emailVerified,
        name: "Ana",
      });
    }
    if (/^https?:\/\/(localhost|127\.0\.0\.1)/.test(url)) return realFetch(input, init);
    throw new Error(`unexpected outbound request in test: ${url}`);
  });
}

/** Cookie jar: `name=value` pairs from Set-Cookie, merged over the ones given. */
function cookiesFrom(res: Response, jar: Map<string, string>): Map<string, string> {
  const next = new Map(jar);
  for (const line of res.headers.getSetCookie()) {
    const [pair] = line.split(";");
    const at = pair?.indexOf("=") ?? -1;
    if (!pair || at < 0) continue;
    next.set(pair.slice(0, at), pair.slice(at + 1));
  }
  return next;
}
const header = (jar: Map<string, string>) =>
  [...jar].map(([name, value]) => `${name}=${value}`).join("; ");
function jarOf(cookie: string): Map<string, string> {
  const at = cookie.indexOf("=");
  return new Map([[cookie.slice(0, at), cookie.slice(at + 1)]]);
}

/**
 * Starts a sign-in (`/sign-in/social`) or, with a session, a link (`/link-social`), then plays the
 * provider: returns to the callback with a code and the state Better Auth issued.
 */
async function roundTrip(opts: {
  provider: "google" | "chesscom";
  sessionCookie?: string;
  dropSessionOnCallback?: boolean;
}): Promise<{ error: string | null; location: string }> {
  let jar = opts.sessionCookie ? jarOf(opts.sessionCookie) : new Map<string, string>();
  const path = opts.sessionCookie ? "/api/auth/link-social" : "/api/auth/sign-in/social";
  const start = await POST(
    new Request(`${BASE}${path}`, {
      method: "POST",
      headers: {
        "content-type": "application/json",
        origin: BASE,
        cookie: header(jar),
        "x-forwarded-for": client,
      },
      body: JSON.stringify({ provider: opts.provider, callbackURL: "/settings/accounts" }),
    }),
  );
  expect(start.status).toBe(200);
  const { url } = (await start.json()) as { url: string };
  const state = new URL(url).searchParams.get("state");
  expect(state).toBeTruthy();
  jar = cookiesFrom(start, jar);
  if (opts.dropSessionOnCallback) jar.delete("ci.session_token");
  const callback = await GET(
    new Request(`${BASE}/api/auth/callback/${opts.provider}?code=abc&state=${state}`, {
      headers: { cookie: header(jar), "x-forwarded-for": client },
    }),
  );
  expect(callback.status).toBe(302);
  const location = callback.headers.get("location") ?? "";
  return { error: new URL(location, BASE).searchParams.get("error"), location };
}

async function accountsOf(userId: string, providerId: string) {
  return (await db.select().from(account).where(eq(account.userId, userId))).filter(
    (row) => row.providerId === providerId,
  );
}

beforeEach(async () => {
  await resetAuthTables(db);
  clientCounter += 1;
  client = `198.51.${Math.floor(clientCounter / 250)}.${(clientCounter % 250) + 1}`;
  setEnv({
    TRUSTED_PROXIES: PROXY_CIDR,
    INVITE_ONLY: "1",
    INVITE_ALLOWLIST: "",
    GOOGLE_CLIENT_ID: "google-client",
    GOOGLE_CLIENT_SECRET: "google-secret",
    CHESSCOM_OAUTH_ENABLED: "1",
    CHESSCOM_CLIENT_ID: "chesscom-client",
    CHESSCOM_CLIENT_SECRET: "chesscom-secret",
  });
  stubProviders();
});

afterEach(() => {
  vi.unstubAllGlobals();
});

async function ana() {
  await inviteEmail(db, "ana@example.com");
  return createTestSession(db, { email: "ana@example.com" });
}

describe("Google through the real callback", () => {
  beforeEach(() => {
    identity = { sub: "google-ana", email: "ana@example.com", emailVerified: false };
  });

  it("a live session links a Google identity whose email is unverified", async () => {
    const { user: me, cookie } = await ana();
    const result = await roundTrip({ provider: "google", sessionCookie: cookie });
    expect(result.error).toBeNull();
    expect(new URL(result.location, BASE).pathname).toBe("/settings/accounts");
    expect(await accountsOf(me.id, "google")).toHaveLength(1);
  });

  it("the exemption needs the session on the callback itself, not only at the start", async () => {
    const { user: me, cookie } = await ana();
    const result = await roundTrip({
      provider: "google",
      sessionCookie: cookie,
      dropSessionOnCallback: true,
    });
    expect(result.error).toBe("email_not_verified");
    expect(await accountsOf(me.id, "google")).toHaveLength(0);
  });

  it("an unverified Google email is not linked implicitly to the account with that email", async () => {
    const { user: me } = await ana();
    const result = await roundTrip({ provider: "google" });
    expect(result.error).toBe("email_not_verified");
    expect(result.location).toBe(`${BASE}/sign-in?error=email_not_verified`);
    expect(await accountsOf(me.id, "google")).toHaveLength(0);
  });

  it("a verified Google email still links implicitly (Google is a trusted provider)", async () => {
    identity.emailVerified = true;
    const { user: me } = await ana();
    const result = await roundTrip({ provider: "google" });
    expect(result.error).toBeNull();
    expect(await accountsOf(me.id, "google")).toHaveLength(1);
  });
});

describe("chess.com OAuth through the real callback (emailVerified is always false)", () => {
  beforeEach(() => {
    // Even a provider that claims a verified email is not believed.
    identity = { sub: "chesscom-ana", email: "ana@example.com", emailVerified: true };
  });

  it("is never linked implicitly to the account with the same email", async () => {
    const { user: me } = await ana();
    const result = await roundTrip({ provider: "chesscom" });
    expect(result.error).toBe("account_not_linked");
    expect(await accountsOf(me.id, "chesscom")).toHaveLength(0);
    expect(await db.select().from(user)).toHaveLength(1);
  });

  it("cannot create an account under INVITE_ONLY", async () => {
    identity.email = "stranger@example.com";
    const result = await roundTrip({ provider: "chesscom" });
    expect(result.error).toBe("not_invited");
    expect(await db.select().from(user)).toHaveLength(0);
  });

  it("is not linked to Better Auth accounts even from a live session (chess.com links go through account_links)", async () => {
    const { user: me, cookie } = await ana();
    const result = await roundTrip({ provider: "chesscom", sessionCookie: cookie });
    expect(result.error).toBe("unable_to_link_account");
    expect(await accountsOf(me.id, "chesscom")).toHaveLength(0);
  });
});

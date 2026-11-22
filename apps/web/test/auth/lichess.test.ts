import { account, accountLinks, linkedAccounts, user } from "@ci/db";
import { and, eq } from "drizzle-orm";
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { DENY_HINT, FAKE_LICHESS_EMAIL, resetLichessFake } from "../fakes/lichess";
import { type Fakes, startFakes } from "../fakes/main";
import { createTestSession, inviteEmail, resetAuthTables, webTestDb } from "../helpers/session";

/**
 * Lichess OAuth (PKCE, no client secret) and Lichess linking (ADR-WEB-4), driven in-process
 * through Better Auth's real `/sign-in/social`, `/link-social` and `/callback/lichess` endpoints
 * against the fake Lichess server on a random local port. Any other outbound request throws, so
 * nothing reaches lichess.org.
 */

const db = webTestDb();
const { GET, POST } = await import("../../src/app/api/auth/[...all]/route");
const { resetAuth } = await import("../../src/server/auth");
const { resetEnvCache, getEnv } = await import("../../src/env");
const { lichessProvider, lichessLinkStatus, LICHESS_USER_AGENT } = await import(
  "../../src/server/auth/lichess"
);
const { onLichessAccount } = await import("../../src/server/accounts/lichess-link");
const { listAccounts } = await import("../../src/server/accounts/list");

const BASE = "http://localhost:3100";
const PROXY_CIDR = "172.16.0.0/12";
const DAY = 24 * 3600 * 1000;
let clientCounter = 0;
let client = "203.0.113.1";
let fakes: Fakes;
const realFetch = globalThis.fetch;
/** User-Agent of every request that reached the fake. */
let agents: string[] = [];

function setEnv(values: Record<string, string>): void {
  for (const [key, value] of Object.entries(values)) vi.stubEnv(key, value);
  resetEnvCache();
  resetAuth();
}

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

/** Starts sign-in (or, with a session, a link) and returns the authorisation URL and cookies. */
async function start(opts: { sessionCookie?: string; loginHint?: string } = {}) {
  const jar = opts.sessionCookie ? jarOf(opts.sessionCookie) : new Map<string, string>();
  const linking = Boolean(opts.sessionCookie);
  const res = await POST(
    new Request(`${BASE}/api/auth/${linking ? "link-social" : "sign-in/social"}`, {
      method: "POST",
      headers: {
        "content-type": "application/json",
        origin: BASE,
        cookie: header(jar),
        "x-forwarded-for": client,
      },
      body: JSON.stringify({
        provider: "lichess",
        ...(linking
          ? {
              callbackURL: "/app/accounts?connected=lichess",
              errorCallbackURL: "/app/accounts?error=lichess_denied",
            }
          : { callbackURL: "/", newUserCallbackURL: "/welcome/age", errorCallbackURL: "/sign-in" }),
        ...(opts.loginHint ? { loginHint: opts.loginHint } : {}),
      }),
    }),
  );
  expect(res.status).toBe(200);
  const { url } = (await res.json()) as { url: string };
  return { url: new URL(url), jar: cookiesFrom(res, jar) };
}

/** Plays the browser: authorisation URL → fake Lichess → back to our callback. */
async function roundTrip(opts: { sessionCookie?: string; loginHint?: string } = {}) {
  const { url, jar } = await start(opts);
  const consent = await realFetch(url, { redirect: "manual" });
  expect(consent.status).toBe(302);
  const back = new URL(consent.headers.get("location") ?? "");
  expect(back.origin + back.pathname).toBe(`${BASE}/api/auth/callback/lichess`);
  const callback = await GET(
    new Request(back.toString(), { headers: { cookie: header(jar), "x-forwarded-for": client } }),
  );
  expect(callback.status).toBe(302);
  const location = new URL(callback.headers.get("location") ?? "", BASE);
  return { location, jar: cookiesFrom(callback, jar) };
}

async function lichessAccounts() {
  return db.select().from(account).where(eq(account.providerId, "lichess"));
}

async function linksOf(userId: string) {
  return db
    .select({ link: accountLinks, account: linkedAccounts })
    .from(accountLinks)
    .innerJoin(linkedAccounts, eq(linkedAccounts.id, accountLinks.linkedAccountId))
    .where(eq(accountLinks.userId, userId));
}

beforeAll(async () => {
  fakes = await startFakes(0);
});

afterAll(async () => {
  await fakes.close();
});

beforeEach(async () => {
  await resetAuthTables(db);
  await db.delete(linkedAccounts).where(eq(linkedAccounts.platform, "lichess"));
  resetLichessFake();
  agents = [];
  clientCounter += 1;
  client = `203.0.${Math.floor(clientCounter / 250)}.${(clientCounter % 250) + 1}`;
  setEnv({
    TRUSTED_PROXIES: PROXY_CIDR,
    INVITE_ONLY: "0",
    INVITE_ALLOWLIST: "",
    LICHESS_CLIENT_ID: "chess-improver",
    LICHESS_AUTHORIZE_BASE_URL: fakes.url,
    LICHESS_API_BASE_URL: fakes.url,
  });
  vi.stubGlobal("fetch", async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input instanceof Request ? input.url : input);
    if (url.startsWith(fakes.url)) {
      const headers = new Headers(input instanceof Request ? input.headers : init?.headers);
      agents.push(headers.get("user-agent") ?? "");
      return realFetch(input, init);
    }
    throw new Error(`unexpected outbound request in test: ${url}`);
  });
});

afterEach(() => {
  vi.unstubAllGlobals();
  vi.unstubAllEnvs();
});

describe("lichessProvider", () => {
  it("is a PKCE provider with no client secret and the brief's endpoints", () => {
    const provider = lichessProvider(getEnv());
    expect(provider).toEqual(
      expect.objectContaining({
        providerId: "lichess",
        clientId: "chess-improver",
        authorizationUrl: `${fakes.url}/oauth`,
        tokenUrl: `${fakes.url}/api/token`,
        scopes: ["email:read"],
        pkce: true,
      }),
    );
    expect(provider.clientSecret).toBeUndefined();
  });

  it("sends the project User-Agent, with no email in it", async () => {
    const { USER_AGENT } = await import("../../../worker/src/http/platform-client");
    expect(LICHESS_USER_AGENT).toBe("chess-improver/0.1 (+https://github.com/chess-improver)");
    expect(LICHESS_USER_AGENT).toBe(USER_AGENT);
    expect(LICHESS_USER_AGENT).not.toMatch(/@|mailto/);
  });
});

describe("Lichess sign-in through the fake", () => {
  it("redirects with S256 PKCE, the client id, email:read and a state", async () => {
    const { url } = await start();
    expect(url.origin + url.pathname).toBe(`${fakes.url}/oauth`);
    expect(url.searchParams.get("code_challenge_method")).toBe("S256");
    expect(url.searchParams.get("client_id")).toBe("chess-improver");
    expect(url.searchParams.get("scope")).toBe("email:read");
    expect(url.searchParams.get("state")).toBeTruthy();
    expect(url.searchParams.get("code_challenge")).toHaveLength(43);
  });

  it("exchanges the code with the verifier and the client id, never a client secret", async () => {
    await roundTrip();
    const token = fakes.lastTokenRequest();
    expect(token?.get("code_verifier")).toBeTruthy();
    expect(token?.get("client_id")).toBe("chess-improver");
    expect(token?.get("client_secret") ?? "").toBe("");
  });

  it("creates the user, stores the token encrypted and links the Lichess account", async () => {
    const before = Date.now();
    const { location } = await roundTrip();
    expect(location.pathname).toBe("/welcome/age");

    const users = await db.select().from(user).where(eq(user.email, FAKE_LICHESS_EMAIL));
    expect(users).toHaveLength(1);
    const me = users[0];
    if (!me) throw new Error("no user");
    expect(me.emailVerified).toBe(false);

    const [row] = await lichessAccounts();
    expect(row?.userId).toBe(me.id);
    expect(row?.accountId).toBe("fakeuser");
    expect(row?.accessToken).toBeTruthy();
    expect(row?.accessToken).not.toBe("lio_test_token");
    expect(row?.accessToken).not.toContain("lio_test_token");
    const expires = row?.accessTokenExpiresAt?.getTime() ?? 0;
    expect(Math.abs(expires - (before + 365 * DAY))).toBeLessThan(60_000);

    const links = await linksOf(me.id);
    expect(links).toHaveLength(1);
    expect(links[0]?.account.platform).toBe("lichess");
    expect(links[0]?.account.username).toBe("fakeuser");
    expect(links[0]?.account.displayName).toBe("FakeUser");
    expect(links[0]?.link.method).toBe("lichess_oauth");
    expect(links[0]?.link.verifiedAt).toBeInstanceOf(Date);
  });

  it("calls the account endpoints with the project User-Agent", async () => {
    await roundTrip();
    // The token request plus /api/account and /api/account/email.
    expect(agents.length).toBeGreaterThanOrEqual(3);
    for (const agent of agents) expect(agent).toBe(LICHESS_USER_AGENT);
  });

  it("never puts the token in the redirect or a cookie", async () => {
    const { location, jar } = await roundTrip();
    expect(location.toString()).not.toContain("lio_test_token");
    for (const value of jar.values()) expect(value).not.toContain("lio_test_token");
  });

  it("cannot create an account under INVITE_ONLY, even for an invited address", async () => {
    setEnv({ INVITE_ONLY: "1" });
    await inviteEmail(db, FAKE_LICHESS_EMAIL);
    const { location } = await roundTrip();
    expect(location.pathname).toBe("/sign-in");
    expect(location.searchParams.get("error")).toBe("not_invited");
    expect(await db.select().from(user)).toHaveLength(0);
  });

  it("email collision is not auto-linked", async () => {
    const { user: ana } = await createTestSession(db, { email: FAKE_LICHESS_EMAIL });
    const { location } = await roundTrip();
    expect(`${location.pathname}${location.search}`).toBe("/sign-in?error=account_not_linked");
    expect(await lichessAccounts()).toHaveLength(0);
    expect(await linksOf(ana.id)).toHaveLength(0);
    expect(await db.select().from(user)).toHaveLength(1);
  });

  it("consent denied returns to sign-in with message", async () => {
    const { location } = await roundTrip({ loginHint: DENY_HINT });
    expect(`${location.pathname}${location.search}`).toBe("/sign-in?error=lichess_denied");
    expect(fakes.lastTokenRequest()).toBeNull();
    expect(await db.select().from(user)).toHaveLength(0);
  });

  it("a returning Lichess user signs in to the same account", async () => {
    await roundTrip();
    const { location } = await roundTrip();
    expect(location.pathname).toBe("/");
    expect(await db.select().from(user)).toHaveLength(1);
    expect(await lichessAccounts()).toHaveLength(1);
  });
});

describe("Connect Lichess from a live session (linkSocial)", () => {
  it("adds the link to the signed-in magic-link user and creates no new user", async () => {
    const { user: me, cookie } = await createTestSession(db, { email: "ana@example.com" });
    const { location } = await roundTrip({ sessionCookie: cookie });
    expect(`${location.pathname}${location.search}`).toBe("/app/accounts?connected=lichess");
    expect(await db.select().from(user)).toHaveLength(1);
    const rows = await lichessAccounts();
    expect(rows).toHaveLength(1);
    expect(rows[0]?.userId).toBe(me.id);
    const links = await linksOf(me.id);
    expect(links.map((l) => [l.account.username, l.link.method])).toEqual([
      ["fakeuser", "lichess_oauth"],
    ]);
    expect(links[0]?.link.verifiedAt).toBeInstanceOf(Date);
  });

  it("denied consent goes back to Accounts with the Lichess message", async () => {
    const { cookie } = await createTestSession(db, { email: "ana@example.com" });
    const { location } = await roundTrip({ sessionCookie: cookie, loginHint: DENY_HINT });
    expect(location.pathname).toBe("/app/accounts");
    expect(location.searchParams.getAll("error")).toEqual(["lichess_denied"]);
  });

  it("reconnecting after the link was removed restores it", async () => {
    const { user: me, cookie } = await createTestSession(db, { email: "ana@example.com" });
    await roundTrip({ sessionCookie: cookie });
    await db.delete(accountLinks).where(eq(accountLinks.userId, me.id));
    await roundTrip({ sessionCookie: cookie });
    expect(await linksOf(me.id)).toHaveLength(1);
    expect(await lichessAccounts()).toHaveLength(1);
  });

  it("shows the Lichess link in the account list with its token status", async () => {
    const { user: me, cookie } = await createTestSession(db, { email: "ana@example.com" });
    await roundTrip({ sessionCookie: cookie });
    const [view] = await listAccounts(db, me.id);
    expect(view).toEqual(
      expect.objectContaining({ platform: "lichess", username: "fakeuser", status: "ok" }),
    );
    expect(JSON.stringify(view)).not.toContain("lio_test_token");
    const soon = new Date(Date.now() + 360 * DAY);
    expect((await listAccounts(db, me.id, soon))[0]?.status).toBe("expiring");
    const later = new Date(Date.now() + 366 * DAY);
    expect((await listAccounts(db, me.id, later))[0]?.status).toBe("expired");
  });
});

describe("onLichessAccount", () => {
  it("moves the verified link to the newest OAuth owner", async () => {
    const first = await createTestSession(db, { email: "first@example.com" });
    const second = await createTestSession(db, { email: "second@example.com" });
    await onLichessAccount(db, {
      userId: first.user.id,
      lichessId: "fakeuser",
      username: "FakeUser",
    });
    await onLichessAccount(db, {
      userId: second.user.id,
      lichessId: "fakeuser",
      username: "FakeUser",
    });
    const [old] = await linksOf(first.user.id);
    const [current] = await linksOf(second.user.id);
    expect(old?.link.verifiedAt).toBeNull();
    expect(current?.link.verifiedAt).toBeInstanceOf(Date);
    expect(current?.link.method).toBe("lichess_oauth");
    const accounts = await db
      .select()
      .from(linkedAccounts)
      .where(and(eq(linkedAccounts.platform, "lichess"), eq(linkedAccounts.username, "fakeuser")));
    expect(accounts).toHaveLength(1);
  });

  it("is idempotent for the same user", async () => {
    const me = await createTestSession(db, { email: "ana@example.com" });
    const args = { userId: me.user.id, lichessId: "fakeuser", username: "FakeUser" };
    await onLichessAccount(db, args);
    await onLichessAccount(db, args);
    const links = await linksOf(me.user.id);
    expect(links).toHaveLength(1);
    expect(links[0]?.link.verifiedAt).toBeInstanceOf(Date);
  });
});

describe("lichessLinkStatus", () => {
  const expiry = new Date("2027-06-01T00:00:00Z");
  const at = (days: number) => new Date(expiry.getTime() + days * DAY);

  it("is ok 30 days before expiry, expiring 10 days before, expired 1 day after", () => {
    expect(lichessLinkStatus({ accessTokenExpiresAt: expiry }, at(-30))).toBe("ok");
    expect(lichessLinkStatus({ accessTokenExpiresAt: expiry }, at(-10))).toBe("expiring");
    expect(lichessLinkStatus({ accessTokenExpiresAt: expiry }, at(1))).toBe("expired");
  });

  it("treats a token without an expiry as ok", () => {
    expect(lichessLinkStatus({ accessTokenExpiresAt: null }, at(0))).toBe("ok");
  });
});

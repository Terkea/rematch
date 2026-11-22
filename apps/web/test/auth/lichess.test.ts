import { account, accountLinks, linkedAccounts, session, user } from "@ci/db";
import { and, eq } from "drizzle-orm";
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import {
  DENY_HINT,
  FAKE_LICHESS_EMAIL,
  FAKE_LICHESS_TOKEN,
  failRevokes,
  resetLichessFake,
  revokedTokens,
} from "../fakes/lichess";
import { type Fakes, startFakes } from "../fakes/main";
import { createTestSession, inviteEmail, resetAuthTables, webTestDb } from "../helpers/session";

vi.mock("../../src/server/mail", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../src/server/mail")>()),
  sendMail: vi.fn(async () => {}),
}));

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
const { lichessProvider, lichessLinkStatus, LICHESS_USER_AGENT, lichessPlaceholderEmail } =
  await import("../../src/server/auth/lichess");
const { sendMail } = await import("../../src/server/mail");
const { flushBackgroundTasks } = await import("../../src/server/auth/background");
const linkRoute = await import("../../src/app/api/accounts/[linkId]/route");
const mailed = vi.mocked(sendMail);
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
              errorCallbackURL: "/app/accounts",
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

/** The fake Lichess consent screen: returns the callback URL it sends the browser back to. */
async function consent(url: URL): Promise<URL> {
  const res = await realFetch(url, { redirect: "manual" });
  expect(res.status).toBe(302);
  const back = new URL(res.headers.get("location") ?? "");
  expect(back.origin + back.pathname).toBe(`${BASE}/api/auth/callback/lichess`);
  return back;
}

/** Our callback, opened with the given cookies. */
async function callback(back: URL, jar: Map<string, string>) {
  const res = await GET(
    new Request(back.toString(), { headers: { cookie: header(jar), "x-forwarded-for": client } }),
  );
  expect(res.status).toBe(302);
  const location = new URL(res.headers.get("location") ?? "", BASE);
  return { location, jar: cookiesFrom(res, jar) };
}

/** Plays the browser: authorisation URL → fake Lichess → back to our callback. */
async function roundTrip(opts: { sessionCookie?: string; loginHint?: string } = {}) {
  const { url, jar } = await start(opts);
  return callback(await consent(url), jar);
}

const sessionCookieOf = (jar: Map<string, string>) =>
  `ci.session_token=${jar.get("ci.session_token") ?? ""}`;

/** DELETE /api/accounts/:linkId as the browser would send it. */
function removeVia(cookie: string, linkId: string): Promise<Response> {
  return linkRoute.DELETE(
    new Request(`${BASE}/api/accounts/${linkId}`, {
      method: "DELETE",
      headers: { cookie, origin: BASE, "content-type": "application/json" },
      body: "{}",
    }),
    { params: Promise.resolve({ linkId }) },
  );
}

/** Requests a magic link for `email` and opens it; returns the verify response. */
async function magicLinkSignIn(email: string): Promise<Response> {
  mailed.mockClear();
  const res = await POST(
    new Request(`${BASE}/api/auth/sign-in/magic-link`, {
      method: "POST",
      headers: { "content-type": "application/json", origin: BASE, "x-forwarded-for": client },
      body: JSON.stringify({ email, callbackURL: "/", newUserCallbackURL: "/welcome/age" }),
    }),
  );
  expect(res.status).toBe(200);
  await flushBackgroundTasks();
  const text = mailed.mock.calls.at(-1)?.[0].text ?? "";
  const link = text.match(/https?:\/\/\S+/)?.[0];
  if (!link) throw new Error(`no magic link mailed to ${email}`);
  return GET(new Request(link, { headers: { "x-forwarded-for": client } }));
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
  mailed.mockClear();
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

afterEach(async () => {
  await flushBackgroundTasks();
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
        // No scope: a Lichess email is never used (ADR-WEB-4, review I5), so none is requested.
        scopes: [],
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
  it("redirects with S256 PKCE, the client id and a state, asking for no email", async () => {
    const { url } = await start();
    expect(url.origin + url.pathname).toBe(`${fakes.url}/oauth`);
    expect(url.searchParams.get("code_challenge_method")).toBe("S256");
    expect(url.searchParams.get("client_id")).toBe("chess-improver");
    expect(url.searchParams.get("scope") ?? "").not.toContain("email");
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

    // No email from Lichess is stored: the user gets a non-routable placeholder (review I5).
    const users = await db.select().from(user);
    expect(users).toHaveLength(1);
    const me = users[0];
    if (!me) throw new Error("no user");
    expect(me.email).toBe(lichessPlaceholderEmail("fakeuser"));
    expect(me.email).toMatch(/\.invalid$/);
    expect(me.email).not.toBe(FAKE_LICHESS_EMAIL);
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
    // The token request plus /api/account.
    expect(agents.length).toBeGreaterThanOrEqual(2);
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
    // Ana's address is the one her Lichess account would report; it is never looked at.
    const { user: ana } = await createTestSession(db, { email: FAKE_LICHESS_EMAIL });
    const { location } = await roundTrip();
    expect(location.pathname).toBe("/welcome/age");
    expect(await linksOf(ana.id)).toHaveLength(0);
    const rows = await lichessAccounts();
    expect(rows).toHaveLength(1);
    expect(rows[0]?.userId).not.toBe(ana.id);
  });

  it("a Lichess sign-up cannot pre-claim an address: the magic link lands in its own account", async () => {
    // INVITE_ONLY=0 (the beforeEach): Lichess may create an account.
    await roundTrip();
    const [lichessUser] = await db.select().from(user);
    if (!lichessUser) throw new Error("no Lichess user");

    const verify = await magicLinkSignIn(FAKE_LICHESS_EMAIL);
    expect(verify.status).toBe(302);
    const [owner] = await db.select().from(user).where(eq(user.email, FAKE_LICHESS_EMAIL));
    expect(owner).toBeDefined();
    expect(owner?.id).not.toBe(lichessUser.id);
    const ownerSessions = await db
      .select()
      .from(session)
      .where(eq(session.userId, owner?.id ?? lichessUser.id));
    expect(ownerSessions).toHaveLength(1);
    const lichessSessions = await db
      .select()
      .from(session)
      .where(eq(session.userId, lichessUser.id));
    expect(lichessSessions).toHaveLength(1);
  });

  it("never mails a magic link to a Lichess placeholder address", async () => {
    await roundTrip();
    mailed.mockClear();
    await POST(
      new Request(`${BASE}/api/auth/sign-in/magic-link`, {
        method: "POST",
        headers: { "content-type": "application/json", origin: BASE, "x-forwarded-for": client },
        body: JSON.stringify({ email: lichessPlaceholderEmail("fakeuser") }),
      }),
    );
    await flushBackgroundTasks();
    expect(mailed).not.toHaveBeenCalled();
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

  it("connecting again while linked refreshes the same account", async () => {
    const { user: me, cookie } = await createTestSession(db, { email: "ana@example.com" });
    await roundTrip({ sessionCookie: cookie });
    const { location } = await roundTrip({ sessionCookie: cookie });
    expect(location.searchParams.get("connected")).toBe("lichess");
    expect(await linksOf(me.id)).toHaveLength(1);
    expect(await lichessAccounts()).toHaveLength(1);
  });

  it("a Lichess account connected to another user is refused, with its own message", async () => {
    const first = await createTestSession(db, { email: "first@example.com" });
    const second = await createTestSession(db, { email: "second@example.com" });
    await roundTrip({ sessionCookie: first.cookie });
    const { location } = await roundTrip({ sessionCookie: second.cookie });
    expect(location.pathname).toBe("/app/accounts");
    expect(location.searchParams.getAll("error")).toEqual(["lichess_taken"]);
    // The first holder keeps it; the second gains nothing.
    const rows = await lichessAccounts();
    expect(rows.map((r) => r.userId)).toEqual([first.user.id]);
    const [kept] = await linksOf(first.user.id);
    expect(kept?.link.verifiedAt).toBeInstanceOf(Date);
    expect(await linksOf(second.user.id)).toHaveLength(0);
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
  it("never takes a verified link away from another user", async () => {
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
    const [kept] = await linksOf(first.user.id);
    const [other] = await linksOf(second.user.id);
    expect(kept?.link.verifiedAt).toBeInstanceOf(Date);
    expect(other?.link.verifiedAt ?? null).toBeNull();
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

describe("tokens never reach the browser (review I1)", () => {
  it("the token-returning and unlink endpoints are not mounted", async () => {
    const { cookie } = await createTestSession(db, { email: "ana@example.com" });
    await roundTrip({ sessionCookie: cookie });
    const [row] = await lichessAccounts();
    const headers = {
      cookie,
      origin: BASE,
      "content-type": "application/json",
      "x-forwarded-for": client,
    };
    const calls = [
      new Request(`${BASE}/api/auth/get-access-token`, {
        method: "POST",
        headers,
        body: JSON.stringify({ providerId: "lichess", accountId: row?.id }),
      }),
      new Request(`${BASE}/api/auth/refresh-token`, {
        method: "POST",
        headers,
        body: JSON.stringify({ providerId: "lichess", accountId: row?.id }),
      }),
      new Request(`${BASE}/api/auth/account-info?accountId=${row?.id}`, { headers }),
      new Request(`${BASE}/api/auth/unlink-account`, {
        method: "POST",
        headers,
        body: JSON.stringify({ providerId: "lichess" }),
      }),
    ];
    for (const req of calls) {
      const res = await (req.method === "GET" ? GET(req) : POST(req));
      expect(res.status, req.url).toBe(404);
      expect(await res.text()).not.toContain(FAKE_LICHESS_TOKEN);
    }
    expect(await lichessAccounts()).toHaveLength(1);
    const list = await GET(new Request(`${BASE}/api/auth/list-accounts`, { headers }));
    expect(await list.text()).not.toContain(FAKE_LICHESS_TOKEN);
  });
});

describe("removing a Lichess link (review I2)", () => {
  async function connected() {
    const me = await createTestSession(db, { email: "ana@example.com" });
    await roundTrip({ sessionCookie: me.cookie });
    const [view] = await listAccounts(db, me.user.id);
    if (!view) throw new Error("no link");
    return { ...me, linkId: view.linkId };
  }

  it("deletes the OAuth account and its token, and revokes the token at Lichess", async () => {
    const me = await connected();
    const res = await removeVia(me.cookie, me.linkId);
    expect(res.status).toBe(204);
    expect(await lichessAccounts()).toHaveLength(0);
    expect(await linksOf(me.user.id)).toHaveLength(0);
    await flushBackgroundTasks();
    expect(revokedTokens()).toEqual([FAKE_LICHESS_TOKEN]);
  });

  it.each([
    ["Lichess answers 500", () => failRevokes()],
    ["Lichess is unreachable", () => setEnv({ LICHESS_API_BASE_URL: "http://127.0.0.1:9" })],
  ])(
    "still deletes locally when revoking fails (%s), and logs no token",
    async (_case, breakIt) => {
      const me = await connected();
      const logged: unknown[] = [];
      const record = (...args: unknown[]) => {
        logged.push(args);
      };
      const spies = [
        vi.spyOn(console, "error").mockImplementation(record),
        vi.spyOn(console, "warn").mockImplementation(record),
        vi.spyOn(console, "log").mockImplementation(record),
        vi.spyOn(console, "info").mockImplementation(record),
      ];
      try {
        breakIt();
        const res = await removeVia(me.cookie, me.linkId);
        expect(res.status).toBe(204);
        await expect(flushBackgroundTasks()).resolves.toBeUndefined();
        expect(await lichessAccounts()).toHaveLength(0);
        expect(await linksOf(me.user.id)).toHaveLength(0);
        expect(revokedTokens()).toEqual([]);
        expect(
          JSON.stringify(logged, (_k, v) => (v instanceof Error ? v.message : v)),
        ).not.toContain(FAKE_LICHESS_TOKEN);
      } finally {
        for (const spy of spies) spy.mockRestore();
      }
    },
  );

  it("a later Continue with Lichess does not restore the link (INVITE_ONLY)", async () => {
    const me = await connected();
    await inviteEmail(db, "ana@example.com");
    await removeVia(me.cookie, me.linkId);
    setEnv({ INVITE_ONLY: "1" });
    const { location } = await roundTrip();
    expect(`${location.pathname}${location.search}`).toBe("/sign-in?error=not_invited");
    expect(await linksOf(me.user.id)).toHaveLength(0);
    expect(await lichessAccounts()).toHaveLength(0);
  });

  it("a later Continue with Lichess does not restore the link (open sign-up)", async () => {
    const me = await connected();
    await removeVia(me.cookie, me.linkId);
    await roundTrip();
    expect(await linksOf(me.user.id)).toHaveLength(0);
    const rows = await lichessAccounts();
    expect(rows.map((r) => r.userId)).not.toContain(me.user.id);
  });

  it("refuses to remove Lichess when it is the account's only way to sign in", async () => {
    const { jar } = await roundTrip();
    const [lichessUser] = await db.select().from(user);
    if (!lichessUser) throw new Error("no Lichess user");
    // Past the age screen, so the route answers for this user.
    await db.update(user).set({ birthMonth: "1990-01" }).where(eq(user.id, lichessUser.id));
    const [view] = await listAccounts(db, lichessUser.id);
    const res = await removeVia(sessionCookieOf(jar), view?.linkId ?? "");
    expect(res.status).toBe(409);
    expect(await res.json()).toEqual({ error: "last_sign_in_method" });
    expect(await lichessAccounts()).toHaveLength(1);
    expect(revokedTokens()).toEqual([]);
  });
});

describe("state binds the callback to the browser that started it (review I4)", () => {
  it("a callback without the state cookie is refused and creates no user", async () => {
    const { url, jar } = await start();
    const back = await consent(url);
    const withoutState = new Map([...jar].filter(([name]) => !name.includes("state")));
    expect(withoutState.size).toBeLessThan(jar.size);
    const { location } = await callback(back, withoutState);
    expect(`${location.pathname}${location.search}`).toBe("/sign-in?error=lichess_failed");
    expect(await db.select().from(user)).toHaveLength(0);
    expect(await lichessAccounts()).toHaveLength(0);
  });

  it("a replayed callback is refused", async () => {
    const { url, jar } = await start();
    const back = await consent(url);
    const first = await callback(back, jar);
    expect(first.location.pathname).toBe("/welcome/age");
    const replay = await callback(back, jar);
    expect(`${replay.location.pathname}${replay.location.search}`).toBe(
      "/sign-in?error=lichess_failed",
    );
    expect(await db.select().from(user)).toHaveLength(1);
  });

  it("a link started by an attacker attaches nothing when the victim's browser completes it", async () => {
    const attacker = await createTestSession(db, { email: "mallory@example.com" });
    const victim = await createTestSession(db, { email: "victim@example.com" });
    const { url } = await start({ sessionCookie: attacker.cookie });
    const back = await consent(url);
    const { location } = await callback(back, jarOf(victim.cookie));
    expect(location.searchParams.get("error")).toBe("lichess_failed");
    expect(await lichessAccounts()).toHaveLength(0);
    expect(await linksOf(victim.user.id)).toHaveLength(0);
    expect(await linksOf(attacker.user.id)).toHaveLength(0);
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

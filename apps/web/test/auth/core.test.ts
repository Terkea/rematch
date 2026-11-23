import { invites, session, user, verification } from "@ci/db";
import { eq, sql } from "drizzle-orm";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createTestSession, inviteEmail, resetAuthTables, webTestDb } from "../helpers/session";

vi.mock("../../src/server/mail", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../src/server/mail")>()),
  sendMail: vi.fn(async () => {}),
}));

const db = webTestDb();
const { sendMail } = await import("../../src/server/mail");
const { GET, POST } = await import("../../src/app/api/auth/[...all]/route");
const { mapAuthResponse, resetAuth } = await import("../../src/server/auth");
const {
  droppedBackgroundTasks,
  flushBackgroundTasks,
  MAX_PENDING_TASKS,
  MAX_RUNNING_TASKS,
  pendingBackgroundTasks,
  runAfterResponse,
} = await import("../../src/server/auth/background");
const { googleProvider } = await import("../../src/server/auth/google");
const { normaliseEmail } = await import("../../src/server/auth/invite");
const { admissionPolicy, secureCookies } = await import("../../src/server/auth/policy");
const { requireApiUser, getSession } = await import("../../src/server/auth/session");
const { getEnv, resetEnvCache } = await import("../../src/env");

const BASE = "http://localhost:3100";
/**
 * The test harness plays a trusted reverse proxy (the compose network). Like a real one it appends
 * the address it saw to X-Forwarded-For, so the header reads `<anything the client sent>, client`.
 */
const PROXY_CIDR = "172.16.0.0/12";
const mailed = vi.mocked(sendMail);
let clientCounter = 0;
/** The magic-link rate limit is per client IP, so every test is its own client. */
let client = "203.0.113.1";

function viaProxy(from = client): Record<string, string> {
  return { "x-forwarded-for": from };
}

/** POSTs a magic-link request without waiting for the mail to go out. */
function postLink(email: string, headers: Record<string, string> = viaProxy()): Promise<Response> {
  return POST(
    new Request(`${BASE}/api/auth/sign-in/magic-link`, {
      method: "POST",
      headers: { "content-type": "application/json", origin: BASE, ...headers },
      body: JSON.stringify({ email }),
    }),
  );
}

/** POSTs a magic-link request and lets the detached delivery finish. */
async function requestLink(email: string, from = client): Promise<Response> {
  const res = await postLink(email, viaProxy(from));
  await flushBackgroundTasks();
  return res;
}

function open(url: string, headers: Record<string, string> = {}): Promise<Response> {
  return GET(new Request(url, { headers: { ...viaProxy(), ...headers } }));
}

function lastLink(): string {
  const text = mailed.mock.calls.at(-1)?.[0].text ?? "";
  const url = text.match(/https?:\/\/\S+/)?.[0];
  if (!url) throw new Error(`no link in mail: ${text}`);
  return url;
}

function setEnv(values: Record<string, string>): void {
  for (const [key, value] of Object.entries(values)) vi.stubEnv(key, value);
  resetEnvCache();
  resetAuth();
}

function setInviteOnly(on: boolean): void {
  setEnv({ INVITE_ONLY: on ? "1" : "0" });
}

// Requests that are not flushed inside a test must not leak delivery work into the next one.
afterEach(() => flushBackgroundTasks());

beforeEach(async () => {
  await resetAuthTables(db);
  mailed.mockClear();
  clientCounter += 1;
  client = `203.0.${Math.floor(clientCounter / 250)}.${(clientCounter % 250) + 1}`;
  setEnv({ INVITE_ONLY: "1", TRUSTED_PROXIES: PROXY_CIDR, INVITE_ALLOWLIST: "" });
});

describe("magic link (invited address)", () => {
  beforeEach(async () => {
    await inviteEmail(db, "ana@example.com");
  });

  it("mails the link to that address", async () => {
    const res = await requestLink("ana@example.com");
    expect(res.status).toBe(200);
    expect(mailed).toHaveBeenCalledTimes(1);
    const msg = mailed.mock.calls[0]?.[0];
    expect(msg?.to).toBe("ana@example.com");
    expect(msg?.subject).toBe("Your sign-in link");
    expect(msg?.text).toContain(
      "Use this link to sign in to Rematch. It works once and expires in 10 minutes.",
    );
    expect(lastLink()).toContain("/api/auth/magic-link/verify");
  });

  it("creates a uuid user and sets a ci. session cookie when opened", async () => {
    await requestLink("ana@example.com");
    const res = await open(lastLink());
    expect(res.status).toBe(302);
    const [row] = await db.select().from(user).where(eq(user.email, "ana@example.com"));
    expect(row?.id).toMatch(
      /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/,
    );
    const cookies = res.headers.getSetCookie();
    expect(cookies.some((c) => c.startsWith("ci.session_token="))).toBe(true);
  });

  it("reused magic link is rejected", async () => {
    await requestLink("ana@example.com");
    const link = lastLink();
    await open(link);
    const sessionsAfterFirst = await db.select().from(session);
    const again = await open(link);
    expect(again.status).toBe(302);
    expect(again.headers.get("location")).toBe(`${BASE}/sign-in?error=link_expired`);
    const issued = again.headers.getSetCookie().filter((c) => /^ci\.session_token=[^;]+/.test(c));
    expect(issued).toEqual([]);
    expect(await db.select().from(session)).toHaveLength(sessionsAfterFirst.length);
  });

  it("returns 429 for the fourth request within 10 minutes", async () => {
    for (let i = 0; i < 3; i += 1) expect((await requestLink("ana@example.com")).status).toBe(200);
    expect((await requestLink("ana@example.com")).status).toBe(429);
  });
});

describe("invite-only (INVITE_ONLY on, the default)", () => {
  it("answers an uninvited address exactly like an invited one, but sends nothing", async () => {
    await inviteEmail(db, "ana@example.com");
    const invited = await requestLink("ana@example.com");
    const stranger = await requestLink("stranger@example.com", "192.0.2.1");
    expect(stranger.status).toBe(invited.status);
    expect(await stranger.json()).toEqual(await invited.json());
    expect(mailed).toHaveBeenCalledTimes(1);
    expect(mailed.mock.calls[0]?.[0].to).toBe("ana@example.com");
  });

  it("accepts addresses from INVITE_ALLOWLIST, case-insensitively", async () => {
    vi.stubEnv("INVITE_ALLOWLIST", "Dev@Example.invalid");
    resetEnvCache();
    resetAuth();
    await requestLink("dev@example.invalid");
    expect(mailed).toHaveBeenCalledTimes(1);
  });

  it("refuses to create an account whose invite was withdrawn, with a neutral message", async () => {
    await inviteEmail(db, "ana@example.com");
    await requestLink("ana@example.com");
    await db.delete(invites);
    const res = await open(lastLink());
    expect(res.headers.get("location")).toBe(`${BASE}/sign-in?error=not_invited`);
    expect(await db.select().from(user)).toHaveLength(0);
  });

  it("refuses a session for an existing account that is no longer invited", async () => {
    await inviteEmail(db, "ana@example.com");
    await requestLink("ana@example.com");
    await open(lastLink());
    await requestLink("ana@example.com");
    const second = lastLink();
    await db.delete(invites);
    await db.delete(session);
    const res = await open(second);
    expect(res.headers.get("location")).toBe(`${BASE}/sign-in?error=not_invited`);
    expect(await db.select().from(session)).toHaveLength(0);
  });
});

describe("mapAuthResponse", () => {
  const redirect = (location: string) =>
    new Response(null, {
      status: 302,
      headers: { location, "set-cookie": "ci.state=; Max-Age=0" },
    });

  it("sends a Google sign-in refused by the invite gate to the neutral sign-in message", async () => {
    const req = new Request(`${BASE}/api/auth/callback/google?code=x&state=y`);
    const res = await mapAuthResponse(req, redirect(`${BASE}/sign-in?error=not_invited`), BASE);
    expect(res.headers.get("location")).toBe(`${BASE}/sign-in?error=not_invited`);
    expect(res.headers.getSetCookie()).toEqual(["ci.state=; Max-Age=0"]);
  });

  it("leaves other OAuth errors and successful redirects alone", async () => {
    const req = new Request(`${BASE}/api/auth/callback/google`);
    const denied = await mapAuthResponse(
      req,
      redirect(`${BASE}/sign-in?error=access_denied`),
      BASE,
    );
    expect(denied.headers.get("location")).toBe(`${BASE}/sign-in?error=access_denied`);
    const ok = await mapAuthResponse(req, redirect(`${BASE}/app`), BASE);
    expect(ok.headers.get("location")).toBe(`${BASE}/app`);
  });
});

describe("open sign-up (INVITE_ONLY=0)", () => {
  it("mails any address and creates the account", async () => {
    setInviteOnly(false);
    await requestLink("stranger@example.com");
    expect(mailed).toHaveBeenCalledTimes(1);
    const res = await open(lastLink());
    expect(res.status).toBe(302);
    expect(await db.select().from(user).where(eq(user.email, "stranger@example.com"))).toHaveLength(
      1,
    );
  });
});

describe("googleProvider", () => {
  it("is empty without both env vars and has a google key with them", () => {
    expect(googleProvider({})).toEqual({});
    expect(googleProvider({ GOOGLE_CLIENT_ID: "id" })).toEqual({});
    expect(
      Object.keys(googleProvider({ GOOGLE_CLIENT_ID: "id", GOOGLE_CLIENT_SECRET: "secret" })),
    ).toEqual(["google"]);
  });
});

describe("requireApiUser", () => {
  it("returns 401 without a cookie", async () => {
    const res = await requireApiUser(new Request(`${BASE}/api/x`));
    expect(res).toBeInstanceOf(Response);
    expect((res as Response).status).toBe(401);
    expect(await (res as Response).json()).toEqual({ error: "unauthenticated" });
  });

  it("returns 403 age_required while birthMonth is null", async () => {
    const { cookie } = await createTestSession(db, { birthMonth: null });
    const res = await requireApiUser(new Request(`${BASE}/api/x`, { headers: { cookie } }));
    expect((res as Response).status).toBe(403);
    expect(await (res as Response).json()).toEqual({ error: "age_required" });
  });

  it("returns the user once the age is known", async () => {
    const { user: created, cookie } = await createTestSession(db, { email: "dev@example.invalid" });
    const res = await requireApiUser(new Request(`${BASE}/api/x`, { headers: { cookie } }));
    expect(res).toEqual(created);
    const current = await getSession(new Headers({ cookie }));
    expect(current?.user).toEqual(created);
    expect(current?.session.expiresAt).toBeInstanceOf(Date);
  });
});

describe("verification table", () => {
  it("expires magic links after 10 minutes", async () => {
    await inviteEmail(db, "ana@example.com");
    const before = Date.now();
    await requestLink("ana@example.com");
    const [row] = await db.select().from(verification);
    const ms = (row?.expiresAt.getTime() ?? 0) - before;
    expect(ms).toBeGreaterThan(590_000);
    expect(ms).toBeLessThan(610_000);
  });
});

describe("detached delivery", () => {
  it("answers invited and uninvited addresses on the same path, before any delivery work", async () => {
    await inviteEmail(db, "ana@example.com");
    const invited = await postLink("ana@example.com");
    expect(pendingBackgroundTasks()).toBe(1);
    const stranger = await postLink("stranger@example.com", viaProxy("198.51.100.7"));
    expect(pendingBackgroundTasks()).toBe(2);
    // Nothing has been looked up or sent yet: both responses were produced before delivery ran.
    expect(mailed).not.toHaveBeenCalled();
    expect([invited.status, await invited.json()]).toEqual([200, { status: true }]);
    expect([stranger.status, await stranger.json()]).toEqual([200, { status: true }]);
    await flushBackgroundTasks();
    expect(mailed.mock.calls.map(([msg]) => msg.to)).toEqual(["ana@example.com"]);
  });

  it("mails the normalised address", async () => {
    await inviteEmail(db, "ana@example.com");
    await requestLink("Ana@Example.COM");
    expect(mailed.mock.calls[0]?.[0].to).toBe("ana@example.com");
  });
});

describe("per-email throttle", () => {
  const sends = async () =>
    (await db.execute<{ n: number }>(sql`select count(*)::int as n from magic_link_sends`))[0]?.n;

  it("sends at most 3 links per address in 10 minutes, whatever the client IP", async () => {
    await inviteEmail(db, "ana@example.com");
    const responses = [];
    for (let i = 1; i <= 5; i += 1)
      responses.push(await requestLink("ana@example.com", `198.51.100.${i}`));
    expect(responses.map((r) => r.status)).toEqual([200, 200, 200, 200, 200]);
    expect(mailed).toHaveBeenCalledTimes(3);
  });

  for (const burst of [4, 150]) {
    it(`sends exactly 3 links for ${burst} concurrent requests`, async () => {
      await inviteEmail(db, "ana@example.com");
      const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
      const responses = await Promise.all(
        Array.from({ length: burst }, (_, i) =>
          postLink("ana@example.com", viaProxy(`198.51.${Math.floor(i / 250)}.${(i % 250) + 1}`)),
        ),
      );
      await flushBackgroundTasks();
      warn.mockRestore();
      expect(new Set(responses.map((r) => r.status))).toEqual(new Set([200]));
      expect(mailed).toHaveBeenCalledTimes(3);
      expect(await sends()).toBe(3);
    });
  }

  it("does not count throttled requests, so the lockout ends a window after the third send", async () => {
    await inviteEmail(db, "ana@example.com");
    for (let i = 1; i <= 6; i += 1) await requestLink("ana@example.com", `198.51.100.${i}`);
    expect(mailed).toHaveBeenCalledTimes(3);
    expect(await sends()).toBe(3);
    // The window passes for the three real sends; the three throttled requests left no trace.
    await db.execute(sql`update magic_link_sends set sent_at = now() - interval '11 minutes'`);
    await requestLink("ana@example.com", "198.51.100.9");
    expect(mailed).toHaveBeenCalledTimes(4);
    expect(await sends()).toBe(1);
  });

  it("fails the test when delivery breaks, instead of only logging", async () => {
    await inviteEmail(db, "ana@example.com");
    const error = vi.spyOn(console, "error").mockImplementation(() => {});
    mailed.mockRejectedValueOnce(new Error("smtp down"));
    await expect(requestLink("ana@example.com")).rejects.toThrow("smtp down");
    error.mockRestore();
  });
});

describe("background queue", () => {
  it("runs at most 4 tasks at once and drops (and counts) beyond 100 pending", async () => {
    expect([MAX_RUNNING_TASKS, MAX_PENDING_TASKS]).toEqual([4, 100]);
    let release = () => {};
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    let running = 0;
    let peak = 0;
    const droppedBefore = droppedBackgroundTasks();
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    const accepted = Array.from({ length: 105 }, () =>
      runAfterResponse("test", async () => {
        running += 1;
        peak = Math.max(peak, running);
        await gate;
        running -= 1;
      }),
    );
    expect(accepted.filter(Boolean)).toHaveLength(100);
    expect(droppedBackgroundTasks() - droppedBefore).toBe(5);
    expect(warn).toHaveBeenCalledTimes(5);
    await new Promise((resolve) => setImmediate(resolve));
    expect(running).toBe(4);
    release();
    await flushBackgroundTasks();
    expect(peak).toBe(4);
    expect(pendingBackgroundTasks()).toBe(0);
    warn.mockRestore();
  });
});

describe("client IP for the rate limit", () => {
  it("ignores X-Forwarded-For when no proxy is trusted (one shared bucket)", async () => {
    setEnv({ TRUSTED_PROXIES: "" });
    const statuses = [];
    for (let i = 1; i <= 4; i += 1) {
      statuses.push(
        (await postLink("x@example.com", { "x-forwarded-for": `192.0.2.${i}` })).status,
      );
    }
    expect(statuses).toEqual([200, 200, 200, 429]);
  });

  it("takes the address the trusted proxy appended, not a forged left-hand entry", async () => {
    const statuses = [];
    for (let i = 1; i <= 4; i += 1) {
      const forged = `192.0.2.${i}, ${client}`;
      statuses.push((await postLink("x@example.com", { "x-forwarded-for": forged })).status);
    }
    expect(statuses).toEqual([200, 200, 200, 429]);
  });
});

describe("origin and CSRF checks (forced on, as in production)", () => {
  it("rejects a magic-link request with an untrusted callbackURL (open redirect)", async () => {
    const res = await POST(
      new Request(`${BASE}/api/auth/sign-in/magic-link`, {
        method: "POST",
        headers: { "content-type": "application/json", origin: BASE, ...viaProxy() },
        body: JSON.stringify({ email: "ana@example.com", callbackURL: "https://evil.example/x" }),
      }),
    );
    expect(res.status).toBe(403);
    expect(pendingBackgroundTasks()).toBe(0);
  });

  it("rejects verifying with an untrusted callbackURL", async () => {
    const res = await open(
      `${BASE}/api/auth/magic-link/verify?token=t&callbackURL=https://evil.example/`,
    );
    expect(res.status).toBe(403);
  });

  it("rejects a cross-site request that carries cookies (CSRF)", async () => {
    const res = await postLink("ana@example.com", {
      ...viaProxy(),
      origin: "https://evil.example",
      cookie: "ci.session_token=x",
    });
    expect(res.status).toBe(403);
    expect(pendingBackgroundTasks()).toBe(0);
  });

  it("rejects a cross-site navigation sign-in (login CSRF)", async () => {
    const res = await postLink("ana@example.com", {
      ...viaProxy(),
      origin: "https://evil.example",
      "sec-fetch-site": "cross-site",
      "sec-fetch-mode": "navigate",
      "sec-fetch-dest": "document",
    });
    expect(res.status).toBe(403);
    expect(pendingBackgroundTasks()).toBe(0);
  });
});

describe("admission policy (validateUserInfo)", () => {
  const policy = () => admissionPolicy(db, getEnv());
  const oauth = { method: "oauth" as const, oauth: { providerId: "google" } };
  const ctx = {} as never;

  it("refuses an OAuth identity with an unverified email, for sign-up and for linking", async () => {
    await inviteEmail(db, "ana@example.com");
    const unverified = { email: "ana@example.com", emailVerified: false };
    for (const action of ["create-user", "link-account"] as const) {
      expect(await policy()({ user: unverified, source: { ...oauth, action } }, ctx)).toEqual(
        expect.objectContaining({ error: "email_not_verified" }),
      );
    }
    expect(
      await policy()(
        { user: { email: "ana@example.com" }, source: { ...oauth, action: "link-account" } },
        ctx,
      ),
    ).toEqual(expect.objectContaining({ error: "email_not_verified" }));
  });

  it("admits a verified, invited OAuth identity and refuses a verified, uninvited one", async () => {
    await inviteEmail(db, "ana@example.com");
    const source = { ...oauth, action: "create-user" as const };
    expect(
      await policy()({ user: { email: "ana@example.com", emailVerified: true }, source }, ctx),
    ).toBe(undefined);
    expect(
      await policy()({ user: { email: "eve@example.com", emailVerified: true }, source }, ctx),
    ).toEqual(expect.objectContaining({ error: "not_invited" }));
  });

  it("lets a live session link a provider whose email is unverified, but not someone else's", async () => {
    const source = { ...oauth, action: "link-account" as const };
    const owner = admissionPolicy(db, getEnv(), async () => "user-1");
    const stranger = admissionPolicy(db, getEnv(), async () => "user-2");
    const unverified = { id: "user-1", email: "ana@example.com", emailVerified: false };
    expect(await owner({ user: unverified, source }, ctx)).toBe(undefined);
    expect(await stranger({ user: unverified, source }, ctx)).toEqual(
      expect.objectContaining({ error: "email_not_verified" }),
    );
    // A live session never exempts creating an account.
    expect(
      await owner({ user: unverified, source: { ...oauth, action: "create-user" } }, ctx),
    ).toEqual(expect.objectContaining({ error: "email_not_verified" }));
  });

  it("does not use the email of providers that do not assert one (Lichess, chess.com)", async () => {
    await inviteEmail(db, "ana@example.com");
    for (const providerId of ["lichess", "chesscom"]) {
      const source = { method: "oauth" as const, oauth: { providerId } };
      const user = { id: "user-1", email: "ana@example.com", emailVerified: false };
      // Linking is not gated on their email, but only the account's own live session may link
      // them: an implicit link by email is refused (T17 final review) ...
      expect(await policy()({ user, source: { ...source, action: "link-account" } }, ctx)).toEqual(
        expect.objectContaining({ error: "account_not_linked" }),
      );
      const owner = admissionPolicy(db, getEnv(), async () => "user-1");
      const stranger = admissionPolicy(db, getEnv(), async () => "user-2");
      expect(await owner({ user, source: { ...source, action: "link-account" } }, ctx)).toBe(
        undefined,
      );
      expect(await stranger({ user, source: { ...source, action: "link-account" } }, ctx)).toEqual(
        expect.objectContaining({ error: "account_not_linked" }),
      );
      // ... and under INVITE_ONLY their email cannot prove an invite.
      expect(
        await policy()(
          { user: { ...user, emailVerified: true }, source: { ...source, action: "create-user" } },
          ctx,
        ),
      ).toEqual(expect.objectContaining({ error: "not_invited" }));
    }
  });

  it("maps an unverified Google identity to a sign-in message", async () => {
    const req = new Request(`${BASE}/api/auth/callback/google`);
    const res = await mapAuthResponse(
      req,
      new Response(null, {
        status: 302,
        headers: { location: `${BASE}/?error=email_not_verified` },
      }),
      BASE,
    );
    expect(res.headers.get("location")).toBe(`${BASE}/sign-in?error=email_not_verified`);
  });
});

describe("withdrawing an invite", () => {
  it("deletes that user's sessions", async () => {
    await inviteEmail(db, "ana@example.com");
    const { cookie } = await createTestSession(db, { email: "ana@example.com" });
    const other = await createTestSession(db, { email: "bo@example.com" });
    await db.delete(invites).where(eq(invites.email, "ana@example.com"));
    expect(
      (await requireApiUser(new Request(`${BASE}/api/x`, { headers: { cookie } }))) as Response,
    ).toHaveProperty("status", 401);
    const stillIn = await requireApiUser(
      new Request(`${BASE}/api/x`, { headers: { cookie: other.cookie } }),
    );
    expect(stillIn).toEqual(other.user);
  });
});

describe("changing an invite's email", () => {
  it("deletes the sessions of the old address, and only on an email change", async () => {
    await inviteEmail(db, "ana@example.com");
    const { cookie } = await createTestSession(db, { email: "ana@example.com" });
    const ask = () => requireApiUser(new Request(`${BASE}/api/x`, { headers: { cookie } }));
    await db.update(invites).set({ note: "renamed" }).where(eq(invites.email, "ana@example.com"));
    expect(await ask()).not.toBeInstanceOf(Response);
    await db
      .update(invites)
      .set({ email: "ana2@example.com" })
      .where(eq(invites.email, "ana@example.com"));
    expect(((await ask()) as Response).status).toBe(401);
  });
});

describe("normaliseEmail", () => {
  it("applies NFKC and rejects anything that is not ASCII afterwards", () => {
    expect(normaliseEmail(" Ana@Example.COM ")).toBe("ana@example.com");
    expect(normaliseEmail("ａｎａ@example.com")).toBe("ana@example.com");
    expect(normaliseEmail("аna@example.com")).toBe(null);
    expect(normaliseEmail("")).toBe(null);
  });
});

describe("secure cookies", () => {
  const env = (url: string) => ({ ...getEnv(), BETTER_AUTH_URL: url });

  it("follows the base URL: https gives __Secure-ci.session_token", () => {
    expect(secureCookies(env("https://chess.example"), "production")).toBe(true);
    expect(secureCookies(env("http://localhost:3100"), "development")).toBe(false);
  });

  it("requires https in production except on a local host", () => {
    expect(() => secureCookies(env("http://chess.example"), "production")).toThrow(/https/);
    expect(secureCookies(env("http://localhost:3100"), "production")).toBe(false);
  });
});

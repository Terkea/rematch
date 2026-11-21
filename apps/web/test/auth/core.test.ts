import { invites, session, user, verification } from "@ci/db";
import { eq } from "drizzle-orm";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createTestSession, inviteEmail, resetAuthTables, webTestDb } from "../helpers/session";

vi.mock("../../src/server/mail", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../src/server/mail")>()),
  sendMail: vi.fn(async () => {}),
}));

const db = webTestDb();
const { sendMail } = await import("../../src/server/mail");
const { GET, POST } = await import("../../src/app/api/auth/[...all]/route");
const { mapAuthResponse, resetAuth } = await import("../../src/server/auth");
const { googleProvider } = await import("../../src/server/auth/google");
const { requireApiUser, getSession } = await import("../../src/server/auth/session");
const { resetEnvCache } = await import("../../src/env");

const BASE = "http://localhost:3100";
const mailed = vi.mocked(sendMail);
let ipCounter = 0;
/** The magic-link rate limit is per IP and path, so every test gets its own client address. */
let ip = "10.0.0.1";

function requestLink(email: string, from = ip): Promise<Response> {
  return POST(
    new Request(`${BASE}/api/auth/sign-in/magic-link`, {
      method: "POST",
      headers: { "content-type": "application/json", origin: BASE, "x-forwarded-for": from },
      body: JSON.stringify({ email }),
    }),
  );
}

function open(url: string): Promise<Response> {
  return GET(new Request(url, { headers: { "x-forwarded-for": ip } }));
}

function lastLink(): string {
  const text = mailed.mock.calls.at(-1)?.[0].text ?? "";
  const url = text.match(/https?:\/\/\S+/)?.[0];
  if (!url) throw new Error(`no link in mail: ${text}`);
  return url;
}

function setInviteOnly(on: boolean): void {
  vi.stubEnv("INVITE_ONLY", on ? "1" : "0");
  resetEnvCache();
  resetAuth();
}

beforeEach(async () => {
  await resetAuthTables(db);
  mailed.mockClear();
  ipCounter += 1;
  ip = `10.0.${Math.floor(ipCounter / 250)}.${(ipCounter % 250) + 1}`;
  setInviteOnly(true);
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
      "Use this link to sign in to chess-improver. It works once and expires in 10 minutes.",
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

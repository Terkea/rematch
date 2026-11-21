import { accountLinks, invites, linkedAccounts, session, user } from "@ci/db";
import { eq, sql } from "drizzle-orm";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createTestSession, inviteEmail, resetAuthTables, webTestDb } from "../helpers/session";

vi.mock("../../src/server/mail", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../src/server/mail")>()),
  sendMail: vi.fn(async () => {}),
}));

const db = webTestDb();
const { sendMail } = await import("../../src/server/mail");
const {
  AGE_BLOCK_COOKIE,
  AGE_BLOCK_MAX_AGE,
  AgeAlreadyRecordedError,
  ageBand,
  ageFromBirthMonth,
  canEnableMarketing,
  InvalidBirthMonthError,
  isAgeGatedPath,
  isMinor,
  parseBirthMonth,
  recordAge,
} = await import("../../src/server/age");
const authRoute = await import("../../src/app/api/auth/[...all]/route");
const ageRoute = await import("../../src/app/api/age/route");
const { resetAuth } = await import("../../src/server/auth");
const { flushBackgroundTasks } = await import("../../src/server/auth/background");
const { resetEnvCache } = await import("../../src/env");

const BASE = "http://localhost:3100";
const TODAY = new Date("2026-11-21T12:00:00Z");
const mailed = vi.mocked(sendMail);

function postAge(body: unknown, headers: Record<string, string> = {}): Promise<Response> {
  return ageRoute.POST(
    new Request(`${BASE}/api/age`, {
      method: "POST",
      headers: { "content-type": "application/json", origin: BASE, ...headers },
      body: typeof body === "string" ? body : JSON.stringify(body),
    }),
  );
}

/** A platform account shared by every user who links it, plus this user's link to it. */
async function linkAccount(userId: string, username: string) {
  const [shared] = await db
    .insert(linkedAccounts)
    .values({ platform: "chesscom", username, displayName: username })
    .returning();
  if (!shared) throw new Error("linked account insert returned no row");
  await db.insert(accountLinks).values({ userId, linkedAccountId: shared.id, method: "username" });
  return shared;
}

afterEach(() => flushBackgroundTasks());

beforeEach(async () => {
  await resetAuthTables(db);
  await db.execute(sql`truncate table linked_accounts cascade`);
  mailed.mockClear();
  vi.unstubAllEnvs();
  vi.stubEnv("INVITE_ONLY", "1");
  vi.stubEnv("INVITE_ALLOWLIST", "");
  resetEnvCache();
  resetAuth();
});

describe("ageFromBirthMonth (conservative: birthday on the last day of the month)", () => {
  it.each([
    ["2013-11", 12],
    ["2013-10", 13],
    ["2008-11", 17],
    ["2008-10", 18],
  ])("%s is %i on 2026-11-21", (birthMonth, age) => {
    expect(ageFromBirthMonth(birthMonth, TODAY)).toBe(age);
  });

  it("turns a year older only on the last day of the birth month", () => {
    expect(ageFromBirthMonth("2013-11", new Date("2026-11-29T23:59:59Z"))).toBe(12);
    expect(ageFromBirthMonth("2013-11", new Date("2026-11-30T00:00:00Z"))).toBe(13);
  });

  it("never overstates a leap-day birthday", () => {
    // Born some day in February 2012 (a leap year): 14 only from 29 February, so 1 March in 2026.
    expect(ageFromBirthMonth("2012-02", new Date("2026-02-28T12:00:00Z"))).toBe(13);
    expect(ageFromBirthMonth("2012-02", new Date("2026-03-01T00:00:00Z"))).toBe(14);
  });

  it("rejects a malformed value", () => {
    expect(() => ageFromBirthMonth("2013-13", TODAY)).toThrow(InvalidBirthMonthError);
    expect(() => ageFromBirthMonth("13-1", TODAY)).toThrow(InvalidBirthMonthError);
  });
});

describe("ageBand", () => {
  it("splits at 13 and 18", () => {
    expect(ageBand(12)).toBe("under13");
    expect(ageBand(13)).toBe("teen");
    expect(ageBand(17)).toBe("teen");
    expect(ageBand(18)).toBe("adult");
  });
});

describe("parseBirthMonth", () => {
  it("returns YYYY-MM", () => {
    expect(parseBirthMonth({ year: 1990, month: 3 }, TODAY)).toBe("1990-03");
    expect(parseBirthMonth({ year: 2026, month: 11 }, TODAY)).toBe("2026-11");
    expect(parseBirthMonth({ year: 1900, month: 1 }, TODAY)).toBe("1900-01");
  });

  it.each([
    [{ year: 2026, month: 12 }, "a month in the future"],
    [{ year: 2027, month: 1 }, "a year in the future"],
    [{ year: 1899, month: 5 }, "a year before 1900"],
    [{ year: 2000, month: 0 }, "month 0"],
    [{ year: 2000, month: 13 }, "month 13"],
    [{ year: 2000.5, month: 1 }, "a fractional year"],
    [{ year: 2000, month: Number.NaN }, "NaN"],
  ])("throws for %o (%s)", (input) => {
    expect(() => parseBirthMonth(input, TODAY)).toThrow(InvalidBirthMonthError);
  });
});

describe("isMinor and canEnableMarketing", () => {
  const base = {
    id: "u",
    email: "dev@example.invalid",
    name: "Dev",
    marketingEmail: false,
    publicShareDefault: false,
  };

  it("treats an unknown age as a minor", () => {
    expect(isMinor({ ...base, birthMonth: null }, TODAY)).toBe(true);
    expect(canEnableMarketing({ ...base, birthMonth: null }, TODAY)).toBe(false);
  });

  it("lets only adults opt in to marketing", () => {
    expect(isMinor({ ...base, birthMonth: "2008-11" }, TODAY)).toBe(true);
    expect(canEnableMarketing({ ...base, birthMonth: "2008-11" }, TODAY)).toBe(false);
    expect(isMinor({ ...base, birthMonth: "2008-10" }, TODAY)).toBe(false);
    expect(canEnableMarketing({ ...base, birthMonth: "2008-10" }, TODAY)).toBe(true);
  });
});

describe("recordAge", () => {
  it("deletes an under-13 at once, keeping only the shared platform accounts", async () => {
    const email = "young@example.invalid";
    await inviteEmail(db, email);
    const { user: created } = await createTestSession(db, { email, birthMonth: null });
    const shared = await linkAccount(created.id, "someplayer");
    await db.execute(sql`insert into magic_link_sends (email) values (${email})`);

    const result = await recordAge({ db }, created.id, { year: 2014, month: 1 }, TODAY);

    expect(result).toEqual({ band: "under13", redirectTo: "/blocked" });
    expect(await db.select().from(user).where(eq(user.id, created.id))).toHaveLength(0);
    expect(await db.select().from(user).where(eq(user.email, email))).toHaveLength(0);
    expect(await db.select().from(session).where(eq(session.userId, created.id))).toHaveLength(0);
    expect(
      await db.select().from(accountLinks).where(eq(accountLinks.userId, created.id)),
    ).toHaveLength(0);
    expect(
      await db.select().from(linkedAccounts).where(eq(linkedAccounts.id, shared.id)),
    ).toHaveLength(1);
    // No other copy of the address survives: the invite and the send log go too.
    expect(await db.select().from(invites).where(eq(invites.email, email))).toHaveLength(0);
    const sends = await db.execute(sql`select 1 from magic_link_sends where email = ${email}`);
    expect(sends).toHaveLength(0);
  });

  it("gives a teen the private defaults", async () => {
    const { user: created } = await createTestSession(db, {
      birthMonth: null,
      marketingEmail: true,
      publicShareDefault: true,
    });

    const result = await recordAge({ db }, created.id, { year: 2010, month: 3 }, TODAY);

    expect(result).toEqual({ band: "teen", redirectTo: "/app/accounts" });
    const [row] = await db.select().from(user).where(eq(user.id, created.id));
    expect(row?.birthMonth).toBe("2010-03");
    expect(row?.ageCheckedAt?.toISOString()).toBe(TODAY.toISOString());
    expect(row?.marketingEmail).toBe(false);
    expect(row?.publicShareDefault).toBe(false);
    if (!row) throw new Error("teen row missing");
    expect(canEnableMarketing({ ...created, birthMonth: row.birthMonth }, TODAY)).toBe(false);
  });

  it("records an adult's month and keeps both preferences off", async () => {
    const { user: created } = await createTestSession(db, { birthMonth: null });

    const result = await recordAge({ db }, created.id, { year: 1990, month: 6 }, TODAY);

    expect(result).toEqual({ band: "adult", redirectTo: "/app/accounts" });
    const [row] = await db.select().from(user).where(eq(user.id, created.id));
    expect(row?.birthMonth).toBe("1990-06");
    expect(row?.ageCheckedAt?.toISOString()).toBe(TODAY.toISOString());
    expect(row?.marketingEmail).toBe(false);
    expect(row?.publicShareDefault).toBe(false);
  });

  it("answers only once: a recorded age cannot be changed or used to delete", async () => {
    const { user: created } = await createTestSession(db, { birthMonth: "2010-03" });
    await expect(
      recordAge({ db }, created.id, { year: 1990, month: 1 }, TODAY),
    ).rejects.toBeInstanceOf(AgeAlreadyRecordedError);
    await expect(
      recordAge({ db }, created.id, { year: 2020, month: 1 }, TODAY),
    ).rejects.toBeInstanceOf(AgeAlreadyRecordedError);
    const [row] = await db.select().from(user).where(eq(user.id, created.id));
    expect(row?.birthMonth).toBe("2010-03");
  });

  it("validates before touching the database", async () => {
    const { user: created } = await createTestSession(db, { birthMonth: null });
    await expect(
      recordAge({ db }, created.id, { year: 2026, month: 12 }, TODAY),
    ).rejects.toBeInstanceOf(InvalidBirthMonthError);
    const [row] = await db.select().from(user).where(eq(user.id, created.id));
    expect(row?.birthMonth).toBeNull();
  });
});

describe("POST /api/age", () => {
  it("blocks an under-13 for 30 days and clears the session cookie", async () => {
    vi.useFakeTimers({ toFake: ["Date"], now: TODAY });
    try {
      const { user: created, cookie } = await createTestSession(db, { birthMonth: null });
      const res = await postAge({ year: 2014, month: 1 }, { cookie });

      expect(res.status).toBe(200);
      expect(await res.json()).toEqual({ redirectTo: "/blocked" });
      const cookies = res.headers.getSetCookie();
      const block = cookies.find((c) => c.startsWith(`${AGE_BLOCK_COOKIE}=`));
      expect(block).toBe(
        `ci_age_block=1; Max-Age=${AGE_BLOCK_MAX_AGE}; Path=/; HttpOnly; SameSite=Lax`,
      );
      expect(AGE_BLOCK_MAX_AGE).toBe(2592000);
      expect(cookies).toContain("ci.session_token=; Max-Age=0; Path=/; HttpOnly; SameSite=Lax");
      expect(await db.select().from(user).where(eq(user.id, created.id))).toHaveLength(0);
    } finally {
      vi.useRealTimers();
    }
  });

  it("marks the block cookie Secure when the app runs over https", async () => {
    vi.stubEnv("BETTER_AUTH_URL", "https://chess.example.invalid");
    vi.stubEnv("BETTER_AUTH_TRUSTED_ORIGINS", "");
    resetEnvCache();
    resetAuth();
    const { cookie } = await createTestSession(db, { birthMonth: null });
    const secureCookie = cookie.replace("ci.session_token=", "__Secure-ci.session_token=");
    const res = await ageRoute.POST(
      new Request("https://chess.example.invalid/api/age", {
        method: "POST",
        headers: {
          "content-type": "application/json",
          origin: "https://chess.example.invalid",
          cookie: secureCookie,
        },
        body: JSON.stringify({ year: 2020, month: 1 }),
      }),
    );
    expect(res.status).toBe(200);
    const cookies = res.headers.getSetCookie();
    expect(cookies).toContain(
      "ci_age_block=1; Max-Age=2592000; Path=/; HttpOnly; SameSite=Lax; Secure",
    );
    expect(cookies).toContain(
      "__Secure-ci.session_token=; Max-Age=0; Path=/; HttpOnly; SameSite=Lax; Secure",
    );
  });

  it("sends teens and adults on without a block cookie", async () => {
    const { cookie } = await createTestSession(db, { birthMonth: null });
    const res = await postAge({ year: 2010, month: 3 }, { cookie });
    expect(res.status).toBe(200);
    expect(await res.json()).toEqual({ redirectTo: "/app/accounts" });
    expect(res.headers.getSetCookie()).toEqual([]);
  });

  it("returns 401 without a session", async () => {
    const res = await postAge({ year: 1990, month: 1 });
    expect(res.status).toBe(401);
    expect(await res.json()).toEqual({ error: "unauthenticated" });
  });

  it("refuses a cross-site or missing Origin with 403 before reading the session", async () => {
    const { user: created, cookie } = await createTestSession(db, { birthMonth: null });
    const foreign = await postAge(
      { year: 2014, month: 1 },
      { cookie, origin: "https://evil.test" },
    );
    expect(foreign.status).toBe(403);
    expect(await foreign.json()).toEqual({ error: "invalid_origin" });
    const missing = await ageRoute.POST(
      new Request(`${BASE}/api/age`, {
        method: "POST",
        headers: { "content-type": "application/json", cookie },
        body: JSON.stringify({ year: 2014, month: 1 }),
      }),
    );
    expect(missing.status).toBe(403);
    // The under-13 answer did not go through: the account still exists.
    expect(await db.select().from(user).where(eq(user.id, created.id))).toHaveLength(1);
  });

  it("refuses a body that is not declared JSON with 415", async () => {
    const { cookie } = await createTestSession(db, { birthMonth: null });
    const res = await postAge("year=2014&month=1", {
      cookie,
      "content-type": "application/x-www-form-urlencoded",
    });
    expect(res.status).toBe(415);
  });

  it("returns 400 invalid_birth_month for a bad or missing value", async () => {
    const { cookie } = await createTestSession(db, { birthMonth: null });
    for (const body of [{ year: 2026, month: 12 }, { year: "1990", month: 1 }, {}, "not json"]) {
      const res = await postAge(body, { cookie });
      expect(res.status).toBe(400);
      expect(await res.json()).toEqual({ error: "invalid_birth_month" });
    }
  });

  it("returns 409 once the age is recorded", async () => {
    const { cookie } = await createTestSession(db, { birthMonth: "1990-01" });
    const res = await postAge({ year: 2014, month: 1 }, { cookie });
    expect(res.status).toBe(409);
    expect(await res.json()).toEqual({ error: "age_already_recorded" });
  });
});

describe("the age block on sign-in", () => {
  it("covers the sign-in, sign-up, magic-link and OAuth paths", () => {
    for (const path of [
      "/sign-in/magic-link",
      "/sign-in/social",
      "/sign-in/oauth2",
      "/sign-up/email",
      "/magic-link/verify",
      "/oauth2/callback/lichess",
      "/callback/google",
    ]) {
      expect(isAgeGatedPath(path), path).toBe(true);
    }
    for (const path of ["/get-session", "/sign-out", "/sign-in", "/ok"]) {
      expect(isAgeGatedPath(path), path).toBe(false);
    }
  });

  it("refuses a magic link with 403 AGE_BLOCKED and sends no mail", async () => {
    await inviteEmail(db, "ana@example.com");
    const res = await authRoute.POST(
      new Request(`${BASE}/api/auth/sign-in/magic-link`, {
        method: "POST",
        headers: { "content-type": "application/json", origin: BASE, cookie: "ci_age_block=1" },
        body: JSON.stringify({ email: "ana@example.com" }),
      }),
    );
    await flushBackgroundTasks();
    expect(res.status).toBe(403);
    expect(((await res.json()) as { code?: string }).code).toBe("AGE_BLOCKED");
    expect(mailed).not.toHaveBeenCalled();
  });

  it("refuses to open a magic link in a blocked browser", async () => {
    const res = await authRoute.GET(
      new Request(`${BASE}/api/auth/magic-link/verify?token=whatever`, {
        headers: { cookie: "other=1; ci_age_block=1" },
      }),
    );
    expect(res.status).toBe(403);
    expect(((await res.json()) as { code?: string }).code).toBe("AGE_BLOCKED");
  });

  it("refuses social sign-in in a blocked browser", async () => {
    const res = await authRoute.POST(
      new Request(`${BASE}/api/auth/sign-in/social`, {
        method: "POST",
        headers: { "content-type": "application/json", origin: BASE, cookie: "ci_age_block=1" },
        body: JSON.stringify({ provider: "google" }),
      }),
    );
    expect(res.status).toBe(403);
    expect(((await res.json()) as { code?: string }).code).toBe("AGE_BLOCKED");
  });

  it("lets an unblocked browser through", async () => {
    await inviteEmail(db, "ana@example.com");
    const res = await authRoute.POST(
      new Request(`${BASE}/api/auth/sign-in/magic-link`, {
        method: "POST",
        headers: { "content-type": "application/json", origin: BASE, cookie: "ci_theme=night" },
        body: JSON.stringify({ email: "ana@example.com" }),
      }),
    );
    await flushBackgroundTasks();
    expect(res.status).toBe(200);
    expect(mailed).toHaveBeenCalledTimes(1);
  });
});

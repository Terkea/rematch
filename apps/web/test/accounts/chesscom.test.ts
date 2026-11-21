import { readFileSync } from "node:fs";
import { createServer, type Server } from "node:http";
import type { AddressInfo } from "node:net";
import { accountLinks, chesscomVerifications, games, linkedAccounts } from "@ci/db";
import { and, eq, sql } from "drizzle-orm";
import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { createTestSession, resetAuthTables, webTestDb } from "../helpers/session";

const db = webTestDb();
const { CHESSCOM_USER_AGENT, ChesscomUnavailableError, fetchChesscomProfile } = await import(
  "../../src/server/chesscom/profile"
);
const { checkChesscomVerification, generateVerificationCode, LinkError, startChesscomLink } =
  await import("../../src/server/accounts/chesscom-link");
const { listAccounts } = await import("../../src/server/accounts/list");
const { removeLink } = await import("../../src/server/accounts/remove");
const { resetEnvCache, getEnv } = await import("../../src/env");
const { resetAuth } = await import("../../src/server/auth");
const { oauthProviders } = await import("../../src/server/auth/oauth-providers");
const accountsRoute = await import("../../src/app/api/accounts/route");
const chesscomRoute = await import("../../src/app/api/accounts/chesscom/route");
const verifyRoute = await import("../../src/app/api/accounts/chesscom/[linkId]/verify/route");
const linkRoute = await import("../../src/app/api/accounts/[linkId]/route");

/** A recorded-shape public profile (synthetic values; never a real user's data). */
const HIKARU = JSON.parse(
  readFileSync(new URL("../fixtures/chesscom/player-hikaru.json", import.meta.url), "utf8"),
) as Record<string, unknown>;

const BASE_URL = "https://api.chess.test";
const MINUTE = 60_000;
const DAY = 24 * 3600 * 1000;

/**
 * Fake chess.com Published-Data API: `profiles` maps lower-case usernames to their location.
 * Unknown names get 404; `status` overrides the next responses.
 */
function fakeChesscom() {
  const profiles = new Map<string, string | null>([["hikaru", "Florida"]]);
  const statuses: number[] = [];
  const calls: { url: string; headers: Headers }[] = [];
  const fetchFn = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input);
    calls.push({ url, headers: new Headers(init?.headers) });
    const forced = statuses.shift();
    if (forced) return new Response("{}", { status: forced });
    const name = decodeURIComponent(url.split("/pub/player/")[1] ?? "");
    if (!profiles.has(name))
      return Response.json({ code: 0, message: "not found" }, { status: 404 });
    const display = name === "hikaru" ? "Hikaru" : name;
    return Response.json({
      ...HIKARU,
      "@id": `https://api.chess.com/pub/player/${name}`,
      url: `https://www.chess.com/member/${display}`,
      username: name,
      location: profiles.get(name),
    });
  }) as unknown as typeof fetch & { mock: { calls: unknown[] } };
  return { profiles, statuses, calls, fetch: fetchFn };
}

let chesscom = fakeChesscom();
const noSleep = async () => {};
let clock = new Date("2026-11-21T12:00:00Z");
let nextCode: string | undefined;

function deps() {
  return {
    db,
    chesscom: { fetch: chesscom.fetch, baseUrl: BASE_URL, sleep: noSleep },
    now: () => clock,
    ...(nextCode ? { generateCode: () => nextCode as string } : {}),
  };
}

async function newUser(): Promise<{ id: string; cookie: string }> {
  const { user, cookie } = await createTestSession(db);
  return { id: user.id, cookie };
}

function setEnv(values: Record<string, string>): void {
  for (const [key, value] of Object.entries(values)) vi.stubEnv(key, value);
  resetEnvCache();
  resetAuth();
}

beforeEach(async () => {
  await resetAuthTables(db);
  await db.execute(sql`truncate table linked_accounts cascade`);
  chesscom = fakeChesscom();
  clock = new Date("2026-11-21T12:00:00Z");
  nextCode = undefined;
  setEnv({ CHESSCOM_ENABLED: "1", CHESSCOM_OAUTH_ENABLED: "" });
});

describe("linking tables (migration 20261121010100_web_linking)", () => {
  async function account(username = "hikaru"): Promise<string> {
    const [row] = await db
      .insert(linkedAccounts)
      .values({ platform: "chesscom", username, displayName: username })
      .returning();
    if (!row) throw new Error("no linked account");
    return row.id;
  }

  it("lets two users hold unverified links to one account, but only one verified owner", async () => {
    const id = await account();
    const [a, b] = [await newUser(), await newUser()];
    await db.insert(accountLinks).values([
      { userId: a.id, linkedAccountId: id, method: "username" },
      { userId: b.id, linkedAccountId: id, method: "username" },
    ]);
    await db.update(accountLinks).set({ verifiedAt: clock }).where(eq(accountLinks.userId, a.id));
    await expect(
      db.update(accountLinks).set({ verifiedAt: clock }).where(eq(accountLinks.userId, b.id)),
    ).rejects.toThrow();
  });

  it("refuses a second link from the same user to the same account", async () => {
    const id = await account();
    const a = await newUser();
    await db.insert(accountLinks).values({ userId: a.id, linkedAccountId: id, method: "username" });
    await expect(
      db.insert(accountLinks).values({ userId: a.id, linkedAccountId: id, method: "username" }),
    ).rejects.toThrow();
  });

  it("deleting a user cascades its links and codes, not the shared account", async () => {
    const id = await account();
    const a = await newUser();
    const [link] = await db
      .insert(accountLinks)
      .values({ userId: a.id, linkedAccountId: id, method: "username" })
      .returning();
    await db.insert(chesscomVerifications).values({
      accountLinkId: link?.id ?? "",
      code: "CI-7K3QXM",
      expiresAt: new Date(clock.getTime() + 7 * DAY),
    });
    await db.execute(sql`delete from "user" where id = ${a.id}`);
    expect(await db.select().from(accountLinks)).toHaveLength(0);
    expect(await db.select().from(chesscomVerifications)).toHaveLength(0);
    expect(await db.select().from(linkedAccounts)).toHaveLength(1);
  });

  it("checks the code format and defaults checks to 0", async () => {
    const id = await account();
    const a = await newUser();
    const [link] = await db
      .insert(accountLinks)
      .values({ userId: a.id, linkedAccountId: id, method: "username" })
      .returning();
    const [row] = await db
      .insert(chesscomVerifications)
      .values({ accountLinkId: link?.id ?? "", code: "CI-7K3QXM", expiresAt: clock })
      .returning();
    expect(row?.checks).toBe(0);
    await expect(
      db
        .update(chesscomVerifications)
        .set({ code: "CI-0O1IL0" })
        .where(eq(chesscomVerifications.id, row?.id ?? "")),
    ).rejects.toThrow();
  });
});

describe("fetchChesscomProfile", () => {
  const profileDeps = () => ({ fetch: chesscom.fetch, baseUrl: BASE_URL, sleep: noSleep });

  it("sends the project User-Agent and no email address in any header", async () => {
    await fetchChesscomProfile("Hikaru", profileDeps());
    const call = chesscom.calls[0];
    expect(call?.url).toBe(`${BASE_URL}/pub/player/hikaru`);
    expect(call?.headers.get("user-agent")).toBe(
      "chess-improver-ci (+https://github.com/chess-improver)",
    );
    expect(CHESSCOM_USER_AGENT).toBe(call?.headers.get("user-agent"));
    for (const [, value] of call?.headers ?? []) expect(value).not.toMatch(/@|mailto/);
  });

  it("returns the profile with its display name and location", async () => {
    expect(await fetchChesscomProfile("hikaru", profileDeps())).toEqual({
      username: "hikaru",
      displayName: "Hikaru",
      location: "Florida",
    });
  });

  it("returns null on 404", async () => {
    expect(await fetchChesscomProfile("nobody_here", profileDeps())).toBeNull();
  });

  it("throws ChesscomUnavailableError on a persistent 429, after one backed-off retry", async () => {
    chesscom.statuses.push(429, 429);
    await expect(fetchChesscomProfile("hikaru", profileDeps())).rejects.toBeInstanceOf(
      ChesscomUnavailableError,
    );
    expect(chesscom.calls).toHaveLength(2);
  });

  it("succeeds when the retry after a 429 is answered", async () => {
    chesscom.statuses.push(429);
    const sleeps: number[] = [];
    const sleep = async (ms: number) => {
      sleeps.push(ms);
    };
    expect(await fetchChesscomProfile("hikaru", { ...profileDeps(), sleep })).not.toBeNull();
    expect(sleeps).toEqual([1000]);
  });

  it.each([403, 500, 503])("throws ChesscomUnavailableError on %i", async (status) => {
    chesscom.statuses.push(status);
    await expect(fetchChesscomProfile("hikaru", profileDeps())).rejects.toBeInstanceOf(
      ChesscomUnavailableError,
    );
  });

  it("throws ChesscomUnavailableError on a timeout", async () => {
    const hang = ((_url: string, init?: RequestInit) =>
      new Promise((_resolve, reject) => {
        init?.signal?.addEventListener("abort", () => reject(init.signal?.reason));
      })) as unknown as typeof fetch;
    await expect(
      fetchChesscomProfile("hikaru", { fetch: hang, baseUrl: BASE_URL, timeoutMs: 50 }),
    ).rejects.toBeInstanceOf(ChesscomUnavailableError);
  });

  it("sends requests one at a time", async () => {
    let inFlight = 0;
    let most = 0;
    const slow = (async () => {
      inFlight += 1;
      most = Math.max(most, inFlight);
      await new Promise((resolve) => setTimeout(resolve, 10));
      inFlight -= 1;
      return Response.json({ username: "hikaru", location: null });
    }) as unknown as typeof fetch;
    await Promise.all(
      [1, 2, 3].map(() => fetchChesscomProfile("hikaru", { fetch: slow, baseUrl: BASE_URL })),
    );
    expect(most).toBe(1);
  });
});

describe("generateVerificationCode", () => {
  it("is CI- plus 6 characters without look-alikes", () => {
    for (let i = 0; i < 200; i++)
      expect(generateVerificationCode()).toMatch(/^CI-[2-9A-HJKMNP-Z]{6}$/);
    expect(generateVerificationCode(() => 0)).toBe("CI-222222");
    expect(generateVerificationCode(() => 0.999999)).toBe("CI-ZZZZZZ");
  });
});

describe("startChesscomLink", () => {
  it('stores "Hikaru" as "hikaru" and returns the same code on a second call', async () => {
    const a = await newUser();
    const first = await startChesscomLink(deps(), a.id, "Hikaru");
    expect(first).toEqual({
      linkId: expect.any(String),
      username: "hikaru",
      code: expect.stringMatching(/^CI-[2-9A-HJKMNP-Z]{6}$/),
      verified: false,
    });
    const [stored] = await db.select().from(linkedAccounts);
    expect(stored).toMatchObject({
      platform: "chesscom",
      username: "hikaru",
      displayName: "Hikaru",
    });
    const [link] = await db.select().from(accountLinks);
    expect(link).toMatchObject({ method: "username", verifiedAt: null });
    const [code] = await db.select().from(chesscomVerifications);
    expect(code?.expiresAt.getTime()).toBe(clock.getTime() + 7 * DAY);

    const again = await startChesscomLink(deps(), a.id, "HIKARU");
    expect(again).toEqual(first);
    expect(await db.select().from(accountLinks)).toHaveLength(1);
  });

  it('refuses "ab" with invalid_username and an unknown name with not_found', async () => {
    const a = await newUser();
    for (const name of ["ab", "a".repeat(26), "bad name", "x/../y"]) {
      await expect(startChesscomLink(deps(), a.id, name)).rejects.toMatchObject({
        code: "invalid_username",
      });
    }
    expect(chesscom.calls).toHaveLength(0);
    await expect(startChesscomLink(deps(), a.id, "nobody_here")).rejects.toMatchObject({
      code: "not_found",
    });
    await expect(startChesscomLink(deps(), a.id, "nobody_here")).rejects.toBeInstanceOf(LinkError);
  });

  it("allows 3 chess.com links per user and refuses the 4th with limit_reached", async () => {
    const a = await newUser();
    for (const name of ["one_1", "two_2", "three_3", "four_4"]) chesscom.profiles.set(name, null);
    for (const name of ["one_1", "two_2", "three_3"]) await startChesscomLink(deps(), a.id, name);
    await expect(startChesscomLink(deps(), a.id, "four_4")).rejects.toMatchObject({
      code: "limit_reached",
    });
    // Re-asking for an existing link is not a new one.
    await expect(startChesscomLink(deps(), a.id, "two_2")).resolves.toMatchObject({
      username: "two_2",
    });
  });

  it("unverified links do not block others", async () => {
    const [a, b] = [await newUser(), await newUser()];
    const linkA = await startChesscomLink(deps(), a.id, "hikaru");
    const linkB = await startChesscomLink(deps(), b.id, "Hikaru");
    expect(linkA.linkId).not.toBe(linkB.linkId);
    expect(await db.select().from(linkedAccounts)).toHaveLength(1);
  });

  it("issues a fresh code once the old one has expired", async () => {
    const a = await newUser();
    const first = await startChesscomLink(deps(), a.id, "hikaru");
    clock = new Date(clock.getTime() + 8 * DAY);
    nextCode = "CI-7K3QXM";
    const second = await startChesscomLink(deps(), a.id, "hikaru");
    expect(second.linkId).toBe(first.linkId);
    expect(second.code).toBe("CI-7K3QXM");
  });
});

describe("checkChesscomVerification", () => {
  async function started(userId: string, code = "CI-7K3QXM") {
    nextCode = code;
    const link = await startChesscomLink(deps(), userId, "hikaru");
    nextCode = undefined;
    return link;
  }

  it("is pending without the code, makes no request within 60 s, and verifies once the code is in the location", async () => {
    const a = await newUser();
    const link = await started(a.id);
    const fetchesBefore = chesscom.calls.length;

    const pending = await checkChesscomVerification(deps(), a.id, link.linkId, clock);
    expect(pending).toEqual({ status: "pending", nextCheckAt: new Date(clock.getTime() + MINUTE) });
    expect(chesscom.calls.length).toBe(fetchesBefore + 1);

    chesscom.profiles.set("hikaru", "Paris CI-7K3QXM ");
    const soon = new Date(clock.getTime() + 30_000);
    expect(await checkChesscomVerification(deps(), a.id, link.linkId, soon)).toEqual({
      status: "pending",
      nextCheckAt: new Date(clock.getTime() + MINUTE),
    });
    expect(chesscom.calls.length).toBe(fetchesBefore + 1);

    const later = new Date(clock.getTime() + MINUTE);
    const verified = await checkChesscomVerification(deps(), a.id, link.linkId, later);
    expect(verified.status).toBe("verified");
    const [row] = await db.select().from(accountLinks);
    expect(row).toMatchObject({ method: "location_code", verifiedAt: later });
    const [code] = await db.select().from(chesscomVerifications);
    expect(code?.checks).toBe(2);
  });

  it("matches the code case-insensitively anywhere in the trimmed location", async () => {
    const a = await newUser();
    const link = await started(a.id);
    chesscom.profiles.set("hikaru", "  verify: ci-7k3qxm  ");
    expect((await checkChesscomVerification(deps(), a.id, link.linkId, clock)).status).toBe(
      "verified",
    );
  });

  it("newest verified owner wins", async () => {
    const [a, b] = [await newUser(), await newUser()];
    const linkA = await started(a.id, "CI-AAAAAA");
    const linkB = await started(b.id, "CI-BBBBBB");
    chesscom.profiles.set("hikaru", "CI-AAAAAA");
    expect((await checkChesscomVerification(deps(), a.id, linkA.linkId, clock)).status).toBe(
      "verified",
    );
    chesscom.profiles.set("hikaru", "CI-BBBBBB");
    const later = new Date(clock.getTime() + 2 * MINUTE);
    expect((await checkChesscomVerification(deps(), b.id, linkB.linkId, later)).status).toBe(
      "verified",
    );
    const [rowA] = await db.select().from(accountLinks).where(eq(accountLinks.userId, a.id));
    const [rowB] = await db.select().from(accountLinks).where(eq(accountLinks.userId, b.id));
    expect(rowA?.verifiedAt).toBeNull();
    expect(rowB?.verifiedAt).toEqual(later);
  });

  it("concurrent checks of one link make one request", async () => {
    const a = await newUser();
    const link = await started(a.id);
    const fetchesBefore = chesscom.calls.length;
    await Promise.all(
      [1, 2, 3, 4].map(() => checkChesscomVerification(deps(), a.id, link.linkId, clock)),
    );
    expect(chesscom.calls.length).toBe(fetchesBefore + 1);
  });

  it("concurrent starts never exceed 3 links", async () => {
    const a = await newUser();
    const names = ["one_1", "two_2", "three_3", "four_4", "five_5"];
    for (const name of names) chesscom.profiles.set(name, null);
    const results = await Promise.allSettled(
      names.map((name) => startChesscomLink(deps(), a.id, name)),
    );
    expect(results.filter((r) => r.status === "fulfilled")).toHaveLength(3);
    expect(await db.select().from(accountLinks)).toHaveLength(3);
  });

  it("gives expired after 7 days, without a request", async () => {
    const a = await newUser();
    const link = await started(a.id);
    chesscom.profiles.set("hikaru", "CI-7K3QXM");
    const fetchesBefore = chesscom.calls.length;
    const late = new Date(clock.getTime() + 7 * DAY);
    expect((await checkChesscomVerification(deps(), a.id, link.linkId, late)).status).toBe(
      "expired",
    );
    expect(chesscom.calls.length).toBe(fetchesBefore);
  });

  it("refuses another user's link with link_not_found", async () => {
    const [a, b] = [await newUser(), await newUser()];
    const link = await started(a.id);
    await expect(checkChesscomVerification(deps(), b.id, link.linkId, clock)).rejects.toMatchObject(
      { code: "link_not_found" },
    );
  });
});

describe("listAccounts and removeLink", () => {
  it("lists a pending link with its code and a verified one as ok", async () => {
    const a = await newUser();
    chesscom.profiles.set("second_2", null);
    nextCode = "CI-7K3QXM";
    const pending = await startChesscomLink(deps(), a.id, "hikaru");
    nextCode = "CI-BBBBBB";
    const other = await startChesscomLink(deps(), a.id, "second_2");
    chesscom.profiles.set("second_2", "CI-BBBBBB");
    await checkChesscomVerification(deps(), a.id, other.linkId, clock);

    expect(await listAccounts(db, a.id, clock)).toEqual([
      {
        linkId: pending.linkId,
        platform: "chesscom",
        username: "hikaru",
        displayName: "Hikaru",
        verified: false,
        status: "pending_verification",
        code: "CI-7K3QXM",
      },
      {
        linkId: other.linkId,
        platform: "chesscom",
        username: "second_2",
        displayName: "second_2",
        verified: true,
        status: "ok",
      },
    ]);
    const late = new Date(clock.getTime() + 8 * DAY);
    expect((await listAccounts(db, a.id, late))[0]).toMatchObject({ status: "expired" });
    expect((await listAccounts(db, a.id, late))[0]).not.toHaveProperty("code");
  });

  it("removeLink leaves the linked_accounts row (and its games) in place", async () => {
    const [a, b] = [await newUser(), await newUser()];
    const link = await startChesscomLink(deps(), a.id, "hikaru");
    const linkB = await startChesscomLink(deps(), b.id, "hikaru");
    const [shared] = await db.select().from(linkedAccounts);
    await db.insert(games).values({
      accountId: shared?.id ?? "",
      platform: "chesscom",
      platformGameId: "g1",
      pgn: "1. e4 *",
      playedAt: clock,
      timeClass: "blitz",
      rated: true,
      playerColor: "white",
      whiteName: "hikaru",
      blackName: "x",
      result: "*",
    });

    await expect(removeLink(db, b.id, link.linkId)).rejects.toMatchObject({
      code: "link_not_found",
    });
    await removeLink(db, a.id, link.linkId);
    expect(await db.select().from(accountLinks).where(eq(accountLinks.userId, a.id))).toEqual([]);
    expect(
      await db
        .select()
        .from(accountLinks)
        .where(and(eq(accountLinks.userId, b.id), eq(accountLinks.id, linkB.linkId))),
    ).toHaveLength(1);
    expect(await db.select().from(linkedAccounts)).toHaveLength(1);
    expect(await db.select().from(games)).toHaveLength(1);
  });
});

describe("routes", () => {
  // A local mock of chess.com's API: the routes use the real fetch against CHESSCOM_API_BASE_URL.
  let server: Server;
  let serverUrl = "";
  const locations = new Map<string, string | null>();
  let forcedStatus: number | undefined;
  const seenAgents: string[] = [];

  beforeAll(async () => {
    server = createServer((req, res) => {
      seenAgents.push(String(req.headers["user-agent"]));
      const name = decodeURIComponent((req.url ?? "").split("/pub/player/")[1] ?? "");
      if (forcedStatus) {
        res.writeHead(forcedStatus).end("{}");
        return;
      }
      if (!locations.has(name)) {
        res.writeHead(404, { "content-type": "application/json" }).end("{}");
        return;
      }
      res.writeHead(200, { "content-type": "application/json" });
      res.end(JSON.stringify({ ...HIKARU, username: name, location: locations.get(name) }));
    });
    await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
    serverUrl = `http://127.0.0.1:${(server.address() as AddressInfo).port}`;
  });
  afterAll(() => new Promise<void>((resolve) => server.close(() => resolve())));

  beforeEach(() => {
    locations.clear();
    locations.set("hikaru", "Florida");
    forcedStatus = undefined;
    setEnv({ CHESSCOM_ENABLED: "1", CHESSCOM_API_BASE_URL: serverUrl });
  });

  const BASE = "http://localhost:3100";
  const json = (body: unknown, cookie: string, method = "POST") =>
    ({
      method,
      headers: { "content-type": "application/json", cookie, origin: BASE },
      body: JSON.stringify(body),
    }) satisfies RequestInit;
  const params = (linkId: string) => ({ params: Promise.resolve({ linkId }) });

  async function post(body: unknown, cookie: string): Promise<Response> {
    return chesscomRoute.POST(new Request(`${BASE}/api/accounts/chesscom`, json(body, cookie)));
  }

  it("links, verifies, lists and removes through the API", async () => {
    const a = await newUser();
    const created = await post({ username: "Hikaru" }, a.cookie);
    expect(created.status).toBe(201);
    const body = (await created.json()) as { linkId: string; code: string };
    expect(seenAgents.at(-1)).toBe("chess-improver-ci (+https://github.com/chess-improver)");

    locations.set("hikaru", `Paris ${body.code} `);
    const verify = await verifyRoute.POST(
      new Request(`${BASE}/api/accounts/chesscom/${body.linkId}/verify`, json({}, a.cookie)),
      params(body.linkId),
    );
    expect(verify.status).toBe(200);
    expect(await verify.json()).toMatchObject({ status: "verified" });

    const list = await accountsRoute.GET(
      new Request(`${BASE}/api/accounts`, { headers: { cookie: a.cookie } }),
    );
    expect(list.status).toBe(200);
    expect(await list.json()).toMatchObject([{ username: "hikaru", verified: true, status: "ok" }]);

    const removed = await linkRoute.DELETE(
      new Request(`${BASE}/api/accounts/${body.linkId}`, json({}, a.cookie, "DELETE")),
      params(body.linkId),
    );
    expect(removed.status).toBe(204);
    expect(await db.select().from(accountLinks)).toHaveLength(0);
  });

  it("maps errors to 400, 404, 409 and 503", async () => {
    const a = await newUser();
    expect((await post({ username: "ab" }, a.cookie)).status).toBe(400);
    expect(await (await post({}, a.cookie)).json()).toEqual({ error: "invalid_username" });
    const missing = await post({ username: "nobody_here" }, a.cookie);
    expect([missing.status, await missing.json()]).toEqual([404, { error: "not_found" }]);
    for (const name of ["one_1", "two_2", "three_3", "four_4"]) locations.set(name, null);
    for (const name of ["one_1", "two_2", "three_3"]) await post({ username: name }, a.cookie);
    const limit = await post({ username: "four_4" }, a.cookie);
    expect([limit.status, await limit.json()]).toEqual([409, { error: "limit_reached" }]);
    forcedStatus = 503;
    const fresh = await newUser();
    const down = await post({ username: "hikaru" }, fresh.cookie);
    expect([down.status, await down.json()]).toEqual([503, { error: "chesscom_unavailable" }]);
  });

  it("needs a signed-in user", async () => {
    expect((await post({ username: "hikaru" }, "")).status).toBe(401);
    expect((await accountsRoute.GET(new Request(`${BASE}/api/accounts`))).status).toBe(401);
  });

  it("answers 404 for someone else's link", async () => {
    const [a, b] = [await newUser(), await newUser()];
    const { linkId } = (await (await post({ username: "hikaru" }, a.cookie)).json()) as {
      linkId: string;
    };
    const verify = await verifyRoute.POST(
      new Request(`${BASE}/api/accounts/chesscom/${linkId}/verify`, json({}, b.cookie)),
      params(linkId),
    );
    expect(verify.status).toBe(404);
    const removed = await linkRoute.DELETE(
      new Request(`${BASE}/api/accounts/${linkId}`, json({}, b.cookie, "DELETE")),
      params(linkId),
    );
    expect(removed.status).toBe(404);
    const junk = await linkRoute.DELETE(
      new Request(`${BASE}/api/accounts/not-a-uuid`, json({}, a.cookie, "DELETE")),
      params("not-a-uuid"),
    );
    expect(junk.status).toBe(404);
  });

  it("CHESSCOM_ENABLED=false makes every chess.com route answer 404", async () => {
    const a = await newUser();
    const { linkId } = (await (await post({ username: "hikaru" }, a.cookie)).json()) as {
      linkId: string;
    };
    setEnv({ CHESSCOM_ENABLED: "0" });
    expect((await post({ username: "hikaru" }, a.cookie)).status).toBe(404);
    const verify = await verifyRoute.POST(
      new Request(`${BASE}/api/accounts/chesscom/${linkId}/verify`, json({}, a.cookie)),
      params(linkId),
    );
    expect(verify.status).toBe(404);
  });
});

describe("chesscomOAuthProvider", () => {
  it("oauthProviders(env) has no chesscom entry by default", () => {
    expect(oauthProviders(getEnv()).map((p) => p.providerId)).not.toContain("chesscom");
  });

  it("is registered only with CHESSCOM_OAUTH_ENABLED and both client credentials", () => {
    setEnv({ CHESSCOM_OAUTH_ENABLED: "1" });
    expect(oauthProviders(getEnv()).map((p) => p.providerId)).not.toContain("chesscom");
    setEnv({ CHESSCOM_OAUTH_ENABLED: "1", CHESSCOM_CLIENT_ID: "id", CHESSCOM_CLIENT_SECRET: "s" });
    const provider = oauthProviders(getEnv()).find((p) => p.providerId === "chesscom");
    expect(provider?.discoveryUrl).toBe("https://oauth.chess.com/.well-known/openid-configuration");
    setEnv({ CHESSCOM_OAUTH_ENABLED: "", CHESSCOM_CLIENT_ID: "id", CHESSCOM_CLIENT_SECRET: "s" });
    expect(oauthProviders(getEnv()).map((p) => p.providerId)).not.toContain("chesscom");
  });

  it("never reports a chess.com email as verified", async () => {
    setEnv({ CHESSCOM_OAUTH_ENABLED: "1", CHESSCOM_CLIENT_ID: "id", CHESSCOM_CLIENT_SECRET: "s" });
    const provider = oauthProviders(getEnv()).find((p) => p.providerId === "chesscom");
    const mapped = await provider?.mapProfileToUser?.({
      sub: "1",
      email: "x@example.invalid",
      email_verified: true,
      emailVerified: true,
    });
    expect(mapped).toMatchObject({ emailVerified: false });
  });
});

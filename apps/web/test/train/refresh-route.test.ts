import { drillItems, gameMetrics, moves } from "@ci/db/generated";
import { sql } from "drizzle-orm";
import { beforeEach, describe, expect, it } from "vitest";
import { timeZoneFromCookieHeader } from "../../src/lib/time-zone";
import { resetGameTables, seedAccount, seedSyntheticGame } from "../helpers/games";
import { createTestSession, resetAuthTables, webTestDb } from "../helpers/session";

// POST /api/train/refresh (ADR 0090 Amendment 1): the trainer's items are made here, never on a
// page's GET, and only once per set of analysed games.

const db = webTestDb();
const route = await import("../../src/app/api/train/refresh/route");
const ORIGIN = "http://localhost:3100";
const FEN = "6k1/5ppp/8/8/8/8/5PPP/3R2K1 w - - 0 1";
let cookie: string;
let accountId: string;

beforeEach(async () => {
  await resetAuthTables(db);
  await resetGameTables(db);
  const session = await createTestSession(db);
  cookie = session.cookie;
  ({ accountId } = await seedAccount(db, { userId: session.user.id, username: "refresh" }));
  const { gameId } = await seedSyntheticGame(db, accountId, {
    playedAt: "2026-10-01T12:00:00Z",
    timeClass: "rapid",
    result: "0-1",
    noMoves: true,
  });
  await db.insert(moves).values({
    gameId,
    ply: 21,
    color: "white",
    san: "Rd7",
    uci: "d1d7",
    fenBefore: FEN,
    bestUci: "d1d8",
    winBefore: 90,
    winAfter: 40,
    deltaW: 50,
    accuracy: 20,
    judgement: "blunder",
    phase: "middlegame",
    decided: false,
  });
  await db.insert(gameMetrics).values({
    gameId,
    accountId,
    timeClass: "rapid",
    playedAt: new Date("2026-10-01T12:00:00Z"),
    protocol: "t",
    metrics: { version: 1 },
  });
});

function post(body: unknown, opts: { origin?: string; cookie?: string | null } = {}) {
  const headers = new Headers({
    "content-type": "application/json",
    origin: opts.origin ?? ORIGIN,
  });
  const c = opts.cookie === undefined ? cookie : opts.cookie;
  if (c) headers.set("cookie", c);
  return route.POST(
    new Request(`${ORIGIN}/api/train/refresh`, {
      method: "POST",
      headers,
      body: JSON.stringify(body),
    }),
  );
}

const items = async () => (await db.select({ id: drillItems.id }).from(drillItems)).length;

describe("POST /api/train/refresh", () => {
  it("makes the new positions once, then nothing for the same games", async () => {
    const first = await post({ accountId, timeClass: "rapid" });
    expect(first.status).toBe(200);
    expect(await first.json()).toEqual({ added: 1 });
    expect(await items()).toBe(1);
    expect(await (await post({ accountId, timeClass: "rapid" })).json()).toEqual({ added: 0 });
    const [mark] = await db.execute<{ n: number }>(
      sql`select count(*)::int as n from drill_refreshes`,
    );
    expect(mark?.n).toBe(1);
  });

  it("is 400 for a bad body, 404 for another user's account", async () => {
    expect((await post({ accountId, timeClass: "hyper" })).status).toBe(400);
    expect((await post({ accountId: "nope", timeClass: "rapid" })).status).toBe(400);
    const other = await createTestSession(db);
    const res = await post({ accountId, timeClass: "rapid" }, { cookie: other.cookie });
    expect(res.status).toBe(404);
    expect(await items()).toBe(0);
  });

  it("is 401 signed out and 403 cross-site", async () => {
    expect((await post({ accountId, timeClass: "rapid" }, { cookie: null })).status).toBe(401);
    expect(
      (await post({ accountId, timeClass: "rapid" }, { origin: "https://evil.example" })).status,
    ).toBe(403);
    expect(await items()).toBe(0);
  });
});

describe("the time zone cookie", () => {
  it("reads the browser's zone, else UTC", () => {
    expect(timeZoneFromCookieHeader("a=1; ci_tz=Europe%2FBerlin; b=2")).toBe("Europe/Berlin");
    expect(timeZoneFromCookieHeader("ci_tz=Mars%2FOlympus")).toBe("UTC");
    expect(timeZoneFromCookieHeader("ci_tz=%E0%A4%A")).toBe("UTC");
    expect(timeZoneFromCookieHeader(null)).toBe("UTC");
    expect(timeZoneFromCookieHeader("other=1")).toBe("UTC");
  });
});

import { focuses } from "@ci/db/generated";
import { eq } from "drizzle-orm";
import { beforeEach, describe, expect, it } from "vitest";
import { resetGameTables, seedAccount } from "../helpers/games";
import { seedProfile } from "../helpers/profile";
import { createTestSession, resetAuthTables, webTestDb } from "../helpers/session";

const db = webTestDb();
const route = await import("../../src/app/api/focus/route");

const ORIGIN = "http://localhost:3100";
let cookie: string;
let userId: string;
let accountId: string;

beforeEach(async () => {
  await resetAuthTables(db);
  await resetGameTables(db);
  const session = await createTestSession(db);
  cookie = session.cookie;
  userId = session.user.id;
  ({ accountId } = await seedAccount(db, { userId, username: "focus-route" }));
  await seedProfile(db, accountId, {
    timeClass: "rapid",
    games: 1,
    estimates: [{ seriesKey: "W5", mean: 0.2, kUnweighted: 18, nUnweighted: 90 }],
  });
});

function request(method: "POST" | "PATCH" | "DELETE", body: unknown, c: string | null = cookie) {
  const headers = new Headers({ "content-type": "application/json", origin: ORIGIN });
  if (c) headers.set("cookie", c);
  return new Request(`${ORIGIN}/api/focus`, { method, headers, body: JSON.stringify(body) });
}

describe("/api/focus", () => {
  it("POST sets the focus with a server-side target", async () => {
    const res = await route.POST(
      request("POST", { accountId, timeClass: "rapid", seriesKey: "W5", targetRate: 0.0 }),
    );
    expect(res.status).toBe(201);
    const [row] = await db.select().from(focuses).where(eq(focuses.userId, userId));
    // 20% now, peers at 3% (the seed's band): a quarter fewer, never the client's number.
    expect(row).toMatchObject({ seriesKey: "W5", targetRate: 0.15, gamesN: 20 });
  });

  it("POST is 404 for another user's account and 400 for a bad body", async () => {
    const other = await createTestSession(db);
    const res = await route.POST(
      request("POST", { accountId, timeClass: "rapid", seriesKey: "W5" }, other.cookie),
    );
    expect(res.status).toBe(404);
    expect((await route.POST(request("POST", { accountId, timeClass: "nope" }))).status).toBe(400);
    expect(
      (
        await route.POST(
          request("POST", { accountId, timeClass: "rapid", seriesKey: "W5", gamesN: 3 }),
        )
      ).status,
    ).toBe(400);
  });

  it("PATCH changes N and DELETE clears it", async () => {
    await route.POST(request("POST", { accountId, timeClass: "rapid", seriesKey: "W5" }));
    expect((await route.PATCH(request("PATCH", { timeClass: "rapid", gamesN: 30 }))).status).toBe(
      200,
    );
    expect((await route.DELETE(request("DELETE", { timeClass: "rapid" }))).status).toBe(200);
    expect((await route.DELETE(request("DELETE", { timeClass: "rapid" }))).status).toBe(404);
    const [row] = await db.select().from(focuses).where(eq(focuses.userId, userId));
    expect(row).toMatchObject({ gamesN: 30, outcome: "cleared" });
  });

  it("is 401 without a session", async () => {
    const res = await route.POST(
      request("POST", { accountId, timeClass: "rapid", seriesKey: "W5" }, null),
    );
    expect(res.status).toBe(401);
  });
});

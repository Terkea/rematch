import { drillItems, moves } from "@ci/db/generated";
import { eq } from "drizzle-orm";
import { beforeEach, describe, expect, it } from "vitest";
import { resolveCoachScope } from "../../src/server/profile";
import { refreshItems } from "../../src/server/train/store";
import { resetGameTables, seedAccount, seedSyntheticGame } from "../helpers/games";
import { createTestSession, resetAuthTables, webTestDb } from "../helpers/session";

// POST /api/train/attempts: the trainer's only write from the page.

const db = webTestDb();
const route = await import("../../src/app/api/train/attempts/route");
const ORIGIN = "http://localhost:3100";
const FEN = "6k1/5ppp/8/8/8/8/5PPP/3R2K1 w - - 0 1";
let cookie: string;
let itemId: string;

beforeEach(async () => {
  await resetAuthTables(db);
  await resetGameTables(db);
  const session = await createTestSession(db);
  cookie = session.cookie;
  const { accountId } = await seedAccount(db, { userId: session.user.id, username: "route" });
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
  const { gameMetrics } = await import("@ci/db/generated");
  await db.insert(gameMetrics).values({
    gameId,
    accountId,
    timeClass: "rapid",
    playedAt: new Date("2026-10-01T12:00:00Z"),
    protocol: "t",
    metrics: { version: 1 },
  });
  const scope = await resolveCoachScope(db, session.user.id, {});
  if (!scope) throw new Error("no scope");
  await refreshItems(db, session.user.id, scope);
  await db.update(drillItems).set({ heldOut: false });
  const [item] = await db.select({ id: drillItems.id }).from(drillItems);
  itemId = item?.id ?? "";
});

function post(body: unknown, opts: { origin?: string; cookie?: string | null } = {}) {
  const headers = new Headers({
    "content-type": "application/json",
    origin: opts.origin ?? ORIGIN,
  });
  const c = opts.cookie === undefined ? cookie : opts.cookie;
  if (c) headers.set("cookie", c);
  return route.POST(
    new Request(`${ORIGIN}/api/train/attempts`, {
      method: "POST",
      headers,
      body: JSON.stringify(body),
    }),
  );
}

describe("POST /api/train/attempts", () => {
  it("grades the move and reveals the engine's line", async () => {
    const res = await post({ itemId, uci: "d1d8", confidence: "sure", ms: 3000 });
    expect(res.status).toBe(200);
    const json = await res.json();
    expect(json).toMatchObject({ correct: true, feedback: "There you go: Rd8# is mate." });
    expect(json.game.href).toMatch(/^\/app\/games\/[0-9a-f-]{36}\?ply=21$/);
    const [row] = await db.select().from(drillItems).where(eq(drillItems.id, itemId));
    expect(row?.lastAttemptOn).not.toBeNull();
  });

  it("is 400 without a confidence chosen first", async () => {
    expect((await post({ itemId, uci: "d1d8" })).status).toBe(400);
    expect((await post({ itemId, uci: "d1d8", confidence: "maybe" })).status).toBe(400);
  });

  it("is 404 for another user's item or a malformed one", async () => {
    const other = await createTestSession(db);
    const res = await post({ itemId, uci: "d1d8", confidence: "sure" }, { cookie: other.cookie });
    expect(res.status).toBe(404);
    expect((await post({ itemId: "nope", uci: "d1d8", confidence: "sure" })).status).toBe(404);
  });

  it("is 401 signed out and 403 cross-site", async () => {
    expect((await post({ itemId, uci: "d1d8", confidence: "sure" }, { cookie: null })).status).toBe(
      401,
    );
    const cross = await post(
      { itemId, uci: "d1d8", confidence: "sure" },
      { origin: "https://evil.example" },
    );
    expect(cross.status).toBe(403);
  });
});

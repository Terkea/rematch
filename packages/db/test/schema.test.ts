import { eq, sql } from "drizzle-orm";
import { afterAll, describe, expect, it } from "vitest";
import {
  createDb,
  games,
  linkedAccounts,
  moves,
  type NewGame,
  positionEvals,
  upsertGame,
} from "../src/index.js";

const db = createDb(process.env.DATABASE_URL_TEST as string);

afterAll(async () => {
  await db.$client.end();
});

async function makeAccount() {
  const [acc] = await db
    .insert(linkedAccounts)
    .values({ platform: "lichess", username: "magnus", displayName: "Magnus" })
    .returning();
  if (!acc) throw new Error("no account");
  return acc;
}

function gameRow(accountId: string, platformGameId = "abc123"): NewGame {
  return {
    accountId,
    platform: "lichess",
    platformGameId,
    pgn: "1. e4 e5 *",
    playedAt: new Date("2026-01-01T00:00:00Z"),
    timeClass: "blitz",
    baseSeconds: 180,
    incrementSeconds: 2,
    rated: true,
    playerColor: "white",
    whiteName: "Magnus",
    blackName: "Hikaru",
    result: "*",
  };
}

async function countOf(table: typeof games | typeof moves) {
  const [r] = await db.select({ n: sql<number>`count(*)::int` }).from(table);
  return r?.n;
}

describe("schema", () => {
  it("upsertGame is idempotent on (platform, platformGameId)", async () => {
    const acc = await makeAccount();
    const first = await upsertGame(db, gameRow(acc.id));
    const second = await upsertGame(db, gameRow(acc.id));
    expect(first.inserted).toBe(true);
    expect(second.inserted).toBe(false);
    expect(second.id).toBe(first.id);
    expect(await countOf(games)).toBe(1);
  });

  it("defaults analysisStatus to pending", async () => {
    const acc = await makeAccount();
    const { id } = await upsertGame(db, gameRow(acc.id));
    const [g] = await db.select().from(games).where(eq(games.id, id));
    expect(g?.analysisStatus).toBe("pending");
  });

  it("cascades moves when the game is deleted", async () => {
    const acc = await makeAccount();
    const { id } = await upsertGame(db, gameRow(acc.id));
    await db.insert(moves).values({
      gameId: id,
      ply: 1,
      color: "white",
      san: "e4",
      uci: "e2e4",
      fenBefore: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
      bestUci: "e2e4",
      winBefore: 50,
      winAfter: 52,
      deltaW: 0,
      accuracy: 100,
      judgement: null,
      phase: "opening",
      clockBefore: 180,
      timeSpent: 1.5,
      decided: false,
    });
    expect(await countOf(moves)).toBe(1);
    await db.delete(games).where(eq(games.id, id));
    expect(await countOf(moves)).toBe(0);
  });

  it("rejects a duplicate (fenKey, protocol) in positionEvals", async () => {
    const row = {
      fenKey: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq -",
      protocol: "sf19-n300k-v1",
      scoreCp: 20,
      pv: ["e2e4"],
      source: "stockfish" as const,
    };
    await db.insert(positionEvals).values(row);
    await expect(db.insert(positionEvals).values(row)).rejects.toMatchObject({
      cause: { code: "23505" },
    });
  });
});

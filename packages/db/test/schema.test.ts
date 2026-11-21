import { eq, sql } from "drizzle-orm";
import { afterAll, describe, expect, it } from "vitest";
import {
  createDb,
  games,
  importRuns,
  insertUserEvent,
  linkedAccounts,
  listEventsAfter,
  moves,
  type NewGame,
  positionEvals,
  upsertGame,
  userEvents,
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

  it("upsertGame returns the current analysisStatus of an existing row", async () => {
    const acc = await makeAccount();
    const first = await upsertGame(db, gameRow(acc.id));
    expect(first.analysisStatus).toBe("pending");
    await db.update(games).set({ analysisStatus: "done" }).where(eq(games.id, first.id));
    expect((await upsertGame(db, gameRow(acc.id))).analysisStatus).toBe("done");
  });

  it("new recovery columns default sensibly; import runs and user events exist", async () => {
    const acc = await makeAccount();
    expect(acc.importCursor).toBeNull();
    const { id } = await upsertGame(db, gameRow(acc.id));
    const [g] = await db.select().from(games).where(eq(games.id, id));
    expect(g).toMatchObject({ analysisAttempts: 0, heartbeatAt: null, analysisStartedAt: null });
    const [run] = await db.insert(importRuns).values({ accountId: acc.id }).returning();
    expect(run).toMatchObject({
      status: "running",
      imported: 0,
      finishedAt: null,
      lastError: null,
    });
    const evs = await db
      .insert(userEvents)
      .values([
        { accountId: acc.id, type: "a", payload: {} },
        { accountId: acc.id, type: "b", payload: { n: 1 } },
      ])
      .returning();
    expect((evs[1]?.id as number) > (evs[0]?.id as number)).toBe(true);
  });

  it("createDb can set a statement_timeout on every connection", async () => {
    const limited = createDb(process.env.DATABASE_URL_TEST as string, { statementTimeoutMs: 200 });
    try {
      const err = (await limited.execute(sql`select pg_sleep(1)`).catch((e) => e)) as Error;
      // drizzle wraps the driver error; the server message is on `cause`.
      expect(String((err.cause as Error | undefined)?.message)).toMatch(/statement timeout/);
      const [row] = await limited.execute<{ statement_timeout: string }>(
        sql`show statement_timeout`,
      );
      expect(row?.statement_timeout).toBe("200ms");
    } finally {
      await limited.$client.end();
    }
  });

  it("insertUserEvent hands out ids in commit order and listEventsAfter pages by id", async () => {
    const acc = await makeAccount();
    const a = await insertUserEvent(db, { accountId: acc.id, type: "a", payload: {} });
    const b = await insertUserEvent(db, { accountId: acc.id, type: "b", payload: { n: 1 } });
    expect(b).toBeGreaterThan(a);
    const page = await listEventsAfter(db, [acc.id], a, 10);
    expect(page.map((e) => [e.id, e.type, e.payload])).toEqual([[b, "b", { n: 1 }]]);
  });

  it("games get pendingSince when inserted", async () => {
    const acc = await makeAccount();
    const { id } = await upsertGame(db, gameRow(acc.id));
    const [g] = await db.select().from(games).where(eq(games.id, id));
    expect(g?.pendingSince).toBeInstanceOf(Date);
    const [run] = await db
      .insert(importRuns)
      .values({ accountId: acc.id, status: "paused", errorCode: "circuit_open" })
      .returning();
    expect(run?.errorCode).toBe("circuit_open");
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

import { type Db, games, moves, upsertGame } from "@ci/db";
import { eq } from "drizzle-orm";
import { beforeAll, describe, expect, it } from "vitest";
import { createDbEvalCache } from "../../src/analysis/eval-cache.js";
import { handleAnalyseGame } from "../../src/jobs/analyse-game.js";
import { getWorkerTestDb, workerTestDbConfigured } from "../db.js";
import { insertAccount, makeGame, SCHOLARS } from "./helpers.js";

describe.skipIf(!workerTestDbConfigured())("handleAnalyseGame", () => {
  let db: Db;
  beforeAll(async () => {
    db = (await getWorkerTestDb()) as Db;
  });

  function pool(fail = { on: false }) {
    const calls: string[] = [];
    return {
      calls,
      fail,
      analyse: async (fen: string) => {
        calls.push(fen);
        if (fail.on) throw new Error("engine exploded");
        return [{ multipv: 1, score: { cp: 20 }, pv: ["e2e4"], depth: 1, nodes: 1 }];
      },
    };
  }

  async function addGame(pgn: string, username: string) {
    const acct = await insertAccount(db, username);
    const g = makeGame({ pgn, white: { name: username, rating: 1 } });
    const { id } = await upsertGame(db, {
      accountId: acct.id,
      platform: g.platform,
      platformGameId: g.platformGameId,
      pgn: g.pgn,
      playedAt: g.playedAt,
      timeClass: g.timeClass,
      baseSeconds: g.baseSeconds,
      incrementSeconds: g.incrementSeconds,
      rated: g.rated,
      playerColor: "white",
      whiteName: username,
      blackName: "x",
      result: g.result,
    });
    return id;
  }

  const getGame = async (id: string) => {
    const [g] = await db.select().from(games).where(eq(games.id, id));
    return g;
  };
  const moveCount = async (id: string) =>
    (await db.select().from(moves).where(eq(moves.gameId, id))).length;

  it("analyses Scholar's mate into 7 moves", async () => {
    const id = await addGame(SCHOLARS, "scholar");
    const p = pool();
    const res = await handleAnalyseGame(
      { db, pool: p, cache: createDbEvalCache(db) },
      { gameId: id },
    );
    expect(res).toEqual({ status: "done" });
    const g = await getGame(id);
    expect(g?.analysisStatus).toBe("done");
    expect(g?.analysisProtocol).toBe("sf19-n300k-v1");
    expect(g?.whiteAccuracy).not.toBeNull();
    expect(g?.blackAccuracy).not.toBeNull();
    expect(await moveCount(id)).toBe(7);
  });

  it("skips games with fewer than 2 plies without calling the engine", async () => {
    const id = await addGame("1. e4 *", "short");
    const p = pool();
    const res = await handleAnalyseGame(
      { db, pool: p, cache: createDbEvalCache(db) },
      { gameId: id },
    );
    expect(res).toEqual({ status: "skipped" });
    expect(p.calls).toHaveLength(0);
    expect((await getGame(id))?.analysisStatus).toBe("skipped");
  });

  it("skips unsupported variants", async () => {
    const id = await addGame('[Variant "Crazyhouse"]\n\n1. e4 e5 2. Nf3 *', "variant");
    const p = pool();
    const res = await handleAnalyseGame(
      { db, pool: p, cache: createDbEvalCache(db) },
      { gameId: id },
    );
    expect(res).toEqual({ status: "skipped" });
    expect(p.calls).toHaveLength(0);
  });

  it("marks failed and rethrows on engine error; a re-run succeeds without duplicate moves", async () => {
    const id = await addGame("1. d4 d5 2. c4 e6 *", "flaky");
    const p = pool({ on: true });
    const deps = { db, pool: p, cache: createDbEvalCache(db) };
    await expect(handleAnalyseGame(deps, { gameId: id })).rejects.toThrow("engine exploded");
    const failed = await getGame(id);
    expect(failed?.analysisStatus).toBe("failed");
    expect(failed?.analysisError).toContain("engine exploded");

    p.fail.on = false;
    expect(await handleAnalyseGame(deps, { gameId: id })).toEqual({ status: "done" });
    expect((await getGame(id))?.analysisError).toBeNull();
    expect(await moveCount(id)).toBe(4);
    // Re-analysing a finished game replaces moves rather than duplicating them.
    await handleAnalyseGame(deps, { gameId: id });
    expect(await moveCount(id)).toBe(4);
  });
});

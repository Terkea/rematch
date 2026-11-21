import { type Db, games, moves, upsertGame } from "@ci/db";
import { UnrecoverableError } from "bullmq";
import { eq } from "drizzle-orm";
import { beforeAll, describe, expect, it } from "vitest";
import { createDbEvalCache } from "../../src/analysis/eval-cache.js";
import { EnginePoolClosedError } from "../../src/engine/engine-pool.js";
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

  it("a missing game throws UnrecoverableError, so BullMQ does not retry it", async () => {
    const deps = { db, pool: pool(), cache: createDbEvalCache(db) };
    await expect(
      handleAnalyseGame(deps, { gameId: "00000000-0000-4000-8000-000000000000" }),
    ).rejects.toBeInstanceOf(UnrecoverableError);
  });

  it("records the start, counts attempts and heartbeats from inside the evaluation", async () => {
    const id = await addGame("1. e4 e5 2. Nf3 Nc6 3. Bb5 a6 *", "beating");
    const heartbeats: (Date | null)[] = [];
    const slow = {
      analyse: async () => {
        heartbeats.push((await getGame(id))?.heartbeatAt ?? null);
        await new Promise((r) => setTimeout(r, 5));
        return [{ multipv: 1, score: { cp: 20 }, pv: ["e2e4"], depth: 1, nodes: 1 }];
      },
    };
    const before = Date.now();
    await handleAnalyseGame(
      { db, pool: slow, cache: createDbEvalCache(db), heartbeatEveryMs: 0, concurrency: 1 },
      { gameId: id },
    );
    const g = await getGame(id);
    expect(g?.analysisAttempts).toBe(1);
    expect(g?.analysisStartedAt?.getTime()).toBeGreaterThanOrEqual(before - 1000);
    const times = heartbeats.map((h) => h?.getTime() ?? 0);
    expect(times[0]).toBeGreaterThan(0); // set when the attempt starts
    expect(new Set(times).size).toBeGreaterThan(3); // and refreshed while evaluating
    expect((g?.heartbeatAt?.getTime() ?? 0) >= (times.at(-1) ?? 0)).toBe(true);
  });

  it("goes back to pending (not failed) when BullMQ will retry", async () => {
    const id = await addGame("1. e4 c6 2. d4 d5 *", "retrying");
    const p = pool({ on: true });
    const deps = { db, pool: p, cache: createDbEvalCache(db) };
    await expect(
      handleAnalyseGame(deps, { gameId: id }, { attemptsMade: 0, attempts: 3 }),
    ).rejects.toThrow("engine exploded");
    const g = await getGame(id);
    expect(g?.analysisStatus).toBe("pending");
    expect(g?.analysisError).toContain("engine exploded");
    expect(g?.analysisAttempts).toBe(1);
  });

  it("a closed engine pool (shutdown) leaves the game pending even on the last attempt", async () => {
    const id = await addGame("1. e4 e6 2. d4 d5 *", "shutdown");
    const closed = {
      analyse: async (): Promise<never> => {
        throw new EnginePoolClosedError();
      },
    };
    const published: string[] = [];
    await expect(
      handleAnalyseGame(
        {
          db,
          pool: closed,
          cache: createDbEvalCache(db),
          publish: async (_a, type) => {
            published.push(type);
            return 1;
          },
        },
        { gameId: id },
        { attemptsMade: 2, attempts: 3 },
      ),
    ).rejects.toBeInstanceOf(EnginePoolClosedError);
    expect((await getGame(id))?.analysisStatus).toBe("pending");
    expect(published).toEqual([]);
  });

  it("publishes game.analysed with the player's accuracy after the commit", async () => {
    const id = await addGame("1. d4 Nf6 2. c4 e6 *", "announced");
    const events: { accountId: string; type: string; payload: object; status?: string }[] = [];
    await handleAnalyseGame(
      {
        db,
        pool: pool(),
        cache: createDbEvalCache(db),
        publish: async (accountId, type, payload) => {
          events.push({ accountId, type, payload, status: (await getGame(id))?.analysisStatus });
          return 1;
        },
      },
      { gameId: id },
    );
    const g = await getGame(id);
    expect(events).toEqual([
      {
        accountId: g?.accountId,
        type: "game.analysed",
        payload: { gameId: id, accuracy: expect.closeTo(g?.whiteAccuracy as number, 4) },
        status: "done",
      },
    ]);
  });

  function gatedPool(cp: number) {
    let release!: () => void;
    const gate = new Promise<void>((r) => (release = r));
    let started!: () => void;
    const hasStarted = new Promise<void>((r) => (started = r));
    return {
      release: () => release(),
      hasStarted,
      analyse: async () => {
        started();
        await gate;
        return [{ multipv: 1, score: { cp }, pv: ["e2e4"], depth: 1, nodes: 1 }];
      },
    };
  }
  const cacheless = { getMany: async () => new Map(), putMany: async () => {} };

  it("fences the final write: a stale duplicate run cannot overwrite a newer done", async () => {
    const id = await addGame("1. e4 g6 2. d4 Bg7 *", "fenced");
    const stale = gatedPool(-400);
    const staleRun = handleAnalyseGame({ db, pool: stale, cache: cacheless }, { gameId: id });
    await stale.hasStarted;
    // A second delivery of the same game (for example after a stalled-job pickup) runs to the end.
    expect(await handleAnalyseGame({ db, pool: pool(), cache: cacheless }, { gameId: id })).toEqual(
      {
        status: "done",
      },
    );
    const afterFresh = await getGame(id);
    expect(afterFresh?.analysisAttempts).toBe(2);

    stale.release();
    expect(await staleRun).toEqual({ status: "superseded" });
    const g = await getGame(id);
    expect(g?.analysisStatus).toBe("done");
    expect(g?.whiteAccuracy).toBe(afterFresh?.whiteAccuracy);
    expect(g?.blackAccuracy).toBe(afterFresh?.blackAccuracy);
    expect(await moveCount(id)).toBe(4);
  });

  it("fences the failure write too", async () => {
    const id = await addGame("1. e4 b6 2. d4 Bb7 *", "fenced-fail");
    const stale = gatedPool(0);
    const failing = {
      analyse: async () => {
        await stale.analyse();
        throw new Error("late failure");
      },
    };
    const staleRun = handleAnalyseGame({ db, pool: failing, cache: cacheless }, { gameId: id });
    await stale.hasStarted;
    await handleAnalyseGame({ db, pool: pool(), cache: cacheless }, { gameId: id });
    stale.release();
    await expect(staleRun).rejects.toThrow("late failure");
    const g = await getGame(id);
    expect(g?.analysisStatus).toBe("done");
    expect(g?.analysisError).toBeNull();
  });

  it("skips a game already done at the current protocol without touching it", async () => {
    const id = await addGame("1. Nf3 d5 2. g3 c5 *", "already");
    const deps = { db, pool: pool(), cache: createDbEvalCache(db) };
    await handleAnalyseGame(deps, { gameId: id });
    const before = await getGame(id);
    const p = pool();
    expect(await handleAnalyseGame({ ...deps, pool: p }, { gameId: id })).toEqual({
      status: "done",
    });
    expect(p.calls).toHaveLength(0);
    const after = await getGame(id);
    expect(after?.analysisAttempts).toBe(before?.analysisAttempts);
    expect(after?.analysisStartedAt).toEqual(before?.analysisStartedAt);
  });
});

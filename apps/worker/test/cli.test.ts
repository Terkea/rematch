import { type Db, games, importRuns, linkedAccounts, moves, upsertGame } from "@ci/db";
import { eq } from "drizzle-orm";
import { beforeAll, describe, expect, it } from "vitest";
import { type AnalyseDeps, runAnalyse } from "../src/cli.js";
import { getWorkerTestDb, workerTestDbConfigured } from "./db.js";
import { makeGame } from "./jobs/helpers.js";

describe.skipIf(!workerTestDbConfigured())("cli runAnalyse", () => {
  let db: Db;
  beforeAll(async () => {
    db = (await getWorkerTestDb()) as Db;
  });

  type Fake = { outcome: "done" | "skipped" | "failed"; color: "white" | "black"; acc: number };

  /** A fake worker pool: "imports" the given games and settles them, then the queues are idle. */
  function harness(list: Fake[] | "notfound") {
    const out: string[] = [];
    let idle = false;
    const deps: AnalyseDeps = {
      db,
      print: (l) => out.push(l),
      printError: (l) => out.push(l),
      pollMs: 5,
      queuesIdle: async () => idle,
      enqueueImport: async ({ accountId }) => {
        if (list === "notfound") {
          await db
            .insert(importRuns)
            .values({ accountId, status: "failed", lastError: "Not found: https://x/y" });
          idle = true;
          return;
        }
        let i = 0;
        for (const f of list) {
          const player = f.color === "white" ? "Hero" : "Villain";
          const other = f.color === "white" ? "Villain" : "Hero";
          const g = makeGame({
            platformGameId: `g${i++}${Math.random()}`,
            white: { name: f.color === "white" ? player : other, rating: 1 },
            black: { name: f.color === "white" ? other : player, rating: 1 },
          });
          const res = await upsertGame(db, {
            accountId,
            platform: "lichess",
            platformGameId: g.platformGameId,
            pgn: g.pgn,
            playedAt: g.playedAt,
            timeClass: g.timeClass,
            baseSeconds: 300,
            incrementSeconds: 0,
            rated: true,
            playerColor: f.color,
            whiteName: g.white.name,
            blackName: g.black.name,
            whiteRating: 1,
            blackRating: 1,
            result: "1-0",
            termination: null,
            eco: null,
            openingName: null,
            lichessEvals: null,
            analysisStatus: "pending",
          });
          await db
            .update(games)
            .set({
              analysisStatus: f.outcome,
              whiteAccuracy: f.color === "white" ? f.acc : 50,
              blackAccuracy: f.color === "black" ? f.acc : 50,
            })
            .where(eq(games.id, res.id));
          if (f.outcome === "done") {
            await db.insert(moves).values({
              gameId: res.id,
              ply: 1,
              color: f.color,
              san: "e4",
              uci: "e2e4",
              fenBefore: "x",
              winBefore: 0.5,
              winAfter: 0.1,
              deltaW: -0.4,
              accuracy: 10,
              judgement: "blunder",
              phase: "opening",
              decided: false,
            });
          }
        }
        await db.insert(importRuns).values({ accountId, status: "done" });
        idle = true;
      },
    };
    return { deps, out };
  }

  const args = (name: string) => ["analyse", "--platform", "lichess", "--username", name];

  it("prints the exact summary line from the player's accuracy and blunders", async () => {
    const { deps, out } = harness([
      { outcome: "done", color: "white", acc: 80 },
      { outcome: "done", color: "black", acc: 91 },
      { outcome: "skipped", color: "white", acc: 0 },
      { outcome: "failed", color: "white", acc: 0 },
    ]);
    const code = await runAnalyse(args("Hero"), deps);
    expect(code).toBe(0);
    expect(out.at(-1)).toBe(
      "games: 4  analysed: 2  skipped: 1  failed: 1  avg accuracy: 85.5  blunders/game: 1.00",
    );
    const [acct] = await db
      .select()
      .from(linkedAccounts)
      .where(eq(linkedAccounts.username, "hero"));
    expect(acct?.displayName).toBe("Hero");
  });

  it("exits 1 when every game failed", async () => {
    const { deps, out } = harness([{ outcome: "failed", color: "white", acc: 0 }]);
    expect(await runAnalyse(args("Allfail"), deps)).toBe(1);
    expect(out.at(-1)).toContain("failed: 1");
  });

  it("exits 1 with the not-found message", async () => {
    const { deps, out } = harness("notfound");
    expect(await runAnalyse(args("Ghost"), deps)).toBe(1);
    expect(out).toContain('User "Ghost" not found on lichess');
  });

  it("rejects a bad platform with exit code 1", async () => {
    const { deps } = harness([]);
    expect(await runAnalyse(["analyse", "--platform", "x", "--username", "a"], deps)).toBe(1);
  });
});

import { type Db, games, importRuns, linkedAccounts, moves, upsertGame } from "@ci/db";
import { eq } from "drizzle-orm";
import { beforeAll, describe, expect, it } from "vitest";
import { type AnalyseDeps, importRunErrorCode, runAnalyse, runCli } from "../src/cli.js";
import { getWorkerTestDb, workerTestDbConfigured } from "./db.js";
import { insertAccount, makeGame } from "./jobs/helpers.js";

describe.skipIf(!workerTestDbConfigured())("cli runAnalyse", () => {
  let db: Db;
  beforeAll(async () => {
    db = (await getWorkerTestDb()) as Db;
  });

  type Fake = { outcome: "done" | "skipped" | "failed"; color: "white" | "black"; acc: number };

  /** A fake worker pool: "imports" the given games and settles them, then the queues are idle. */
  function harness(list: Fake[] | "notfound" | "importfail") {
    const out: string[] = [];
    let idle = false;
    const deps: AnalyseDeps = {
      db,
      print: (l) => out.push(l),
      printError: (l) => out.push(l),
      pollMs: 5,
      queuesIdle: async () => idle,
      listRuns: async (accountId) => {
        if (list === "notfound" && idle) {
          return [
            { id: "r1", status: "failed", lastError: "Not found: x", errorCode: "not_found" },
          ];
        }
        if (list === "importfail" && idle) {
          return [{ id: "r1", status: "failed", lastError: "boom", errorCode: "unknown" }];
        }
        const rows = await db.select().from(importRuns).where(eq(importRuns.accountId, accountId));
        return rows;
      },
      enqueueImport: async ({ accountId }) => {
        if (list === "notfound" || list === "importfail") {
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

  it("exits 1 with the import error when the import run failed (not not_found)", async () => {
    const { deps, out } = harness("importfail");
    expect(await runAnalyse(args("Broken"), deps)).toBe(1);
    expect(out).toContain("import failed: boom");
  });

  it("prints a message and exits 0 when no games were found", async () => {
    const { deps, out } = harness([]);
    expect(await runAnalyse(args("Nogames"), deps)).toBe(0);
    expect(out.at(-1)).toBe("no standard games found for Nogames on lichess");
  });

  it("importRunErrorCode reads the structured code, not the message", () => {
    expect(importRunErrorCode({ errorCode: "not_found" })).toBe("not_found");
    expect(importRunErrorCode({})).toBeNull();
    expect(importRunErrorCode({ errorCode: null })).toBeNull();
  });

  it("a failed run with a Not found message but no code is not treated as not-found", async () => {
    const { deps, out } = harness("importfail");
    const listRuns = async () => [
      { id: "r1", status: "failed", lastError: "Not found: x", errorCode: "unknown" },
    ];
    const code = await runAnalyse(args("Msgonly"), { ...deps, listRuns });
    expect(code).toBe(1);
    expect(out.some((l) => l.includes("not found on"))).toBe(false);
  });

  it("ignores stale non-terminal games from an earlier run and warns", async () => {
    const acct = await insertAccount(db, "Stale");
    const g = makeGame({ white: { name: "Stale", rating: 1 } });
    await upsertGame(db, {
      accountId: acct.id,
      platform: "lichess",
      platformGameId: "stale-1",
      pgn: g.pgn,
      playedAt: g.playedAt,
      timeClass: "blitz",
      baseSeconds: 300,
      incrementSeconds: 0,
      rated: true,
      playerColor: "white",
      whiteName: "Stale",
      blackName: "V",
      whiteRating: 1,
      blackRating: 1,
      result: "1-0",
      termination: null,
      eco: null,
      openingName: null,
      lichessEvals: null,
      analysisStatus: "analysing",
    });
    const { deps, out } = harness([{ outcome: "done", color: "white", acc: 70 }]);
    const code = await runAnalyse(args("Stale"), { ...deps, graceMs: 60_000 });
    expect(code).toBe(0);
    expect(out.some((l) => l.includes("1 game(s) left over from an earlier run"))).toBe(true);
    expect(out.at(-1)).toContain("analysed: 1");
  });

  it("gives up after the idle grace when this run's games never finish", async () => {
    const { deps, out } = harness([{ outcome: "done", color: "white", acc: 70 }]);
    const enqueueImport = async (p: { accountId: string }) => {
      await deps.enqueueImport(p);
      await db
        .update(games)
        .set({ analysisStatus: "pending" })
        .where(eq(games.accountId, p.accountId));
    };
    const code = await runAnalyse(args("Stuck"), { ...deps, enqueueImport, graceMs: 30 });
    expect(code).toBe(0);
    expect(out.some((l) => l.includes("1 game(s) are not finished"))).toBe(true);
  });

  describe("signals", () => {
    function signalHarness() {
      const { deps, out } = harness([]);
      const handlers = new Map<string, () => void>();
      const calls = { close: 0, exit: [] as number[] };
      const cliDeps = {
        signals: {
          on: (e: string, fn: () => void) => handlers.set(e, fn),
          off: (e: string) => handlers.delete(e),
        },
        exit: (c: number) => calls.exit.push(c),
        printError: (l: string) => out.push(l),
        start: async () => ({
          analyse: {
            ...deps,
            queuesIdle: async () => false,
            pollMs: 5,
            enqueueImport: async () => {
              setTimeout(() => handlers.get("SIGINT")?.(), 20);
            },
          },
          close: async () => {
            calls.close++;
            return 0;
          },
        }),
      };
      return { cliDeps, handlers, calls, out };
    }

    it("SIGINT aborts the wait, runs shutdown and returns 130", async () => {
      const { cliDeps, calls, handlers } = signalHarness();
      const code = await runCli(args("Sigint"), cliDeps);
      expect(code).toBe(130);
      expect(calls.close).toBe(1);
      expect(handlers.size).toBe(0);
    });

    it("SIGTERM returns 143", async () => {
      const { cliDeps, calls, handlers } = signalHarness();
      const start = cliDeps.start;
      cliDeps.start = async () => {
        const r = await start();
        return {
          ...r,
          analyse: {
            ...r.analyse,
            enqueueImport: async () => {
              setTimeout(() => handlers.get("SIGTERM")?.(), 20);
            },
          },
        };
      };
      expect(await runCli(args("Sigterm"), cliDeps)).toBe(143);
      expect(calls.close).toBe(1);
    });

    it("a second signal forces an immediate exit", async () => {
      const { cliDeps, calls, handlers } = signalHarness();
      const start = cliDeps.start;
      cliDeps.start = async () => {
        const r = await start();
        return {
          ...r,
          close: async () => {
            handlers.get("SIGINT")?.();
            calls.close++;
            return 0;
          },
        };
      };
      expect(await runCli(args("Twice"), cliDeps)).toBe(130);
      expect(calls.exit).toEqual([130]);
    });
  });
});

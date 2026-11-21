import { pathToFileURL } from "node:url";
import { parseArgs } from "node:util";
import { type Db, games, importRuns, linkedAccounts, moves } from "@ci/db";
import { and, eq, sql } from "drizzle-orm";
import { shutdown } from "./lifecycle/shutdown.js";
import { errorMessage } from "./log.js";
import type { ImportAccountPayload } from "./queue/queues.js";
import { startWorkers } from "./start-workers.js";

export interface AnalyseDeps {
  db: Db;
  enqueueImport(payload: ImportAccountPayload): Promise<unknown>;
  /** True when no import or analysis job is waiting, active or delayed. */
  queuesIdle(): Promise<boolean>;
  print(line: string): void;
  printError(line: string): void;
  /** DB polling interval (default 1 s). */
  pollMs?: number;
}

const PLATFORMS = ["lichess", "chesscom"] as const;
type Platform = (typeof PLATFORMS)[number];
const DEFAULT_TIMEOUT_MIN = 30;
const sleep = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));

const USAGE =
  "usage: cli analyse --platform <lichess|chesscom> --username <name> [--max N] [--timeout MINUTES]";

interface Parsed {
  platform: Platform;
  username: string;
  max?: number;
  timeoutMs: number;
}

function parse(argv: string[]): Parsed | string {
  let parsed: ReturnType<typeof parseArgs>;
  try {
    parsed = parseArgs({
      args: argv,
      allowPositionals: true,
      options: {
        platform: { type: "string" },
        username: { type: "string" },
        max: { type: "string" },
        timeout: { type: "string" },
      },
    });
  } catch (err) {
    return `${errorMessage(err)}\n${USAGE}`;
  }
  const { values, positionals } = parsed;
  if (positionals[0] !== "analyse") return USAGE;
  const platform = values.platform;
  if (!PLATFORMS.includes(platform as Platform)) {
    return `--platform must be lichess or chesscom\n${USAGE}`;
  }
  const username = typeof values.username === "string" ? values.username.trim() : "";
  if (!username) return `--username is required\n${USAGE}`;
  const num = (v: unknown, name: string): number | string | undefined => {
    if (v === undefined) return undefined;
    const n = Number(v);
    return Number.isFinite(n) && n > 0 ? n : `--${name} must be a positive number`;
  };
  const max = num(values.max, "max");
  const timeout = num(values.timeout, "timeout");
  if (typeof max === "string") return max;
  if (typeof timeout === "string") return timeout;
  return {
    platform: platform as Platform,
    username,
    ...(max !== undefined ? { max: Math.floor(max) } : {}),
    timeoutMs: (timeout ?? DEFAULT_TIMEOUT_MIN) * 60_000,
  };
}

async function findOrCreateAccount(db: Db, platform: Platform, username: string) {
  await db
    .insert(linkedAccounts)
    .values({ platform, username: username.toLowerCase(), displayName: username })
    .onConflictDoNothing();
  const [row] = await db
    .select()
    .from(linkedAccounts)
    .where(
      and(
        eq(linkedAccounts.platform, platform),
        eq(linkedAccounts.username, username.toLowerCase()),
      ),
    );
  if (!row) throw new Error("linked account missing after insert");
  return row;
}

async function statusCounts(db: Db, accountId: string) {
  const rows = await db
    .select({ status: games.analysisStatus, n: sql<number>`count(*)::int` })
    .from(games)
    .where(eq(games.accountId, accountId))
    .groupBy(games.analysisStatus);
  const c = { pending: 0, analysing: 0, done: 0, failed: 0, skipped: 0 };
  for (const r of rows) c[r.status] = r.n;
  return c;
}

async function summarise(db: Db, accountId: string) {
  const c = await statusCounts(db, accountId);
  const [acc] = await db
    .select({
      avg: sql<
        number | null
      >`avg(case when ${games.playerColor} = 'white' then ${games.whiteAccuracy} else ${games.blackAccuracy} end)`,
    })
    .from(games)
    .where(and(eq(games.accountId, accountId), eq(games.analysisStatus, "done")));
  const [bl] = await db
    .select({ n: sql<number>`count(*)::int` })
    .from(moves)
    .innerJoin(games, eq(moves.gameId, games.id))
    .where(
      and(
        eq(games.accountId, accountId),
        eq(games.analysisStatus, "done"),
        eq(moves.judgement, "blunder"),
        eq(moves.color, games.playerColor),
      ),
    );
  const total = c.pending + c.analysing + c.done + c.failed + c.skipped;
  const avg = Number(acc?.avg ?? 0);
  const perGame = c.done > 0 ? (bl?.n ?? 0) / c.done : 0;
  return {
    total,
    counts: c,
    line: `games: ${total}  analysed: ${c.done}  skipped: ${c.skipped}  failed: ${c.failed}  avg accuracy: ${avg.toFixed(1)}  blunders/game: ${perGame.toFixed(2)}`,
  };
}

/** Runs one `analyse` command against already-running workers; resolves with the exit code. */
export async function runAnalyse(argv: string[], deps: AnalyseDeps): Promise<number> {
  const args = parse(argv);
  if (typeof args === "string") {
    deps.printError(args);
    return 1;
  }
  const { db } = deps;
  const pollMs = deps.pollMs ?? 1000;
  const account = await findOrCreateAccount(db, args.platform, args.username);
  const knownRuns = new Set(
    (
      await db
        .select({ id: importRuns.id })
        .from(importRuns)
        .where(eq(importRuns.accountId, account.id))
    ).map((r) => r.id),
  );
  await deps.enqueueImport({
    accountId: account.id,
    ...(args.max !== undefined ? { max: args.max } : {}),
  });

  const deadline = Date.now() + args.timeoutMs;
  let lastProgress = 0;
  for (;;) {
    const runs = (
      await db.select().from(importRuns).where(eq(importRuns.accountId, account.id))
    ).filter((r) => !knownRuns.has(r.id));
    if (runs.some((r) => r.status === "failed" && r.lastError?.startsWith("Not found:"))) {
      deps.printError(`User "${args.username}" not found on ${args.platform}`);
      return 1;
    }
    const counts = await statusCounts(db, account.id);
    const total = counts.pending + counts.analysing + counts.done + counts.failed + counts.skipped;
    const settled = counts.done + counts.failed + counts.skipped;
    while (counts.done >= lastProgress + 10) {
      lastProgress += 10;
      deps.print(`analysed ${lastProgress}/${total} games`);
    }
    if (await deps.queuesIdle()) {
      const imported = runs.some((r) => r.status === "done");
      if (!imported) {
        const failed = runs.find((r) => r.status === "failed");
        deps.printError(`import failed: ${failed?.lastError ?? "no import run completed"}`);
        return 1;
      }
      if (settled === total) break;
    }
    if (Date.now() >= deadline) {
      deps.printError(
        `timed out after ${Math.round(args.timeoutMs / 60_000)} min (${settled}/${total} games settled)`,
      );
      return 1;
    }
    await sleep(pollMs);
  }
  const s = await summarise(db, account.id);
  deps.print(s.line);
  return s.total > 0 && s.counts.failed === s.total ? 1 : 0;
}

function requireEnv(name: string): string {
  const v = process.env[name];
  if (!v) throw new Error(`${name} is not set`);
  return v;
}

async function main(argv: string[]): Promise<number> {
  const running = await startWorkers({
    REDIS_URL: requireEnv("REDIS_URL"),
    DATABASE_URL: requireEnv("DATABASE_URL"),
    STOCKFISH_PATH: requireEnv("STOCKFISH_PATH"),
    CONTACT_EMAIL: requireEnv("CONTACT_EMAIL"),
    LICHESS_TOKEN: process.env.LICHESS_TOKEN,
  });
  let code = 1;
  try {
    code = await runAnalyse(argv, {
      db: running.db,
      enqueueImport: (p) => running.queues.enqueueImport(p),
      queuesIdle: async () => {
        const states = ["waiting", "active", "delayed", "prioritized"] as const;
        for (const q of [running.queues.importAccount, running.queues.analyseGame]) {
          const counts = await q.getJobCounts(...states);
          if (states.some((s) => (counts[s] ?? 0) > 0)) return false;
        }
        return true;
      },
      print: (l) => console.log(l),
      printError: (l) => console.error(l),
    });
  } finally {
    const closeCode = await shutdown(running, { graceMs: 25_000 });
    if (code === 0) code = closeCode;
  }
  return code;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main(process.argv.slice(2)).then(
    (code) => process.exit(code),
    (err) => {
      console.error(errorMessage(err));
      process.exit(1);
    },
  );
}

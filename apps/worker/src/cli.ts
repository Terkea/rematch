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
  /** Aborts the wait (set by runCli on SIGINT/SIGTERM). */
  signal?: AbortSignal;
  /** Idle-queue grace before giving up on unfinished games (default 60 s). */
  graceMs?: number;
  /** Test seam: the account's import runs (default: select from import_runs). */
  listRuns?(accountId: string): Promise<RunRow[]>;
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

/** The machine-readable failure class of an import run (null until the column is populated). */
export function importRunErrorCode(run: { errorCode?: string | null }): string | null {
  return run.errorCode ?? null;
}

interface RunRow {
  id: string;
  status: string;
  lastError: string | null;
  errorCode?: string | null;
}

const TERMINAL: readonly string[] = ["done", "failed", "skipped"];

async function nonTerminalIds(db: Db, accountId: string): Promise<string[]> {
  const rows = await db
    .select({ id: games.id, status: games.analysisStatus })
    .from(games)
    .where(eq(games.accountId, accountId));
  return rows.filter((r) => !TERMINAL.includes(r.status)).map((r) => r.id);
}

function sleepAbortable(ms: number, signal?: AbortSignal): Promise<void> {
  return new Promise((resolve) => {
    if (signal?.aborted) return resolve();
    const done = () => {
      clearTimeout(t);
      signal?.removeEventListener("abort", done);
      resolve();
    };
    const t = setTimeout(done, ms);
    signal?.addEventListener("abort", done);
  });
}

/** Runs one `analyse` command against already-running workers; resolves with the exit code. */
export async function runAnalyse(argv: string[], deps: AnalyseDeps): Promise<number> {
  const args = parse(argv);
  if (typeof args === "string") {
    deps.printError(args);
    return 1;
  }
  const { db, signal } = deps;
  const pollMs = deps.pollMs ?? 1000;
  const graceMs = deps.graceMs ?? 60_000;
  const account = await findOrCreateAccount(db, args.platform, args.username);
  const listRuns = async (): Promise<RunRow[]> =>
    deps.listRuns
      ? deps.listRuns(account.id)
      : db.select().from(importRuns).where(eq(importRuns.accountId, account.id));
  const knownRuns = new Set((await listRuns()).map((r) => r.id));
  // Games left pending/analysing by an earlier killed run. Only those the import re-enqueues
  // will move, so they must not stall this invocation.
  const stale = new Set(await nonTerminalIds(db, account.id));
  await deps.enqueueImport({
    accountId: account.id,
    ...(args.max !== undefined ? { max: args.max } : {}),
  });

  const deadline = Date.now() + args.timeoutMs;
  let lastProgress = Math.floor((await statusCounts(db, account.id)).done / 10) * 10;
  let idleSince: number | null = null;
  for (;;) {
    if (signal?.aborted) {
      deps.printError("interrupted");
      return 1;
    }
    const runs = (await listRuns()).filter((r) => !knownRuns.has(r.id));
    if (runs.some((r) => r.status === "failed" && importRunErrorCode(r) === "not_found")) {
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
      if (!runs.some((r) => r.status === "done")) {
        const failed = runs.find((r) => r.status === "failed");
        deps.printError(`import failed: ${failed?.lastError ?? "no import run completed"}`);
        return 1;
      }
      const open = await nonTerminalIds(db, account.id);
      const inScope = open.filter((id) => !stale.has(id));
      idleSince ??= Date.now();
      if (inScope.length === 0) {
        if (open.length > 0) {
          deps.printError(
            `warning: ${open.length} game(s) left over from an earlier run are still pending/analysing and were ignored`,
          );
        }
        break;
      }
      if (Date.now() - idleSince >= graceMs) {
        deps.printError(
          `warning: queues are idle but ${inScope.length} game(s) are not finished; giving up (${settled}/${total} settled)`,
        );
        break;
      }
    } else {
      idleSince = null;
    }
    if (Date.now() >= deadline) {
      deps.printError(
        `timed out after ${Math.round(args.timeoutMs / 60_000)} min (${settled}/${total} games settled)`,
      );
      return 1;
    }
    await sleepAbortable(pollMs, signal);
  }
  const s = await summarise(db, account.id);
  if (s.total === 0) {
    deps.print(`no standard games found for ${args.username} on ${args.platform}`);
    return 0;
  }
  deps.print(s.line);
  return s.counts.failed === s.total ? 1 : 0;
}

type SignalName = "SIGINT" | "SIGTERM";

export interface SignalSource {
  on(event: SignalName, fn: () => void): unknown;
  off(event: SignalName, fn: () => void): unknown;
}

export interface CliDeps {
  /** Starts the workers and returns what `runAnalyse` needs plus a closer (the shutdown path). */
  start(): Promise<{ analyse: Omit<AnalyseDeps, "signal">; close(): Promise<number> }>;
  signals: SignalSource;
  /** Used for a second signal: exits immediately. */
  exit(code: number): void;
  printError(line: string): void;
}

const SIGNAL_CODES: Record<SignalName, number> = { SIGINT: 130, SIGTERM: 143 };

/** Runs the CLI: SIGINT/SIGTERM abort the wait, the workers are shut down, and 128+n is returned. */
export async function runCli(argv: string[], deps: CliDeps): Promise<number> {
  const controller = new AbortController();
  let signalCode: number | null = null;
  const handlers = (["SIGINT", "SIGTERM"] as const).map((sig) => {
    const fn = () => {
      if (signalCode !== null) {
        deps.printError("second signal received; exiting immediately");
        deps.exit(SIGNAL_CODES[sig]);
        return;
      }
      signalCode = SIGNAL_CODES[sig];
      deps.printError(`${sig} received; shutting down`);
      controller.abort();
    };
    deps.signals.on(sig, fn);
    return [sig, fn] as const;
  });
  try {
    const running = await deps.start();
    let code = 1;
    try {
      code = await runAnalyse(argv, { ...running.analyse, signal: controller.signal });
    } finally {
      const closeCode = await running.close();
      if (code === 0) code = closeCode;
    }
    return signalCode ?? code;
  } finally {
    // Handlers stay installed through shutdown so a second signal still forces an exit.
    for (const [sig, fn] of handlers) deps.signals.off(sig, fn);
  }
}

function requireEnv(name: string): string {
  const v = process.env[name];
  if (!v) throw new Error(`${name} is not set`);
  return v;
}

async function main(argv: string[]): Promise<number> {
  return runCli(argv, {
    signals: process,
    exit: (code) => process.exit(code),
    printError: (l) => console.error(l),
    start: async () => {
      const running = await startWorkers({
        REDIS_URL: requireEnv("REDIS_URL"),
        DATABASE_URL: requireEnv("DATABASE_URL"),
        STOCKFISH_PATH: requireEnv("STOCKFISH_PATH"),
        LICHESS_TOKEN: process.env.LICHESS_TOKEN,
      });
      return {
        analyse: {
          db: running.db,
          enqueueImport: (p) => running.queues.enqueueImport(p),
          queuesIdle: async () => {
            const states = ["waiting", "active", "delayed", "prioritized"] as const;
            for (const q of [running.queues.importAccount, running.queues.analyseGame]) {
              const counts = await q.getJobCounts(...states);
              if (states.some((st) => (counts[st] ?? 0) > 0)) return false;
            }
            return true;
          },
          print: (l) => console.log(l),
          printError: (l) => console.error(l),
        },
        close: () => shutdown(running, { graceMs: 25_000 }),
      };
    },
  });
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

import {
  type Db,
  games,
  type ImportCursor,
  type ImportErrorCode,
  importRuns,
  linkedAccounts,
  upsertGame,
} from "@ci/db";
import { UnrecoverableError } from "bullmq";
import { count, eq, max } from "drizzle-orm";
import type { PublishFn } from "../events/publish.js";
import {
  type CircuitBreaker,
  CircuitOpenError,
  withCircuitBreaker,
} from "../http/circuit-breaker.js";
import {
  type PlatformClient,
  PlatformNotFoundError,
  PlatformRateLimitError,
  PlatformTimeoutError,
} from "../http/platform-client.js";
import type { fetchChesscomGames } from "../importers/chesscom.js";
import type { fetchLichessGames } from "../importers/lichess.js";
import type { NormalizedGame } from "../importers/types.js";
import { log as defaultLog, errorMessage, type Logger } from "../log.js";
import type { ImportAccountPayload } from "../queue/queues.js";
import { isFinalAttempt, type JobAttempt } from "./analyse-game.js";
import {
  abortable,
  createDeadline,
  IMPORT_IDLE_DEADLINE_MS,
  JobDeadlineError,
} from "./deadline.js";

export type { ImportCursor } from "@ci/db";

type Platform = "lichess" | "chesscom";

export interface ImportAccountDeps {
  db: Db;
  importers: { lichess: typeof fetchLichessGames; chesscom: typeof fetchChesscomGames };
  clients: { lichess: PlatformClient; chesscom: PlatformClient };
  /** Per-platform breakers, shared across jobs; platform calls go through them. */
  breakers?: Partial<Record<Platform, CircuitBreaker>>;
  lichessToken?: string;
  /** Called only after the game row is committed. */
  enqueueAnalyse(gameId: string): Promise<unknown>;
  publish?: PublishFn;
  /** Checkpoint the cursor and run after this many games (default 50)... */
  checkpointEvery?: number;
  /** ...or after this long since the last checkpoint (default 15 s), whichever comes first. */
  checkpointMs?: number;
  /** Abandon the run after this long without a streamed game (default 30 minutes). */
  idleDeadlineMs?: number;
  /**
   * Lichess lists a game by its creation time, so a correspondence game that finishes after a
   * newer one was imported would sit below the watermark for ever. Incremental Lichess passes
   * therefore re-fetch this many days below `newestSeenAt` (default 30; upserts make it free).
   */
  lichessSinceLookbackDays?: number;
  log?: Logger;
}

const DAY_MS = 24 * 3600 * 1000;

/** Maps a failure to the stable code stored on `import_runs.error_code`. */
export function importErrorCode(err: unknown): ImportErrorCode {
  if (err instanceof PlatformNotFoundError) return "not_found";
  if (err instanceof PlatformRateLimitError) return "rate_limited";
  if (err instanceof PlatformTimeoutError || err instanceof JobDeadlineError) return "timeout";
  if (err instanceof CircuitOpenError) return "circuit_open";
  return "unknown";
}

const MAX_ERROR_LENGTH = 2000;
const EMPTY_CURSOR: ImportCursor = { oldestReached: null, newestSeenAt: null };

/**
 * Imports an account's games, newest first, resuming from `linked_accounts.import_cursor`.
 *
 * A pass streams (since: newestSeenAt, until?: oldestReached) and checkpoints `oldestReached`
 * (the oldest game committed so far) every batch. When a pass finishes, `newestSeenAt` becomes the
 * newest game stored and `oldestReached` is cleared. A pass that stops at `--max` keeps
 * `oldestReached` (with `backfill` when it had no lower watermark), so the next run continues below it. A run first finishes any interrupted pass,
 * then fetches games newer than everything stored. Upserts make overlaps harmless.
 */
export async function handleImportAccount(
  deps: ImportAccountDeps,
  payload: ImportAccountPayload,
  attempt?: JobAttempt,
): Promise<{ imported: number; skipped: number }> {
  const { db } = deps;
  const log = deps.log ?? defaultLog;
  const checkpointEvery = deps.checkpointEvery ?? 50;
  const checkpointMs = deps.checkpointMs ?? 15_000;
  const [account] = await db
    .select()
    .from(linkedAccounts)
    .where(eq(linkedAccounts.id, payload.accountId));
  if (!account) throw new UnrecoverableError(`Linked account not found: ${payload.accountId}`);

  const [run] = await db
    .insert(importRuns)
    .values({ accountId: account.id, status: "running" })
    .returning({ id: importRuns.id });
  if (!run) throw new Error("import run insert returned no row");

  const publish = async (type: "import.progress" | "job.failed", body: object) => {
    try {
      await deps.publish?.(account.id, type, body);
    } catch (err) {
      log.warn({ accountId: account.id, err: errorMessage(err) }, "publish failed");
    }
  };

  const username = account.username.toLowerCase();
  const platform = account.platform;
  const breaker = deps.breakers?.[platform];
  const client = breaker
    ? withCircuitBreaker(deps.clients[platform], breaker)
    : deps.clients[platform];
  const fetchGames = (
    range: { since?: Date; until?: Date },
    passMax: number | undefined,
  ): AsyncIterable<NormalizedGame> => {
    const opts = {
      ...range,
      ...(passMax !== undefined ? { max: passMax } : {}),
    };
    return platform === "lichess"
      ? deps.importers.lichess(client, account.username, {
          ...opts,
          ...(deps.lichessToken ? { token: deps.lichessToken } : {}),
        })
      : deps.importers.chesscom(client, account.username, opts);
  };

  const lookbackMs = platform === "lichess" ? (deps.lichessSinceLookbackDays ?? 30) * DAY_MS : 0;
  /** The lower bound for a pass that starts at the watermark. */
  const sinceOf = (newestSeenAt: string) => new Date(new Date(newestSeenAt).getTime() - lookbackMs);

  let cursor: ImportCursor = { ...EMPTY_CURSOR, ...account.importCursor };
  let imported = 0;
  let skipped = 0;
  let frontier: Date | null = cursor.oldestReached ? new Date(cursor.oldestReached) : null;
  let sinceCheckpoint = 0;
  let lastCheckpoint = Date.now();

  const checkpoint = async () => {
    cursor = { ...cursor, oldestReached: frontier ? frontier.toISOString() : null };
    await db
      .update(linkedAccounts)
      .set({ importCursor: cursor })
      .where(eq(linkedAccounts.id, account.id));
    await db
      .update(importRuns)
      .set({ imported, heartbeatAt: new Date() })
      .where(eq(importRuns.id, run.id));
    sinceCheckpoint = 0;
    lastCheckpoint = Date.now();
    await publish("import.progress", { runId: run.id, imported });
  };

  const idleMs = deps.idleDeadlineMs ?? IMPORT_IDLE_DEADLINE_MS;
  const deadline = createDeadline("import-account", idleMs);

  /**
   * Streams one pass. Returns true when it stopped because it hit its `max` (the history may go
   * on), in which case the frontier is kept for the next run; false when the stream ended.
   */
  const runPass = async (
    range: { since?: Date; until?: Date },
    passMax: number | undefined = payload.max,
  ): Promise<boolean> => {
    const it = fetchGames(range, passMax)[Symbol.asyncIterator]();
    let streamed = 0;
    for (;;) {
      let next: IteratorResult<NormalizedGame>;
      try {
        next = await abortable(it.next(), deadline.signal);
      } catch (err) {
        // Do not await: a stream stuck in an await cannot run its cleanup until it wakes.
        void it.return?.()?.catch(() => {});
        throw err;
      }
      if (next.done) break;
      deadline.reset();
      streamed++;
      const g = next.value;
      const playerColor =
        g.white.name.toLowerCase() === username
          ? "white"
          : g.black.name.toLowerCase() === username
            ? "black"
            : null;
      if (playerColor === null) {
        skipped++;
      } else {
        const res = await upsertGame(db, {
          accountId: account.id,
          platform: g.platform,
          platformGameId: g.platformGameId,
          pgn: g.pgn,
          playedAt: g.playedAt,
          timeClass: g.timeClass,
          baseSeconds: g.baseSeconds,
          incrementSeconds: g.incrementSeconds,
          rated: g.rated,
          playerColor,
          whiteName: g.white.name,
          blackName: g.black.name,
          whiteRating: g.white.rating,
          blackRating: g.black.rating,
          result: g.result,
          termination: g.termination,
          eco: g.eco,
          openingName: g.openingName,
          lichessEvals: g.lichessEvals,
          analysisStatus: "pending",
        });
        if (res.inserted) imported++;
        // Also re-enqueue existing pending games: a previous run may have died between the
        // insert and the enqueue. The deterministic job id dedupes.
        if (res.analysisStatus === "pending") await deps.enqueueAnalyse(res.id);
      }
      if (frontier === null || g.playedAt < frontier) frontier = g.playedAt;
      sinceCheckpoint++;
      if (sinceCheckpoint >= checkpointEvery || Date.now() - lastCheckpoint >= checkpointMs) {
        await checkpoint();
      }
    }
    // A pass cut off by max has not reached its lower bound: keep the frontier.
    if (passMax !== undefined && streamed >= passMax) {
      const [top] = await db
        .select({ newest: max(games.playedAt) })
        .from(games)
        .where(eq(games.accountId, account.id));
      // A pass without a lower bound (first run, or a backfill) has imported everything above
      // its frontier, so the watermark moves up. A pass bounded by an old watermark leaves it:
      // the gap between the two is what the next run resumes.
      if (range.since === undefined) {
        const seen = cursor.newestSeenAt ? new Date(cursor.newestSeenAt) : null;
        const newest = [top?.newest, seen]
          .filter((d): d is Date => d instanceof Date)
          .sort((a, b) => b.getTime() - a.getTime())[0];
        cursor = { ...cursor, newestSeenAt: newest ? newest.toISOString() : null, backfill: true };
      }
      await checkpoint();
      return true;
    }
    // The pass reached its lower bound: everything up to the newest stored game is imported.
    const [row] = await db
      .select({ newest: max(games.playedAt) })
      .from(games)
      .where(eq(games.accountId, account.id));
    const newest = [row?.newest, cursor.newestSeenAt ? new Date(cursor.newestSeenAt) : null]
      .filter((d): d is Date => d instanceof Date)
      .sort((a, b) => b.getTime() - a.getTime())[0];
    frontier = null;
    cursor = { oldestReached: null, newestSeenAt: newest ? newest.toISOString() : null };
    await checkpoint();
    return false;
  };

  try {
    let stopped = false;
    if (cursor.oldestReached) {
      const backfill = cursor.backfill === true;
      let budget = payload.max;
      if (backfill && payload.max !== undefined) {
        // --max is a total: only the shortfall is fetched, plus the inclusive boundary game.
        const [stored] = await db
          .select({ n: count() })
          .from(games)
          .where(eq(games.accountId, account.id));
        budget = payload.max - (stored?.n ?? 0) + 1;
        stopped = budget <= 1;
      }
      if (!stopped) {
        stopped = await runPass(
          {
            until: new Date(cursor.oldestReached),
            ...(cursor.newestSeenAt && !backfill ? { since: sinceOf(cursor.newestSeenAt) } : {}),
          },
          budget,
        );
      }
    }
    if (!stopped) {
      await runPass(cursor.newestSeenAt ? { since: sinceOf(cursor.newestSeenAt) } : {});
    }
  } catch (err) {
    deadline.clear();
    const message = errorMessage(err).slice(0, MAX_ERROR_LENGTH);
    const errorCode = importErrorCode(err);
    // An open breaker is not this run failing: the job is delayed (not retried) and starts a
    // new run once the platform is probed again.
    const paused = err instanceof CircuitOpenError;
    try {
      // Keep the progress made: the retry resumes below the last committed game.
      if (frontier !== null) {
        cursor = { ...cursor, oldestReached: frontier.toISOString() };
        await db
          .update(linkedAccounts)
          .set({ importCursor: cursor })
          .where(eq(linkedAccounts.id, account.id));
      }
      await db
        .update(importRuns)
        .set({
          status: paused ? "paused" : "failed",
          imported,
          lastError: message,
          errorCode,
          finishedAt: new Date(),
        })
        .where(eq(importRuns.id, run.id));
    } catch {
      // Keep the original error; the reconciler handles a run left "running".
    }
    if (!paused && isFinalAttempt(attempt) && !(err instanceof UnrecoverableError)) {
      await publish("job.failed", { kind: "import-account", runId: run.id, reason: message });
    }
    throw err;
  }

  deadline.clear();
  const finishedAt = new Date();
  await db
    .update(importRuns)
    .set({ status: "done", imported, finishedAt, heartbeatAt: finishedAt })
    .where(eq(importRuns.id, run.id));
  await db
    .update(linkedAccounts)
    .set({ lastSyncedAt: finishedAt })
    .where(eq(linkedAccounts.id, account.id));
  await publish("import.progress", { runId: run.id, imported, done: true });
  return { imported, skipped };
}

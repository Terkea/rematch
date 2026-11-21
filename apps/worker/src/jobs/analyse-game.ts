import {
  buildMoveRecords,
  type EngineScore,
  gameAccuracy,
  parsePgn,
  UnsupportedVariantError,
  winPercent,
} from "@ci/chess-core";
import { type Db, games, moves } from "@ci/db";
import { eq } from "drizzle-orm";
import type { EvalCache } from "../analysis/eval-cache.js";
import { evaluateGame } from "../analysis/evaluate-game.js";
import type { EnginePool } from "../engine/engine-pool.js";
import { ANALYSIS_PROTOCOL } from "../engine/protocol.js";
import type { AnalyseGamePayload } from "../queue/queues.js";

export interface AnalyseGameDeps {
  db: Db;
  pool: Pick<EnginePool, "analyse">;
  cache: EvalCache;
}

const MAX_ERROR_LENGTH = 2000;
const INSERT_CHUNK = 500;

export async function handleAnalyseGame(
  deps: AnalyseGameDeps,
  payload: AnalyseGamePayload,
): Promise<{ status: "done" | "skipped" }> {
  const { db } = deps;
  const [game] = await db.select().from(games).where(eq(games.id, payload.gameId));
  if (!game) throw new Error(`Game not found: ${payload.gameId}`);

  // A retry (or a re-run after failure) always starts again from here.
  await db
    .update(games)
    .set({ analysisStatus: "analysing", analysisError: null })
    .where(eq(games.id, game.id));

  try {
    let parsed: ReturnType<typeof parsePgn>;
    try {
      parsed = parsePgn(game.pgn);
    } catch (err) {
      if (!(err instanceof UnsupportedVariantError)) throw err;
      await markSkipped(db, game.id);
      return { status: "skipped" };
    }
    if (parsed.plies.length < 2) {
      await markSkipped(db, game.id);
      return { status: "skipped" };
    }

    const evals = await evaluateGame(
      { pool: deps.pool, cache: deps.cache },
      parsed,
      (game.lichessEvals as (EngineScore | null)[] | null) ?? null,
    );
    const clock =
      game.baseSeconds !== null && game.incrementSeconds !== null
        ? { baseSeconds: game.baseSeconds, incrementSeconds: game.incrementSeconds }
        : null;
    const records = buildMoveRecords(parsed, evals, clock);
    const winPercents = evals.map((e) => winPercent(e.score));
    const whiteAccuracy = gameAccuracy(winPercents, "white");
    const blackAccuracy = gameAccuracy(winPercents, "black");

    await db.transaction(async (tx) => {
      await tx.delete(moves).where(eq(moves.gameId, game.id));
      const rows = records.map((r) => ({ gameId: game.id, ...r }));
      for (let i = 0; i < rows.length; i += INSERT_CHUNK) {
        await tx.insert(moves).values(rows.slice(i, i + INSERT_CHUNK));
      }
      await tx
        .update(games)
        .set({
          analysisStatus: "done",
          analysisProtocol: ANALYSIS_PROTOCOL,
          analysisError: null,
          whiteAccuracy,
          blackAccuracy,
        })
        .where(eq(games.id, game.id));
    });
    return { status: "done" };
  } catch (err) {
    const message = (err instanceof Error ? err.message : String(err)).slice(0, MAX_ERROR_LENGTH);
    try {
      await db
        .update(games)
        .set({ analysisStatus: "failed", analysisError: message })
        .where(eq(games.id, game.id));
    } catch {
      // Keep the original error; the retry will set the status again.
    }
    throw err;
  }
}

async function markSkipped(db: Db, gameId: string): Promise<void> {
  await db
    .update(games)
    .set({ analysisStatus: "skipped", analysisError: null })
    .where(eq(games.id, gameId));
}

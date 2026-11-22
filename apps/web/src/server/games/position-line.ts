import { parsePgn } from "@ci/chess-core";
import { type Db, games, moves } from "@ci/db";
import { moveFeatures } from "@ci/db/generated";
import { Chess } from "chess.js";
import { and, eq, inArray } from "drizzle-orm";
import { readOnly } from "./read-only";
import { scopedAccountIds } from "./scope";

// The position explorer's expanded data (feat/position-explorer), fetched when a cited position
// is first expanded: the game's moves within LINE_CONTEXT plies of the cited one, read from the
// PGN like the review, and the engine's stored line from the position before it
// (`move_features.best_pv`, the same line the coach cites; else the analysis' `best_uci` alone).
// A stored move that is not legal where it stands ends the line; nothing is ever made up.

/** Mirrors `@ci/ui/position-explorer` `ExplorerMove`. */
export type ExplorerMove = {
  ply: number;
  color: "white" | "black";
  san: string;
  uci: string;
  fenBefore: string;
  fenAfter: string;
};

/** Mirrors `@ci/ui/position-explorer` `PositionLine`. */
export type PositionLine = { game: ExplorerMove[]; engine: ExplorerMove[] };

/** Plies of the game shown either side of the cited one. */
export const LINE_CONTEXT = 8;
/** The longest engine line shown. */
export const MAX_ENGINE_PLIES = 16;

const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const UCI_RE = /^[a-h][1-8][a-h][1-8][qrbn]?$/;

/** `uci` from `fen`, numbered on from `ply`, stopping at the first move that is not legal. */
function playLine(fen: string, uci: readonly string[], ply: number): ExplorerMove[] {
  const out: ExplorerMove[] = [];
  let chess: Chess;
  try {
    chess = new Chess(fen);
  } catch {
    return out;
  }
  for (const [i, u] of uci.slice(0, MAX_ENGINE_PLIES).entries()) {
    if (!UCI_RE.test(u)) break;
    const before = chess.fen();
    try {
      const m = chess.move({ from: u.slice(0, 2), to: u.slice(2, 4), promotion: u[4] });
      out.push({
        ply: ply + i,
        color: m.color === "w" ? "white" : "black",
        san: m.san,
        uci: `${m.from}${m.to}${m.promotion ?? ""}`,
        fenBefore: before,
        fenAfter: chess.fen(),
      });
    } catch {
      break;
    }
  }
  return out;
}

/**
 * The expanded data for `ply` of one of the user's games (`games.id`), read in a READ ONLY
 * transaction, or null for a malformed id, a game the user may not read, or a ply not in it.
 */
export async function positionLine(
  db: Db,
  userId: string,
  gameId: string,
  ply: number,
): Promise<PositionLine | null> {
  if (!UUID_RE.test(gameId) || !Number.isInteger(ply) || ply < 1) return null;
  return readOnly(db, async (tx) => {
    const ids = await scopedAccountIds(tx, userId);
    if (ids.length === 0) return null;
    const [g] = await tx
      .select({ id: games.id, pgn: games.pgn })
      .from(games)
      .where(and(eq(games.id, gameId), inArray(games.accountId, ids)));
    if (!g) return null;
    let plies: ReturnType<typeof parsePgn>["plies"];
    try {
      plies = parsePgn(g.pgn).plies;
    } catch {
      return null;
    }
    const cited = plies[ply - 1];
    if (!cited || cited.ply !== ply) return null;

    const lo = Math.max(1, ply - LINE_CONTEXT);
    const hi = Math.min(plies.length, ply + LINE_CONTEXT);
    const game = plies.slice(lo - 1, hi).map((p) => ({
      ply: p.ply,
      color: p.color,
      san: p.san,
      uci: p.uci,
      fenBefore: p.fenBefore,
      fenAfter: p.fenAfter,
    }));

    const [features] = await tx
      .select({ bestPv: moveFeatures.bestPv })
      .from(moveFeatures)
      .where(and(eq(moveFeatures.gameId, g.id), eq(moveFeatures.ply, ply)));
    let pv = features?.bestPv ?? [];
    if (pv.length === 0) {
      const [m] = await tx
        .select({ bestUci: moves.bestUci })
        .from(moves)
        .where(and(eq(moves.gameId, g.id), eq(moves.ply, ply)));
      pv = m?.bestUci ? [m.bestUci] : [];
    }
    return { game, engine: playLine(cited.fenBefore, pv, ply) };
  });
}

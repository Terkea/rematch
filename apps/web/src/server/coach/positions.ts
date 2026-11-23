import type { Db } from "@ci/db";
import { games, moves } from "@ci/db/generated";
import { Chess } from "chess.js";
import { and, eq, inArray } from "drizzle-orm";
import type { Segment } from "../coaching/types.js";
import { scopedAccountIds } from "../games/scope.js";

// The positions a thread's answers cite (Fix D, N5), read with the thread so a board costs the
// browser no request of its own. A `move`, `best` or `pv` segment names a game by its PLATFORM id
// and the ply of the move played (Rust `segments.rs`); the board is that move's `fen_before`,
// from the player's side, with the move played and the engine's `best_uci`. Only the
// conversation's own account is read, and only while the user is linked to it.

/** Mirrors `@ci/ui/coach` `CitedPosition`. */
export type CitedPosition = {
  gameId: string;
  ply: number;
  href: string;
  /** The review's game id: the explorer loads the game around the ply by it. */
  reviewGameId: string;
  fen: string;
  orientation: "white" | "black";
  moveText: string;
  played: { san: string; uci: string };
  best: { san: string; uci: string } | null;
  mistake: boolean;
  /** The player's own judged move's stored judgement (its glyph on the board), else null. */
  judgement: "inaccuracy" | "mistake" | "blunder" | null;
};

/** Mirrors `@ci/ui/coach` `citedKey`. */
export const citedKey = (gameId: string, ply: number): string => `${gameId}#${ply}`;

/** At most this many boards per thread; later citations stay links. */
export const MAX_CITED_POSITIONS = 60;

type Cited = { gameId: string; ply: number };

/** The (game, ply) pairs the segments cite, once each, in order. */
export function citedPlies(segments: readonly (readonly Segment[] | null)[]): Cited[] {
  const seen = new Set<string>();
  const out: Cited[] = [];
  for (const list of segments) {
    for (const s of list ?? []) {
      if (s.kind !== "move" && s.kind !== "best" && s.kind !== "pv") continue;
      if (typeof s.gameId !== "string" || s.gameId === "" || s.gameId.length > 200) continue;
      if (!Number.isInteger(s.ply) || s.ply < 1) continue;
      const key = citedKey(s.gameId, s.ply);
      if (seen.has(key)) continue;
      seen.add(key);
      out.push({ gameId: s.gameId, ply: s.ply });
    }
  }
  return out.slice(0, MAX_CITED_POSITIONS);
}

/** "30. Rxg7" for White, "30… Rxg7" for Black (`formatNotation` without a glyph). */
const moveText = (ply: number, color: "white" | "black", san: string) =>
  `${Math.ceil(ply / 2)}${color === "white" ? "." : "…"} ${san}`;

/** The engine's move in SAN from `fen`, or null when it is not a legal move there. */
function sanOf(fen: string, uci: string): string | null {
  if (!/^[a-h][1-8][a-h][1-8][qrbn]?$/.test(uci)) return null;
  try {
    return new Chess(fen).move({ from: uci.slice(0, 2), to: uci.slice(2, 4), promotion: uci[4] })
      .san;
  } catch {
    return null;
  }
}

/**
 * The cited positions of `accountId`'s games, keyed by `citedKey` (platform id and ply). A ply
 * that is not one of that account's analysed moves is left out, and so is everything when the
 * user is not linked to the account.
 */
export async function citedPositions(
  tx: Db,
  userId: string,
  accountId: string,
  cited: readonly Cited[],
): Promise<Record<string, CitedPosition>> {
  if (cited.length === 0) return {};
  if ((await scopedAccountIds(tx, userId, accountId)).length === 0) return {};
  const wanted = new Set(cited.map((c) => citedKey(c.gameId, c.ply)));
  const rows = await tx
    .select({
      id: games.id,
      platformGameId: games.platformGameId,
      playerColor: games.playerColor,
      ply: moves.ply,
      color: moves.color,
      san: moves.san,
      uci: moves.uci,
      fenBefore: moves.fenBefore,
      bestUci: moves.bestUci,
      judgement: moves.judgement,
    })
    .from(moves)
    .innerJoin(games, eq(games.id, moves.gameId))
    .where(
      and(
        eq(games.accountId, accountId),
        inArray(games.platformGameId, [...new Set(cited.map((c) => c.gameId))]),
        inArray(moves.ply, [...new Set(cited.map((c) => c.ply))]),
      ),
    );
  const out: Record<string, CitedPosition> = {};
  for (const r of rows) {
    const key = citedKey(r.platformGameId, r.ply);
    if (!wanted.has(key)) continue;
    const bestSan = r.bestUci && r.bestUci !== r.uci ? sanOf(r.fenBefore, r.bestUci) : null;
    out[key] = {
      gameId: r.platformGameId,
      ply: r.ply,
      href: `/app/games/${encodeURIComponent(r.id)}?ply=${r.ply}`,
      reviewGameId: r.id,
      fen: r.fenBefore,
      orientation: r.playerColor,
      moveText: moveText(r.ply, r.color, r.san),
      played: { san: r.san, uci: r.uci },
      best: bestSan && r.bestUci ? { san: bestSan, uci: r.bestUci } : null,
      mistake:
        r.color === r.playerColor && (r.judgement === "mistake" || r.judgement === "blunder"),
      judgement: r.color === r.playerColor ? (r.judgement ?? null) : null,
    };
  }
  return out;
}

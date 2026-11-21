export type Phase = "opening" | "middlegame" | "endgame";

const OPENING_MIN_PLY = 20;
const OPENING_MAX_PLY = 30;
const PIECE_VALUE: Record<string, number> = { q: 9, r: 5, b: 3, n: 3 };

interface Material {
  white: number;
  black: number;
  queens: number;
}

function nonPawnMaterial(fen: string): Material {
  const board = fen.split(" ")[0] ?? "";
  const m: Material = { white: 0, black: 0, queens: 0 };
  for (const ch of board) {
    const value = PIECE_VALUE[ch.toLowerCase()];
    if (value === undefined) continue;
    if (ch === ch.toUpperCase()) m.white += value;
    else m.black += value;
    if (ch.toLowerCase() === "q") m.queens += 1;
  }
  return m;
}

function isEndgame(fen: string): boolean {
  const m = nonPawnMaterial(fen);
  if (m.white <= 13 && m.black <= 13) return true;
  return m.queens === 0 && m.white + m.black <= 26;
}

/**
 * Phase of each position. `fensBefore[i]` is the position before ply i + 1.
 * Opening lasts until ply max(20, bookExitPly), capped at 30. Afterwards the
 * phase is endgame by material, else middlegame, and never goes backwards.
 */
export function detectPhases(fensBefore: string[], bookExitPly = OPENING_MIN_PLY): Phase[] {
  const openingEnd = Math.min(OPENING_MAX_PLY, Math.max(OPENING_MIN_PLY, bookExitPly));
  let reachedEndgame = false;
  return fensBefore.map((fen, i): Phase => {
    if (i + 1 <= openingEnd) return "opening";
    if (!reachedEndgame && isEndgame(fen)) reachedEndgame = true;
    return reachedEndgame ? "endgame" : "middlegame";
  });
}

import { Chess } from "chess.js";

// The free-capture check (N11) the strengths and the trainer both use. Pure, with no database
// import, so the trainer's rules (`train/model.ts`) and the lab bridge (ADR 0115) load it alone.
// It is part of the trainer's selection rule (`keepCandidate`): the algorithm manifest digests it.

const VALUE: Record<string, number> = { p: 1, n: 3, b: 3, r: 5, q: 9, k: 100 };

/**
 * True for a capture anyone would find (N11): a recapture on the square the opponent just
 * captured on, a capture of more than the capturing piece is worth, or one nothing can legally
 * take back. False for a quiet move or a move that is not legal in `fen`.
 */
export function obviousCapture(fen: string, uci: string, prevSan: string | null): boolean {
  let chess: Chess;
  let move: ReturnType<Chess["move"]>;
  try {
    chess = new Chess(fen);
    move = chess.move({
      from: uci.slice(0, 2),
      to: uci.slice(2, 4),
      ...(uci.length > 4 ? { promotion: uci.slice(4, 5) } : {}),
    });
  } catch {
    return false;
  }
  if (!move.captured) return false;
  if (prevSan?.includes("x") && prevSan.replace(/[+#?!]/g, "").endsWith(move.to)) return true;
  if ((VALUE[move.captured] ?? 0) > (VALUE[move.piece] ?? 0)) return true;
  return !chess.moves({ verbose: true }).some((m) => m.to === move.to && m.captured);
}

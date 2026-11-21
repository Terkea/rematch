import { Chess } from "chess.js";

const UCI = /^([a-h][1-8])([a-h][1-8])([qrbn])?$/;

/** Plays `uci` from `fen`; stops at the first illegal move. Never throws. SAN is as chess.js renders it. */
export function pvToSan(fen: string, uci: string[]): string[] {
  const out: string[] = [];
  let chess: Chess;
  try {
    chess = new Chess(fen);
  } catch {
    return out;
  }
  for (const u of uci) {
    const m = UCI.exec(u);
    if (!m) break;
    try {
      const mv = chess.move({ from: m[1] as string, to: m[2] as string, promotion: m[3] });
      out.push(mv.san);
    } catch {
      break;
    }
  }
  return out;
}

/** FEN after playing `uci` from `fen`, or null when illegal or the FEN is invalid. */
export function fenAfter(fen: string, uci: string): string | null {
  try {
    const chess = new Chess(fen);
    const m = UCI.exec(uci);
    if (!m) return null;
    chess.move({ from: m[1] as string, to: m[2] as string, promotion: m[3] });
    return chess.fen();
  } catch {
    return null;
  }
}

export function stripSan(san: string): string {
  return san.replace(/[+#!?]+$/g, "");
}

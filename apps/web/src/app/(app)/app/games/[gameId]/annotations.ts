import type { Annotation, Glyph, Square } from "@ci/ui/annotations";
import { judgementGlyph } from "@ci/ui/board";
import type { ReviewView } from "./view-model";

// The board's marks at the shown ply (R8, as redesigned in M5a T10 from the T7 critique).
// - The player's mistake or blunder: a red-pen stroke of the move played, from the square it left
//   to the EDGE of the square it reached (`toEdge`), so no ink covers the piece. No ring: on a
//   phone the ring, the arrowhead and the badge made one red blob over the piece.
// - Any judged move: the engine's better move as a `good` arrow (solid head), and the glyph badge
//   on a top corner of the destination, graded: the player's ?? solid pen, ? a pen outline, ?!
//   pencil; the opponent's always pencil.
// - "Show engine move": one pencil arrow, the engine's choice in the shown position (the next
//   ply's `bestUci`).

const squares = (uci: string) => ({
  from: uci.slice(0, 2) as Square,
  to: uci.slice(2, 4) as Square,
});

export function annotationsFor(v: ReviewView, ply: number, showEngine: boolean): Annotation[] {
  if (v.status !== "analysed") return [];
  const out: Annotation[] = [];
  const p = ply > 0 ? v.plies[ply - 1] : undefined;
  const a = p?.analysis;
  if (p && a?.judgement) {
    const mine = p.color === v.playerColor;
    const played = squares(p.uci);
    if (mine && a.judgement !== "inaccuracy")
      out.push({ kind: "arrow", ...played, tone: "pen", toEdge: true });
    if (a.bestUci && a.bestUci !== p.uci)
      out.push({ kind: "arrow", ...squares(a.bestUci), tone: "good" });
    // Graded (fix round 1): only the player's ?? is full red; their ? is a pen outline, and ?!
    // and every opponent mark are pencil. The badge sits on the square's top edge, off the piece.
    const glyph = judgementGlyph[a.judgement] as Glyph;
    const pen = mine && a.judgement !== "inaccuracy";
    out.push({
      kind: "mark",
      square: played.to,
      glyph,
      tone: pen ? "pen" : "pencil",
      ...(pen && a.judgement === "mistake" ? { weight: "light" as const } : {}),
      place: "edge",
    });
  }
  if (showEngine) {
    const next = v.plies[ply]?.analysis?.bestUci;
    if (next) out.push({ kind: "arrow", ...squares(next), tone: "pencil" });
  }
  return out;
}

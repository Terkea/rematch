"use client";

import { Chess } from "chess.js";
import { useMemo } from "react";
import type { Annotation } from "../annotations/index.js";
import { Board, judgementGlyph, moveNumberText } from "../board/index.js";
import type { Orientation, Square } from "../board/theme.js";
import { cn } from "../lib/index.js";
import { ArrowSwatch } from "./arrow-swatch.js";
import { type ExemplarCaption, exemplarCaption, exemplarMoveText, fenMove } from "./text.js";

export interface ExemplarBoardProps {
  /** The position before the played move. */
  fen: string;
  /** Default: the side to move, which is the player in an exemplar. */
  orientation?: Orientation;
  played: { san: string; uci: string };
  best: { san: string; uci: string } | null;
  /** The first move of the refutation line (UCI), or null when there is none. */
  refutation: string | null;
  caption: ExemplarCaption;
  className?: string;
}

const SQUARE = /^[a-h][1-8]$/;

/** `e2e4` or `e7e8q` to its squares, or null for anything else. */
function squares(uci: string | null | undefined): { from: Square; to: Square } | null {
  if (!uci) return null;
  const from = uci.slice(0, 2);
  const to = uci.slice(2, 4);
  if (!SQUARE.test(from) || !SQUARE.test(to) || from === to) return null;
  return { from: from as Square, to: to as Square };
}

/** The SAN of the reply after the played move, or null when either move is not legal here. */
function replySan(fen: string, played: string, reply: string | null): string | null {
  if (!reply) return null;
  try {
    const chess = new Chess(fen);
    chess.move({ from: played.slice(0, 2), to: played.slice(2, 4), promotion: played[4] ?? "q" });
    return chess.move({
      from: reply.slice(0, 2),
      to: reply.slice(2, 4),
      promotion: reply[4] ?? "q",
    }).san;
  } catch {
    return null;
  }
}

/**
 * One example position from a weakness (ADR 0072): the position before the move, the played move
 * as a `pen` arrow (with its stored glyph as a badge), the better move as a `good` arrow and the
 * first move of the refutation as a `pencil` arrow, with the caption in notation ("14. d4? Better
 * was Nxe5. Win chance 52% to 36%."). Static: no move input, but the board's square grid is a tab
 * stop, so a keyboard user can read it square by square. Arrows draw on with the pen and appear
 * fully drawn under reduced motion (the board's own rule).
 */
export function ExemplarBoard({
  fen,
  orientation,
  played,
  best,
  refutation,
  caption,
  className,
}: ExemplarBoardProps) {
  const toMove = fenMove(fen).color;
  const bestUci = best && best.uci !== played.uci ? best.uci : null;
  const judgement = caption.judgement ?? null;
  const annotations = useMemo(() => {
    const out: Annotation[] = [];
    const bestSq = squares(bestUci);
    const refSq = squares(refutation);
    const playedSq = squares(played.uci);
    // The move and its reply are a sequence (1, 2); the better move is the alternative to the
    // move played, so it carries no number.
    const seq = playedSq !== null && refSq !== null;
    if (bestSq) out.push({ kind: "arrow", ...bestSq, tone: "good" });
    if (refSq) out.push({ kind: "arrow", ...refSq, tone: "pencil", ...(seq ? { step: 2 } : {}) });
    if (playedSq) {
      out.push({
        kind: "arrow",
        ...playedSq,
        tone: "pen",
        toEdge: true,
        ...(seq ? { step: 1 } : {}),
      });
      if (judgement) {
        out.push({
          kind: "mark",
          square: playedSq.to,
          glyph: judgementGlyph[judgement],
          tone: "pen",
        });
      }
    }
    return out;
  }, [bestUci, refutation, played.uci, judgement]);
  const tones = new Set(annotations.map((a) => a.tone));
  const steps = annotations.some((a) => a.kind === "arrow" && a.step);
  const move = exemplarMoveText(fen, played.san);
  const mover = fenMove(fen);
  // The reply's own move number: Black answers on White's number, White on the next one.
  const replyNumber =
    mover.color === "white"
      ? moveNumberText(mover.moveNumber, "black")
      : moveNumberText(mover.moveNumber + 1, "white");
  const reply = useMemo(() => replySan(fen, played.uci, refutation), [fen, played.uci, refutation]);
  // Keep the win-chance pair on one line, with no-break spaces around "to".
  const text = exemplarCaption({ fen, played, best, caption }).replace(
    /(\d+%) to (\d+%)/,
    "$1 to $2",
  );

  return (
    <figure data-exemplar className={cn("m-0 flex w-fit flex-col gap-3", className)}>
      <Board
        fen={fen}
        orientation={orientation ?? toMove}
        label={`Position before ${move}`}
        annotations={annotations}
      />
      <figcaption className="flex max-w-(--board-size) flex-col gap-2">
        <p className="m-0 font-serif text-prose text-ink [font-variant-numeric:lining-nums_tabular-nums]">
          {text}
        </p>
        <ul
          aria-label="Arrows"
          className="m-0 flex list-none flex-wrap gap-x-4 gap-y-1 p-0 text-sm text-ink-soft"
        >
          {/* The sequence first, in move order and numbered as on the board; then the
              alternative. */}
          {tones.has("pen") ? (
            <li className="flex items-center gap-1.5">
              <ArrowSwatch tone="pen" />
              {steps ? `1. Played, ${move}` : `Played, ${played.san}`}
            </li>
          ) : null}
          {tones.has("pencil") ? (
            <li className="flex items-center gap-1.5">
              <ArrowSwatch tone="pencil" />
              {steps ? "2. " : ""}
              {reply ? `Their reply, ${steps ? `${replyNumber} ` : ""}${reply}` : "Their reply"}
            </li>
          ) : null}
          {tones.has("good") ? (
            <li className="flex items-center gap-1.5">
              <ArrowSwatch tone="good" />
              Better, {best?.san}
            </li>
          ) : null}
        </ul>
      </figcaption>
    </figure>
  );
}

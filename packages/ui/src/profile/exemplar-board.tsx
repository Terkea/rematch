"use client";

import { useMemo } from "react";
import type { Annotation } from "../annotations/index.js";
import { Board, judgementGlyph } from "../board/index.js";
import type { Orientation, Square } from "../board/theme.js";
import { cn } from "../lib/index.js";
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

/** The key's arrow swatch: the tone's own head shape, as on the board. */
function ArrowSwatch({ tone }: { tone: "pen" | "good" | "pencil" }) {
  const stroke = { pen: "stroke-pen", good: "stroke-good", pencil: "stroke-pencil" }[tone];
  const fill = { pen: "fill-pen", good: "fill-good", pencil: "fill-pencil" }[tone];
  return (
    <svg aria-hidden="true" width={22} height={12} className="shrink-0">
      <line
        x1={1}
        y1={6}
        x2={tone === "good" ? 14 : 18}
        y2={6}
        strokeWidth={2.5}
        className={stroke}
      />
      {tone === "good" ? (
        <path d="M13,1.5L21,6L13,10.5Z" className={fill} />
      ) : (
        <path
          d="M15,2L20,6L15,10"
          fill="none"
          strokeWidth={2}
          strokeLinecap="round"
          strokeLinejoin="round"
          className={stroke}
        />
      )}
    </svg>
  );
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
    if (bestSq) out.push({ kind: "arrow", ...bestSq, tone: "good" });
    if (refSq) out.push({ kind: "arrow", ...refSq, tone: "pencil" });
    if (playedSq) {
      out.push({ kind: "arrow", ...playedSq, tone: "pen", toEdge: true });
      if (judgement) {
        out.push({
          kind: "mark",
          square: playedSq.to,
          glyph: judgementGlyph[judgement],
          tone: "pen",
          place: "edge",
        });
      }
    }
    return out;
  }, [bestUci, refutation, played.uci, judgement]);
  const tones = new Set(annotations.map((a) => a.tone));
  const move = exemplarMoveText(fen, played.san);

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
          {exemplarCaption({ fen, played, best, caption })}
        </p>
        <ul
          aria-label="Arrows"
          className="m-0 flex list-none flex-wrap gap-x-4 gap-y-1 p-0 text-sm text-ink-soft"
        >
          {tones.has("pen") ? (
            <li className="flex items-center gap-1.5">
              <ArrowSwatch tone="pen" />
              Played
            </li>
          ) : null}
          {tones.has("good") ? (
            <li className="flex items-center gap-1.5">
              <ArrowSwatch tone="good" />
              Better
            </li>
          ) : null}
          {tones.has("pencil") ? (
            <li className="flex items-center gap-1.5">
              <ArrowSwatch tone="pencil" />
              The reply that punishes it
            </li>
          ) : null}
        </ul>
      </figcaption>
    </figure>
  );
}

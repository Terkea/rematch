"use client";

import { winPercent } from "@ci/chess-core";
import { useEffect, useRef } from "react";
import { cn } from "../lib/index.js";
import { animate, motion, spring, useMotionValue, useReducedMotion } from "../motion/index.js";
import { evalBarLabel, formatEval, type Score } from "./notation.js";
import type { Orientation } from "./theme.js";

export interface EvalBarProps {
  score: Score;
  /**
   * The position, so `{ mate: 0 }` (the side to move is mated) can tell the winner. Without it a
   * mate-0 score sits level.
   */
  fen?: string;
  orientation?: Orientation;
  label: string;
  /**
   * Below 768px, name both ends of the bar ("White", "Black") and leave the number to the host,
   * so the reading never depends on which end a jumping label sits at (M5a T10).
   */
  ends?: boolean;
  className?: string;
}

/** A change of 0.3 win chance or more swings on `swing-big`. Shares are (wc + 1) / 2, so 0.15. */
const BIG_SWING = 0.15;

/** White's share: win percentage for a centipawn score, the end of the bar for a forced mate. */
function whiteShare(score: Score, fen: string | undefined): number {
  if (!("mate" in score)) return winPercent(score) / 100;
  if (score.mate > 0) return 1;
  if (score.mate < 0) return 0;
  // Mate on the board: the side to move has been mated.
  const turn = fen?.split(" ")[1];
  return turn === "w" ? 0 : turn === "b" ? 1 : 0.5;
}

function describe(score: Score, share: number): string {
  const text = formatEval(score);
  if ("mate" in score && score.mate === 0) {
    return share === 0.5 ? "Checkmate" : `Checkmate, ${share > 0.5 ? "White" : "Black"} wins`;
  }
  if ("mate" in score) {
    return `${text}, ${score.mate > 0 ? "White" : "Black"} mates in ${Math.abs(score.mate)}`;
  }
  if (text === "0.0") return `${text}, equal`;
  return `${text}, ${share > 0.5 ? "White" : "Black"} is better`;
}

/**
 * The eval bar (DESIGN.md, "Eval bar"): White's share is its win percentage on the eval graph's
 * win-chance scale. 16px wide and the board's height from 768px; below that an 8px bar above the
 * board, with the label outside its end. The boundary moves on `swing`, or `swing-big` for big
 * changes, keeping its velocity when values come quickly; under reduced motion it is set at once.
 */
export function EvalBar({
  score,
  fen,
  orientation = "white",
  label,
  ends = false,
  className,
}: EvalBarProps) {
  // A forced mate maps to the end of the bar, as on the graph ("Eval graph", Mapping).
  const share = whiteShare(score, fen);
  const reduced = useReducedMotion();
  const fill = useMotionValue(share);
  const last = useRef(share);

  useEffect(() => {
    const from = last.current;
    last.current = share;
    if (from === share) return;
    if (reduced) {
      fill.jump(share);
      return;
    }
    const controls = animate(
      fill,
      share,
      Math.abs(share - from) >= BIG_SWING ? spring.swingBig : spring.swing,
    );
    return () => controls.stop();
  }, [share, reduced, fill]);

  const whiteWins = share >= 0.5;
  const text = evalBarLabel(score);
  // White sits at the bottom (or left, on the phone bar) when the board is seen from White's side.
  const whiteStart = orientation === "white";
  const labelAtStart = whiteWins === whiteStart;

  return (
    // biome-ignore lint/a11y/useSemanticElements: a native <meter> can't carry the sprung fill and label
    <div
      role="meter"
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round(share * 100)}
      aria-valuetext={describe(score, share)}
      data-orientation={orientation}
      data-winning={whiteWins ? "white" : "black"}
      className={cn(
        "flex w-(--board-size) items-center gap-2 md:h-(--board-size) md:w-4",
        ends || labelAtStart ? "flex-row" : "flex-row-reverse",
        className,
      )}
    >
      {ends ? (
        <span
          aria-hidden="true"
          data-end
          className="shrink-0 font-sans text-xs font-medium text-ink-soft md:hidden"
        >
          {whiteStart ? "White" : "Black"}
        </span>
      ) : (
        <span
          aria-hidden="true"
          className="shrink-0 font-serif text-2xs font-semibold text-ink [font-variant-numeric:lining-nums_tabular-nums] md:hidden"
        >
          {text}
        </span>
      )}
      <div className="relative h-2 min-w-0 flex-1 overflow-hidden rounded-(--radius-board) bg-ink md:h-full md:w-4 md:flex-none dark:bg-well">
        <motion.div
          data-testid="eval-fill"
          className={cn(
            "absolute inset-0 bg-sheet dark:bg-ink",
            "[transform:scaleX(var(--eval-share))] md:[transform:scaleY(var(--eval-share))]",
            whiteStart ? "origin-left md:origin-bottom" : "origin-right md:origin-top",
          )}
          style={{ "--eval-share": fill } as never}
        />
        <span
          aria-hidden="true"
          className={cn(
            "absolute inset-x-0 hidden justify-center md:flex",
            labelAtStart ? "bottom-1" : "top-1",
          )}
        >
          {/* Set vertically, reading upwards (rotated −90°), in the losing side's fill colour. */}
          <span
            className={cn(
              "rotate-180 font-serif text-2xs font-semibold [font-variant-numeric:lining-nums_tabular-nums] [writing-mode:vertical-rl]",
              whiteWins ? "text-ink dark:text-well" : "text-sheet dark:text-ink",
            )}
          >
            {text}
          </span>
        </span>
        <span
          aria-hidden="true"
          className="pointer-events-none absolute inset-0 rounded-(--radius-board) border border-rule-strong"
        />
      </div>
      {ends ? (
        <span
          aria-hidden="true"
          data-end
          className="shrink-0 font-sans text-xs font-medium text-ink-soft md:hidden"
        >
          {whiteStart ? "Black" : "White"}
        </span>
      ) : null}
    </div>
  );
}

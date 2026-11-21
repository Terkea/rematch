import { cn } from "../lib/index.js";
import { formatEval, judgementGlyph, moveNumberText, type NotationMove } from "./notation.js";

export interface NotationTextProps extends NotationMove {
  /**
   * Whose move it is. The player's mistakes are marked in pen; the opponent's are noted in
   * `pencil` (controller ruling, Task 13), so red always means "your mistake". Default `player`.
   */
  side?: "player" | "opponent";
  className?: string;
}

const judgementWord = { inaccuracy: "inaccuracy", mistake: "mistake", blunder: "blunder" } as const;

/**
 * A move in notation (`31. Rxe6?? −4.2`): Literata 600 with tabular lining figures, the glyph in
 * `pen-text` (or `pencil` for the opponent's move) kerned tight to the move, and the evaluation in `ink-faint` at 400 (DESIGN.md, "Type").
 * The text reads exactly as `formatNotation` gives it; the judgement is also named for assistive tech.
 */
export function NotationText({
  className,
  judgement,
  score,
  side = "player",
  ...move
}: NotationTextProps) {
  return (
    <span
      data-notation
      className={cn(
        "font-serif font-semibold text-ink [font-variant-numeric:lining-nums_tabular-nums]",
        className,
      )}
    >
      {moveNumberText(move.moveNumber, move.color)} {move.san}
      {judgement ? (
        <>
          <span
            data-glyph
            className={cn(
              "tracking-(--tracking-glyph)",
              side === "opponent" ? "text-pencil" : "text-pen-text",
            )}
          >
            {judgementGlyph[judgement]}
          </span>
          <span className="sr-only"> ({judgementWord[judgement]})</span>
        </>
      ) : null}
      {score ? <span className="font-normal text-ink-faint"> {formatEval(score)}</span> : null}
    </span>
  );
}

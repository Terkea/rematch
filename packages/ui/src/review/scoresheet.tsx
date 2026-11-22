"use client";

import { memo, useCallback, useId, useLayoutEffect, useMemo, useRef } from "react";
import { judgementGlyph, moveNumberText } from "../board/index.js";
import { cn } from "../lib/index.js";
import { motion, spring, useReducedMotion } from "../motion/index.js";

export type ScoresheetJudgement = "inaccuracy" | "mistake" | "blunder";

export interface ScoresheetMove {
  ply: number;
  color: "white" | "black";
  san: string;
  judgement: ScoresheetJudgement | null;
  /** The move matches the engine's best move. */
  best: boolean;
  /** The player's own move: its glyph is in pen. The opponent's is graphite. */
  mine: boolean;
}

export interface ScoresheetProps {
  /** Every ply in game order. */
  moves: ScoresheetMove[];
  /** The ply on the board; 0 is the start position and marks nothing. */
  currentPly: number;
  onSelectPly: (ply: number) => void;
  /** The region's accessible name, for example "Moves, DrNykterstein v Night-King96". */
  label: string;
  /** Size the sheet here (a height or `max-h-*`); the moves scroll inside it. */
  className?: string;
}

interface Row {
  number: number;
  white: ScoresheetMove | null;
  black: ScoresheetMove | null;
}

/** Pairs plies into numbered rows. A game that opens with Black starts on an empty White cell. */
function rowsOf(moves: ScoresheetMove[]): Row[] {
  const rows: Row[] = [];
  const offset = moves[0]?.color === "black" ? 1 : 0;
  moves.forEach((move, i) => {
    const index = Math.floor((i + offset) / 2);
    const row = rows[index] ?? { number: index + 1, white: null, black: null };
    rows[index] = row;
    if (move.color === "white") row.white = move;
    else row.black = move;
  });
  return rows;
}

/** "28. Nd2?? (blunder, your move)": the move as notation, then what the marks say. */
function accessibleName(move: ScoresheetMove, number: number): string {
  const glyph = move.judgement ? judgementGlyph[move.judgement] : "";
  const notes: string[] = [];
  if (move.judgement) notes.push(move.judgement);
  if (move.best) notes.push("best move");
  if (notes.length && move.mine) notes.push("your move");
  const base = `${moveNumberText(number, move.color)} ${move.san}${glyph}`;
  return notes.length ? `${base} (${notes.join(", ")})` : base;
}

interface CellProps {
  move: ScoresheetMove;
  number: number;
  current: boolean;
  cursorId: string;
  reduced: boolean;
  onSelect: (ply: number) => void;
  register: (ply: number, el: HTMLButtonElement | null) => void;
}

const Cell = memo(function Cell({
  move,
  number,
  current,
  cursorId,
  reduced,
  onSelect,
  register,
}: CellProps) {
  const ref = useCallback(
    (el: HTMLButtonElement | null) => register(move.ply, el),
    [register, move.ply],
  );
  return (
    <button
      ref={ref}
      type="button"
      id={`ply-${move.ply}`}
      data-ply={move.ply}
      aria-current={current ? "step" : undefined}
      aria-label={accessibleName(move, number)}
      onClick={() => onSelect(move.ply)}
      className={cn(
        "relative flex h-full min-w-0 items-center gap-2 rounded-field px-2 text-left",
        "font-serif text-base font-semibold text-ink [font-variant-numeric:lining-nums_tabular-nums]",
        "transition-colors duration-(--dur-quick) ease-out",
        "focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-ink",
        current ? null : "hover:bg-(--surface-hover)",
      )}
    >
      {current ? (
        // The cursor: one well that travels from move to move, like a finger down the sheet.
        <motion.span
          aria-hidden="true"
          data-cursor
          layoutId={cursorId}
          transition={reduced ? { duration: 0 } : spring.settle}
          className="absolute inset-0 rounded-field bg-well inset-ring-1 inset-ring-rule-strong"
        />
      ) : null}
      <span className="relative truncate">
        {move.san}
        {move.judgement ? (
          <span
            data-glyph
            className={cn(
              "tracking-(--tracking-glyph)",
              move.mine ? "text-pen-text" : "text-ink-soft",
            )}
          >
            {judgementGlyph[move.judgement]}
          </span>
        ) : null}
      </span>
      {move.best ? (
        <span
          data-best
          className="relative rounded-pill border border-good px-1.5 font-sans text-xs font-medium text-good-text"
        >
          Best
        </span>
      ) : null}
    </button>
  );
});

interface RowViewProps extends Omit<CellProps, "move" | "number" | "current"> {
  row: Row;
  /** Which of the row's cells is current, if any. */
  current: "white" | "black" | null;
}

const RowView = memo(function RowView({ row, current, ...cell }: RowViewProps) {
  return (
    <li
      className={cn(
        "col-span-3 grid h-(--space-scoresheet-row) grid-cols-subgrid items-stretch border-b border-rule py-0.5",
        "last:border-b-0 pointer-coarse:h-11",
      )}
    >
      <span
        className={cn(
          "self-center pr-2 pl-1 text-right font-serif text-sm [font-variant-numeric:lining-nums_tabular-nums]",
          current ? "text-ink" : "text-ink-faint",
        )}
      >
        {row.number}.
      </span>
      {row.white ? (
        <Cell {...cell} move={row.white} number={row.number} current={current === "white"} />
      ) : (
        <span aria-hidden="true" className="self-center px-2 font-serif text-ink-faint">
          …
        </span>
      )}
      {row.black ? (
        <Cell {...cell} move={row.black} number={row.number} current={current === "black"} />
      ) : (
        <span />
      )}
    </li>
  );
});

/**
 * The game's moves as a two-column scoresheet (White left, Black right), ruled like the paper one.
 * Each move is a button (`ply-<n>`); the current one has `aria-current="step"` and a `well` cursor
 * that travels between moves on `spring.settle` (instantly under reduced motion). The current move
 * is kept in view by scrolling this sheet only, never the page. Rows are memoised, so a step
 * re-renders two rows, not the game.
 */
export function Scoresheet({ moves, currentPly, onSelectPly, label, className }: ScoresheetProps) {
  const reduced = useReducedMotion();
  const cursorId = `scoresheet-cursor-${useId()}`;
  const rows = useMemo(() => rowsOf(moves), [moves]);
  const scroller = useRef<HTMLDivElement>(null);
  const buttons = useRef(new Map<number, HTMLButtonElement>());
  const select = useRef(onSelectPly);
  select.current = onSelectPly;

  const onSelect = useCallback((ply: number) => select.current(ply), []);
  const register = useCallback((ply: number, el: HTMLButtonElement | null) => {
    if (el) buttons.current.set(ply, el);
    else buttons.current.delete(ply);
  }, []);

  const mounted = useRef(false);
  useLayoutEffect(() => {
    const box = scroller.current;
    const el = buttons.current.get(currentPly);
    const first = !mounted.current;
    mounted.current = true;
    if (!box || !el) return;
    // "Nearest", measured in the sheet's own box: element.scrollIntoView would scroll the page too.
    const top = el.offsetTop;
    const bottom = top + el.offsetHeight;
    let next: number | null = null;
    if (top < box.scrollTop) next = top;
    else if (bottom > box.scrollTop + box.clientHeight) next = bottom - box.clientHeight;
    if (next === null) return;
    if (!first && !reduced && typeof box.scrollTo === "function") {
      box.scrollTo({ top: next, behavior: "smooth" });
    } else box.scrollTop = next;
  }, [currentPly, reduced]);

  return (
    <section
      aria-label={label}
      data-surface="sheet"
      className={cn("flex min-h-0 flex-col rounded-sheet bg-sheet", className)}
    >
      {rows.length === 0 ? (
        <p className="m-(--space-panel-pad) rounded-field bg-well px-4 py-6 text-center text-sm text-ink-soft">
          No moves in this game
        </p>
      ) : (
        <motion.div
          ref={scroller}
          layoutScroll
          data-scoresheet-scroll
          className="relative min-h-0 flex-1 overflow-y-auto overscroll-contain px-2 py-1"
        >
          <ol className="grid grid-cols-[auto_1fr_1fr]">
            {rows.map((row) => {
              const current =
                row.white?.ply === currentPly
                  ? "white"
                  : row.black?.ply === currentPly
                    ? "black"
                    : null;
              return (
                <RowView
                  key={row.number}
                  row={row}
                  current={current}
                  cursorId={cursorId}
                  reduced={reduced}
                  onSelect={onSelect}
                  register={register}
                />
              );
            })}
          </ol>
        </motion.div>
      )}
    </section>
  );
}

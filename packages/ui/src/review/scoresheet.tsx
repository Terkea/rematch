"use client";

import {
  type KeyboardEvent,
  memo,
  useCallback,
  useEffect,
  useId,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { judgementGlyph } from "../board/index.js";
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
  /**
   * Every ply in game order. Keep the array referentially stable (`useMemo` in the host): the rows
   * are memoised on it, so a new array each render re-renders every row on every step.
   */
  moves: ScoresheetMove[];
  /** The ply on the board; 0 is the start position and marks nothing. */
  currentPly: number;
  onSelectPly: (ply: number) => void;
  /** The region's accessible name, for example "Moves, DrNykterstein v Night-King96". */
  label: string;
  /** The number of the first row, for a game set up from a position (default 1). */
  startMoveNumber?: number;
  /**
   * Announce the current move politely, 300 ms after the last step, so holding a key reads only
   * the move it lands on (default true). Pass false while autoplaying; the move it stops on is
   * announced when this turns true again. Nothing is announced on mount.
   */
  announce?: boolean;
  /** Size the sheet here (a height or `max-h-*`); the moves scroll inside it. */
  className?: string;
}

/** The status line waits this long after the last step, so a held key reads one move. */
export const ANNOUNCE_DELAY_MS = 300;

interface Row {
  number: number;
  white: ScoresheetMove | null;
  black: ScoresheetMove | null;
}

/** Pairs plies into numbered rows. A game that opens with Black starts on an empty White cell. */
function rowsOf(moves: ScoresheetMove[], start: number): Row[] {
  const rows: Row[] = [];
  const offset = moves[0]?.color === "black" ? 1 : 0;
  moves.forEach((move, i) => {
    const index = Math.floor((i + offset) / 2);
    const row = rows[index] ?? { number: start + index, white: null, black: null };
    rows[index] = row;
    if (move.color === "white") row.white = move;
    else row.black = move;
  });
  return rows;
}

/**
 * "Move 28, White, Nd2, blunder, your move": words, not glyphs or the "…" of notation, so a screen
 * reader never reads "question question". "your move" is added only to a judged or best move,
 * where it says whose mark it is.
 */
function moveText(move: ScoresheetMove, number: number): string {
  const parts = [`Move ${number}`, move.color === "white" ? "White" : "Black", move.san];
  const notes: string[] = [];
  if (move.judgement) notes.push(move.judgement);
  if (move.best) notes.push("best move");
  if (notes.length && move.mine) notes.push("your move");
  return [...parts, ...notes].join(", ");
}

interface CellProps {
  move: ScoresheetMove;
  number: number;
  current: boolean;
  tabbable: boolean;
  cursorId: string;
  reduced: boolean;
  onSelect: (ply: number) => void;
  register: (ply: number, el: HTMLButtonElement | null) => void;
}

const Cell = memo(function Cell({
  move,
  number,
  current,
  tabbable,
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
      tabIndex={tabbable ? 0 : -1}
      aria-current={current ? "step" : undefined}
      aria-label={moveText(move, number)}
      onClick={() => onSelect(move.ply)}
      className={cn(
        "relative flex h-(--space-scoresheet-row) min-w-0 items-center gap-2 px-2 text-left pointer-coarse:h-11",
        "group/cell font-serif text-base font-semibold text-ink [font-variant-numeric:lining-nums_tabular-nums]",
        // Square, so the hit area has no rounded corners: the cursor and hover tone carry the radius.
        "focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-ink",
      )}
    >
      {/* The hover tone sits inside the row's ruling, like the cursor, so the hit area can fill it. */}
      <span
        aria-hidden="true"
        className={cn(
          "absolute inset-x-0 inset-y-0.5 rounded-field transition-colors duration-(--dur-quick) ease-out",
          current ? null : "group-hover/cell:bg-(--surface-hover)",
        )}
      />
      {current ? (
        // The cursor: one well that travels from move to move, like a finger down the sheet.
        <motion.span
          aria-hidden="true"
          data-cursor
          layoutId={cursorId}
          transition={reduced ? { duration: 0 } : spring.settle}
          className="absolute inset-x-0 inset-y-0.5 rounded-field bg-well inset-ring-1 inset-ring-rule-strong"
        />
      ) : null}
      <span className="relative truncate">
        {move.san}
        {move.judgement ? (
          <span
            data-glyph
            data-severity={move.judgement}
            className={cn(
              "tracking-(--tracking-glyph)",
              // Graded (M5a T10): only the player's ?? in full pen, ? lighter, ?! in ink.
              !move.mine
                ? "text-ink-soft"
                : move.judgement === "blunder"
                  ? "text-pen-text"
                  : move.judgement === "mistake"
                    ? "text-pen-text/75"
                    : "text-ink-soft",
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

interface RowViewProps extends Omit<CellProps, "move" | "number" | "current" | "tabbable"> {
  row: Row;
  /** Which of the row's cells is current, if any. */
  current: "white" | "black" | null;
  /** Which of the row's cells holds the sheet's one tab stop, if any. */
  tabStop: "white" | "black" | null;
}

const RowView = memo(function RowView({ row, current, tabStop, ...cell }: RowViewProps) {
  return (
    <li className={cn("col-span-3 grid grid-cols-subgrid border-b border-rule last:border-b-0")}>
      <span
        className={cn(
          "self-center pr-2 pl-1 text-right font-serif text-sm [font-variant-numeric:lining-nums_tabular-nums]",
          current ? "text-ink" : "text-ink-faint",
        )}
      >
        {row.number}.
      </span>
      {row.white ? (
        <Cell
          {...cell}
          move={row.white}
          number={row.number}
          current={current === "white"}
          tabbable={tabStop === "white"}
        />
      ) : (
        <span aria-hidden="true" className="self-center px-2 font-serif text-ink-faint">
          …
        </span>
      )}
      {row.black ? (
        <Cell
          {...cell}
          move={row.black}
          number={row.number}
          current={current === "black"}
          tabbable={tabStop === "black"}
        />
      ) : (
        <span />
      )}
    </li>
  );
});

const sideOf = (row: Row, ply: number): "white" | "black" | null =>
  row.white?.ply === ply ? "white" : row.black?.ply === ply ? "black" : null;

const COLUMNS = "grid-cols-[--spacing(10)_minmax(0,1fr)_minmax(0,1fr)]";

/**
 * The game's moves as a two-column scoresheet (White left, Black right) under its column heads,
 * ruled like the paper one.
 *
 * - Each move is a button (`ply-<n>`); the current one has `aria-current="step"` and a `well`
 *   cursor that travels between moves on `spring.settle` (instantly under reduced motion).
 * - The sheet is one tab stop (a roving tabindex on the current move, or the first at the start).
 *   Inside it the arrows move focus (left and right by ply, up and down by row, Home and End),
 *   without selecting; Enter or Space jumps to the focused move. When the ply changes while the
 *   current move has focus, focus follows it.
 * - A polite status line names the move landed on (see `announce`).
 * - The current move is kept in view by scrolling this sheet only, never the page, again when the
 *   moves arrive and when the sheet is resized.
 * - Rows are 34px, 44px on a coarse pointer, and the move buttons fill them.
 * - Rows are memoised, so a step re-renders two rows, not the game.
 */
export function Scoresheet({
  moves,
  currentPly,
  onSelectPly,
  label,
  startMoveNumber = 1,
  announce = true,
  className,
}: ScoresheetProps) {
  const reduced = useReducedMotion();
  const cursorId = `scoresheet-cursor-${useId()}`;
  const rows = useMemo(() => rowsOf(moves, startMoveNumber), [moves, startMoveNumber]);
  const scroller = useRef<HTMLDivElement>(null);
  const buttons = useRef(new Map<number, HTMLButtonElement>());
  const select = useRef(onSelectPly);
  select.current = onSelectPly;

  const onSelect = useCallback((ply: number) => select.current(ply), []);
  const register = useCallback((ply: number, el: HTMLButtonElement | null) => {
    if (el) buttons.current.set(ply, el);
    else buttons.current.delete(ply);
  }, []);

  // The roving tab stop: the move the arrows last focused, else the current move, else the first.
  const [focusPly, setFocusPly] = useState<number | null>(null);
  const plies = useMemo(() => moves.map((m) => m.ply), [moves]);
  const hasPly = (ply: number) => plies.includes(ply);
  const tabStopPly =
    focusPly !== null && hasPly(focusPly)
      ? focusPly
      : hasPly(currentPly)
        ? currentPly
        : (plies[0] ?? null);

  // When the ply changes while the old tab stop has focus, focus follows the current move.
  const lastPly = useRef(currentPly);
  useLayoutEffect(() => {
    if (lastPly.current === currentPly) return;
    const previous = buttons.current.get(focusPly ?? lastPly.current);
    lastPly.current = currentPly;
    setFocusPly(null);
    if (previous && document.activeElement === previous) {
      buttons.current.get(currentPly)?.focus();
    }
  }, [currentPly, focusPly]);

  const onKeyDown = (event: KeyboardEvent<HTMLOListElement>) => {
    if (event.ctrlKey || event.altKey || event.metaKey || event.shiftKey) return;
    const from = Number((event.target as HTMLElement).dataset.ply);
    const index = plies.indexOf(from);
    if (index < 0) return;
    const steps: Record<string, number> = {
      ArrowLeft: -1,
      ArrowRight: 1,
      ArrowUp: -2,
      ArrowDown: 2,
    };
    let next: number | undefined;
    if (event.key === "Home") next = 0;
    else if (event.key === "End") next = plies.length - 1;
    else if (steps[event.key] !== undefined) {
      next = Math.min(plies.length - 1, Math.max(0, index + (steps[event.key] ?? 0)));
    }
    if (next === undefined) return;
    // Prevented, so the review keys on window leave it alone.
    event.preventDefault();
    const ply = plies[next];
    if (ply === undefined) return;
    setFocusPly(ply);
    buttons.current.get(ply)?.focus();
  };

  // The current move in view, within the sheet only.
  const scrolled = useRef(false);
  const keepInView = useCallback(
    (animate: boolean) => {
      const box = scroller.current;
      const el = buttons.current.get(currentPly);
      if (!box || !el) return;
      const first = !scrolled.current;
      scrolled.current = true;
      // "Nearest", in the sheet's own box: element.scrollIntoView would scroll the page too.
      const top = el.offsetTop;
      const bottom = top + el.offsetHeight;
      let next: number | null = null;
      if (top < box.scrollTop) next = top;
      else if (bottom > box.scrollTop + box.clientHeight) next = bottom - box.clientHeight;
      if (next === null) return;
      if (animate && !first && !reduced && typeof box.scrollTo === "function") {
        box.scrollTo({ top: next, behavior: "smooth" });
      } else box.scrollTop = next;
    },
    [currentPly, reduced],
  );
  // biome-ignore lint/correctness/useExhaustiveDependencies: rows re-run it when the moves arrive
  useLayoutEffect(() => keepInView(true), [keepInView, rows]);
  useEffect(() => {
    const box = scroller.current;
    if (!box || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(() => keepInView(false));
    observer.observe(box);
    return () => observer.disconnect();
  }, [keepInView]);

  // The status line: empty on mount, then the move landed on, after the steps settle.
  const [message, setMessage] = useState("");
  const announced = useRef(currentPly);
  useEffect(() => {
    if (!announce || announced.current === currentPly) return;
    const timer = setTimeout(() => {
      announced.current = currentPly;
      if (currentPly === 0) {
        setMessage("Start position");
        return;
      }
      for (const row of rows) {
        const side = sideOf(row, currentPly);
        const move = side ? row[side] : null;
        if (move) {
          setMessage(moveText(move, row.number));
          return;
        }
      }
    }, ANNOUNCE_DELAY_MS);
    return () => clearTimeout(timer);
  }, [announce, currentPly, rows]);

  return (
    <section
      aria-label={label}
      data-surface="sheet"
      className={cn("flex min-h-0 flex-col rounded-sheet bg-sheet", className)}
    >
      <p role="status" aria-live="polite" aria-atomic="true" className="sr-only">
        {message}
      </p>
      {rows.length === 0 ? (
        <p className="m-(--space-panel-pad) rounded-field bg-well px-4 py-6 text-center text-sm text-ink-soft">
          No moves in this game
        </p>
      ) : (
        <>
          {/* Column heads, as on a paper scoresheet. The moves name their side already. */}
          <div
            aria-hidden="true"
            className={cn(
              "grid shrink-0 overflow-hidden border-b border-rule-strong px-2 pt-2 pb-1 [scrollbar-gutter:stable]",
              COLUMNS,
            )}
          >
            <span />
            <span className="px-2 font-sans text-sm font-medium text-ink-soft">White</span>
            <span className="px-2 font-sans text-sm font-medium text-ink-soft">Black</span>
          </div>
          <motion.div
            ref={scroller}
            layoutScroll
            data-scoresheet-scroll
            className="relative min-h-0 flex-1 overflow-y-auto overscroll-contain px-2 [scrollbar-gutter:stable]"
          >
            <ol className={cn("grid", COLUMNS)} onKeyDown={onKeyDown}>
              {rows.map((row) => (
                <RowView
                  key={row.number}
                  row={row}
                  current={sideOf(row, currentPly)}
                  tabStop={tabStopPly === null ? null : sideOf(row, tabStopPly)}
                  cursorId={cursorId}
                  reduced={reduced}
                  onSelect={onSelect}
                  register={register}
                />
              ))}
            </ol>
          </motion.div>
        </>
      )}
    </section>
  );
}

"use client";

import { Chess } from "chess.js";
import {
  type KeyboardEvent,
  lazy,
  type RefObject,
  Suspense,
  useEffect,
  useLayoutEffect,
  useMemo,
  useReducer,
  useRef,
  useState,
} from "react";
import type { Annotation } from "../annotations/index.js";
import { Board, type BoardMove } from "../board/board.js";
import { formatNotation, judgementGlyph, type NotationJudgement } from "../board/notation.js";
import type { Orientation, Square } from "../board/theme.js";
import {
  ArrowCounterClockwiseIcon,
  ArrowsClockwiseIcon,
  ArrowsOutSimpleIcon,
  CaretLeftIcon,
  CaretRightIcon,
  Icon,
  type PhosphorIcon,
  SkipBackIcon,
  SkipForwardIcon,
} from "../icons/index.js";
import { cn } from "../lib/index.js";
import {
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
  SegmentedControl,
} from "../primitives/index.js";
import { ArrowSwatch } from "../profile/arrow-swatch.js";
import { Scoresheet, type ScoresheetMove } from "../review/scoresheet.js";
import {
  derivedLine,
  type ExplorerAction,
  type ExplorerMode,
  type ExplorerMove,
  explorerReducer,
  initialExplorerState,
  lineMoves,
  moveFrom,
  type PositionLine,
  positionAt,
  tryMatch,
} from "./model.js";

// The position explorer (feat/position-explorer): one cited position, compact by default (the
// small board with the move played and the engine's better move, and its key), expanded into a
// dialog where the player steps through the game around it, compares the engine's stored line,
// and tries moves of their own. The expanded data (the game around the ply and the stored engine
// line) is fetched on first expand through `loadLine`; until it arrives the dialog already shows
// the cited position, the move played and the engine's move, so nothing waits on the network.
// Moves the player tries are never given an evaluation here: no engine checks them (honesty).

const CompactBoard = lazy(() => import("../board/board.js").then((m) => ({ default: m.Board })));

export interface PositionExplorerProps {
  /** The game, as `loadLine` knows it (the review's game id in the web app). */
  gameId: string;
  /** The ply of the cited move (1 = White's first move). */
  ply: number;
  /** The position before the cited move. */
  fenBefore: string;
  played: { san: string; uci: string };
  /** The engine's better move, or null when it is the move played or unknown. */
  best: { san: string; uci: string } | null;
  /** The player's side: the board starts from it. */
  orientation: Orientation;
  /** The cited move as notation, "28. Nd2" or "28… Nd2". */
  label: string;
  /** The player's own mistake or blunder: the move played is drawn in pen, else in pencil. */
  mistake?: boolean;
  /**
   * The stored judgement of the move played. With `mistake`, the pen arrow carries its `?`-family
   * badge and the key names the glyph ("Played, Nd2??"): a pen mark never relies on colour alone.
   */
  judgement?: NotationJudgement | null;
  /**
   * The move played was the engine's own (a strength): it is drawn as a `good` arrow with a `!`
   * badge, and the key says so. Wins over `mistake`.
   */
  found?: boolean;
  /** The full game at that ply (the review); without one there is no link to it. */
  href?: string | undefined;
  /** Fetches the expanded data on first expand; null or a throw keeps the cited moves only. */
  loadLine?: (gameId: string, ply: number) => Promise<PositionLine | null>;
  /** The expanded data, when the host already has it (no fetch). */
  line?: PositionLine;
  className?: string;
}

const SQUARE = /^[a-h][1-8]$/;

function squares(uci: string | null | undefined): { from: Square; to: Square } | null {
  if (!uci) return null;
  const from = uci.slice(0, 2);
  const to = uci.slice(2, 4);
  if (!SQUARE.test(from) || !SQUARE.test(to) || from === to) return null;
  return { from: from as Square, to: to as Square };
}

type Cited = Pick<PositionExplorerProps, "played" | "best" | "mistake" | "found" | "judgement">;

/** The glyph of the player's own judged mistake, or "" (a found move or an unjudged one). */
const mistakeGlyph = (p: Cited) =>
  p.mistake && !p.found && p.judgement ? judgementGlyph[p.judgement] : "";

const betterMove = (p: Cited) => (p.best && p.best.uci !== p.played.uci ? p.best : null);

/** The cited position's marks: the move played and, when it differs, the engine's better move. */
export function explorerAnnotations(p: Cited, show: "both" | "best" = "both"): Annotation[] {
  const out: Annotation[] = [];
  const best = squares(betterMove(p)?.uci);
  const played = show === "both" ? squares(p.played.uci) : null;
  if (best) out.push({ kind: "arrow", ...best, tone: "good" });
  if (played && p.found)
    out.push(
      { kind: "arrow", ...played, tone: "good" },
      { kind: "mark", square: played.to, glyph: "!", tone: "good" },
    );
  else if (played) {
    out.push(
      p.mistake
        ? { kind: "arrow", ...played, tone: "pen", toEdge: true }
        : { kind: "arrow", ...played, tone: "pencil" },
    );
    const glyph = mistakeGlyph(p);
    if (glyph) out.push({ kind: "mark", square: played.to, glyph, tone: "pen" });
  }
  return out;
}

/** "Position before 30. Rxg7; engine preferred Qe3". */
export function explorerLabel(p: Cited & { label: string }): string {
  const best = betterMove(p);
  return `Position before ${p.label}${best ? `; engine preferred ${best.san}` : ""}`;
}

/** True once `ref` comes within a screen of the viewport (at once without IntersectionObserver). */
function useNear(ref: RefObject<HTMLElement | null>): boolean {
  const [near, setNear] = useState(false);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    if (typeof IntersectionObserver === "undefined") {
      setNear(true);
      return;
    }
    const io = new IntersectionObserver(
      (entries) => {
        if (entries.some((e) => e.isIntersecting)) {
          setNear(true);
          io.disconnect();
        }
      },
      { rootMargin: "100% 0px" },
    );
    io.observe(el);
    return () => io.disconnect();
  }, [ref]);
  return near;
}

const linkClass = cn(
  "inline-flex min-h-11 w-fit items-center font-sans text-sm text-ink underline decoration-rule-strong underline-offset-4",
  "transition-colors duration-(--dur-quick) ease-out hover:decoration-ink",
  "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink",
);

type Load = "idle" | "loading" | "done" | "failed";

/**
 * A cited position: the compact board with its key, an Expand button (the board opens it too)
 * and a link to the game; expanded, a dialog to step, compare and try moves.
 */
export function PositionExplorer(props: PositionExplorerProps) {
  const { gameId, ply, fenBefore, played, orientation, label, href, loadLine, className } = props;
  const ref = useRef<HTMLDivElement>(null);
  const near = useNear(ref);
  const [open, setOpen] = useState(false);
  const [loaded, setLoaded] = useState<PositionLine | null>(props.line ?? null);
  const [load, setLoad] = useState<Load>(props.line ? "done" : "idle");
  const best = betterMove(props);
  const boardLabel = explorerLabel(props);

  // The expanded data, fetched once, on the first expand. The fetch outlives a closed dialog, so
  // a reopen finds it done; only an unmount drops its answer.
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  useEffect(() => {
    if (!open || load !== "idle") return;
    if (!loadLine) {
      setLoad("done");
      return;
    }
    setLoad("loading");
    loadLine(gameId, ply).then(
      (l) => {
        if (!mounted.current) return;
        if (l) setLoaded(l);
        setLoad(l ? "done" : "failed");
      },
      () => {
        if (mounted.current) setLoad("failed");
      },
    );
  }, [open, load, loadLine, gameId, ply]);
  // A host that passes the line later (or a new one) wins over the fetch.
  useEffect(() => {
    if (props.line) {
      setLoaded(props.line);
      setLoad("done");
    }
  }, [props.line]);

  const placeholder = (
    <div aria-hidden="true" className="size-(--board-size) rounded-board bg-well" />
  );
  return (
    <figure
      data-cited-position
      data-position-explorer
      className={cn("m-0 flex w-48 shrink-0 flex-col gap-2 md:w-52", className)}
    >
      <div
        ref={ref}
        // ban-lint-allow TOK2: a reference board fills its small column in whole-pixel squares
        className="@container relative w-full [--board-size:round(down,100cqi,8px)] md:[--board-size:round(down,100cqi_-_16px,8px)]"
      >
        {near ? (
          <Suspense fallback={placeholder}>
            <CompactBoard
              fen={fenBefore}
              label={boardLabel}
              orientation={orientation}
              annotations={explorerAnnotations(props)}
            />
          </Suspense>
        ) : (
          placeholder
        )}
        {/* The board opens the explorer for a pointer; the Expand button is the one tab stop. */}
        <button
          type="button"
          data-explorer-open
          tabIndex={-1}
          aria-hidden="true"
          onClick={() => setOpen(true)}
          className="absolute inset-0 z-30 cursor-zoom-in rounded-board"
        />
      </div>
      <figcaption className="flex flex-col gap-1">
        <ul
          aria-label="Arrows"
          className="m-0 flex list-none flex-wrap gap-x-4 gap-y-1 p-0 font-sans text-sm text-ink-soft"
        >
          <li className="flex items-center gap-1.5">
            <ArrowSwatch tone={props.found ? "good" : props.mistake ? "pen" : "pencil"} />
            Played, {played.san}
            {mistakeGlyph(props) ? (
              <span className="tracking-(--tracking-glyph) text-pen-text">{mistakeGlyph(props)}</span>
            ) : null}
            {props.found ? ", the engine's move" : null}
          </li>
          {best ? (
            <li className="flex items-center gap-1.5">
              <ArrowSwatch tone="good" />
              Better, {best.san}
            </li>
          ) : null}
        </ul>
        <div className="flex flex-wrap items-center gap-x-4">
          <Button
            variant="outline"
            size="sm"
            className="mt-1"
            aria-label={`Expand the position before ${label}`}
            aria-haspopup="dialog"
            onClick={() => setOpen(true)}
          >
            <Icon icon={ArrowsOutSimpleIcon} size={16} />
            Expand
          </Button>
          {href ? (
            <a href={href} className={linkClass}>
              Open the game at {label}
            </a>
          ) : null}
        </div>
      </figcaption>
      <Dialog open={open} onOpenChange={setOpen}>
        {open ? (
          <ExplorerDialog
            {...props}
            line={loaded ?? undefined}
            load={load}
            boardLabel={boardLabel}
          />
        ) : null}
      </Dialog>
    </figure>
  );
}

/** "28. Nd2" or "28… Nd2". */
const moveText = (m: ExplorerMove) =>
  formatNotation({ moveNumber: Math.ceil(m.ply / 2), color: m.color, san: m.san });

/** The side-to-move king's square when it is in check. */
function checkSquare(fen: string): Square | undefined {
  try {
    const chess = new Chess(fen);
    if (!chess.inCheck()) return undefined;
    const turn = chess.turn();
    for (const row of chess.board()) {
      for (const cell of row) {
        if (cell && cell.type === "k" && cell.color === turn) return cell.square as Square;
      }
    }
  } catch {
    return undefined;
  }
  return undefined;
}

const LINES: { value: Exclude<ExplorerMode, "try">; label: string }[] = [
  { value: "game", label: "What you played" },
  { value: "engine", label: "Engine line" },
];

interface StepProps {
  label: string;
  icon: PhosphorIcon;
  keys: string;
  disabled?: boolean;
  onClick: () => void;
}

function Step({ label, icon, keys, disabled, onClick }: StepProps) {
  return (
    <Button
      variant="quiet"
      size="md"
      aria-label={label}
      aria-keyshortcuts={keys}
      disabled={disabled}
      onClick={onClick}
      className="w-11 px-0"
    >
      <Icon icon={icon} />
    </Button>
  );
}

// ban-lint-allow TOK2: the dialog keeps a gutter round it at every height and width
const DIALOG_SIZE = "max-h-[calc(100dvh-2*var(--space-gutter))] max-w-5xl overflow-y-auto";
// The board fills its column in whole-pixel squares and leaves room for the controls below it;
// from 768px the well's 16px comes off.
// ban-lint-allow TOK2: a whole-pixel board capped by the viewport height
const BOARD_SIZE = "[--board-size:round(down,min(100cqi,100dvh_-_--spacing(64)),8px)]";
// ban-lint-allow TOK2: the same, less the well's padding
const BOARD_SIZE_MD = "md:[--board-size:round(down,min(100cqi_-_16px,100dvh_-_--spacing(64)),8px)]";

function ExplorerDialog({
  ply,
  fenBefore,
  played,
  best,
  orientation,
  label,
  mistake,
  judgement,
  found,
  href,
  line,
  load,
  boardLabel,
}: PositionExplorerProps & { load: Load; boardLabel: string }) {
  // The first line only (the derived one until the fetch answers); a loaded line arrives through
  // the reducer.
  const [state, dispatch] = useReducer(explorerReducer, null, () =>
    initialExplorerState(line ?? derivedLine({ ply, fenBefore, played, best }), ply),
  );
  useEffect(() => {
    if (line && line !== state.line) dispatch({ type: "loaded", line });
  }, [line, state.line]);
  const [side, setSide] = useState<Orientation>(orientation);

  const moves = useMemo(() => lineMoves(state), [state]);
  const fen = positionAt(moves, state.cursor, fenBefore);
  const last = state.cursor > 0 ? moves[state.cursor - 1] : undefined;
  const lastMove = squares(last?.uci) ?? undefined;
  const atCited = moves[state.cursor]?.ply === ply && moves[state.cursor]?.fenBefore === fenBefore;
  const annotations =
    atCited && state.mode === "game"
      ? explorerAnnotations({ played, best, mistake, found, judgement })
      : atCited && state.mode === "engine"
        ? explorerAnnotations({ played, best, mistake, found, judgement }, "best")
        : [];

  const sheetMoves = useMemo<ScoresheetMove[]>(
    () =>
      moves.map((m: ExplorerMove) => ({
        ply: m.ply,
        color: m.color,
        san: m.san,
        // The cited move keeps its stored glyph on the sheet; tried and engine moves have none.
        judgement:
          mistake && !found && m.ply === ply && m.uci === played.uci ? (judgement ?? null) : null,
        best: false,
        mine: m.color === orientation,
      })),
    [moves, orientation, mistake, found, ply, played.uci, judgement],
  );
  const firstMove = moves[0];
  const startMoveNumber = firstMove ? Math.ceil(firstMove.ply / 2) : Math.ceil(ply / 2);
  const currentPly = last?.ply ?? (firstMove ? firstMove.ply - 1 : ply - 1);

  const onMove = (m: BoardMove): boolean => {
    const uci = `${m.from}${m.to}${m.promotion ?? ""}`;
    const nextPly = last ? last.ply + 1 : (firstMove?.ply ?? ply);
    const move = moveFrom(fen, uci, nextPly);
    if (!move) return false;
    dispatch({ type: "play", move });
    return true;
  };

  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    if (e.defaultPrevented || e.ctrlKey || e.metaKey || e.altKey) return;
    const t = e.target as HTMLElement;
    if (t.closest("[data-square-button],[role='radiogroup'],input,textarea,select")) return;
    const action: ExplorerAction | "flip" | null =
      e.key === "ArrowLeft"
        ? { type: "step", by: -1 }
        : e.key === "ArrowRight"
          ? { type: "step", by: 1 }
          : e.key === "Home"
            ? { type: "start" }
            : e.key === "End"
              ? { type: "end" }
              : e.key === "f" || e.key === "F"
                ? "flip"
                : null;
    if (!action) return;
    e.preventDefault();
    if (action === "flip") setSide((s) => (s === "white" ? "black" : "white"));
    else dispatch(action);
  };

  // A step button that the step disabled keeps focus in the controls: it moves to the first
  // button still enabled, never to the page.
  const controls = useRef<HTMLDivElement>(null);
  const focused = useRef<HTMLButtonElement | null>(null);
  useLayoutEffect(() => {
    const el = focused.current;
    if (!el?.disabled) return;
    const active = document.activeElement;
    if (active !== null && active !== document.body && active !== el) return;
    controls.current?.querySelector<HTMLButtonElement>("button:not(:disabled)")?.focus();
  });

  const match = tryMatch(state);
  const note =
    state.mode === "game"
      ? `The game as it went. ${played.san} was played from the marked position.`
      : state.mode === "engine"
        ? state.line.engine.length > 0
          ? "The engine's line from the marked position, as stored with the analysis."
          : "No engine line is stored for this move."
        : state.tried.length === 0
          ? "Drag or click a piece to try a move from this position."
          : match === "engine"
            ? "That is the engine's move."
            : match === "game"
              ? "That is the move played in the game."
              : `You tried ${state.tried[0] ? moveText(state.tried[0]) : ""}.`;

  return (
    <DialogContent className={cn(DIALOG_SIZE, "gap-5")} onKeyDown={onKeyDown}>
      <div className="flex flex-col gap-1 pr-10">
        <DialogTitle>Position before {label}</DialogTitle>
        <DialogDescription className="text-sm">
          Step through the game, compare the engine's line, or try your own moves.
        </DialogDescription>
      </div>
      <div className="grid min-w-0 gap-5 lg:grid-cols-[minmax(0,1fr)_--spacing(76)] lg:gap-6">
        <div className="flex min-w-0 flex-col gap-3">
          <div className={cn("@container w-full", BOARD_SIZE, BOARD_SIZE_MD)}>
            <Board
              fen={fen}
              label={atCited || !last ? boardLabel : `Position after ${moveText(last)}`}
              orientation={side}
              lastMove={lastMove}
              check={checkSquare(fen)}
              annotations={annotations}
              interactive
              onMove={onMove}
            />
          </div>
          {/* biome-ignore lint/a11y/useSemanticElements: a group of controls, not a fieldset */}
          <div
            ref={controls}
            role="group"
            aria-label="Move controls"
            onFocus={(e) => {
              focused.current = e.target instanceof HTMLButtonElement ? e.target : null;
            }}
            className="flex flex-wrap items-center justify-between gap-1"
          >
            <div className="flex items-center gap-1">
              <Step
                label="First move"
                icon={SkipBackIcon}
                keys="Home"
                disabled={state.cursor === 0}
                onClick={() => dispatch({ type: "start" })}
              />
              <Step
                label="Previous move"
                icon={CaretLeftIcon}
                keys="ArrowLeft"
                disabled={state.cursor === 0}
                onClick={() => dispatch({ type: "step", by: -1 })}
              />
              <Step
                label="Next move"
                icon={CaretRightIcon}
                keys="ArrowRight"
                disabled={state.cursor >= moves.length}
                onClick={() => dispatch({ type: "step", by: 1 })}
              />
              <Step
                label="Last move"
                icon={SkipForwardIcon}
                keys="End"
                disabled={state.cursor >= moves.length}
                onClick={() => dispatch({ type: "end" })}
              />
            </div>
            <Step
              label="Flip board"
              icon={ArrowsClockwiseIcon}
              keys="F"
              onClick={() => setSide((s) => (s === "white" ? "black" : "white"))}
            />
          </div>
        </div>
        <div className="flex min-w-0 flex-col gap-4">
          <div className="flex flex-wrap items-end justify-between gap-x-3 gap-y-3">
            {/* In try mode no line is chosen: picking one leaves the tried moves. */}
            <SegmentedControl
              label="Line"
              value={state.mode === "try" ? "" : state.mode}
              options={LINES}
              onValueChange={(v) => dispatch({ type: "mode", mode: v as ExplorerMode })}
            />
            <Button
              variant={state.mode === "try" ? "primary" : "outline"}
              size="md"
              aria-pressed={state.mode === "try"}
              onClick={() =>
                dispatch({ type: "mode", mode: state.mode === "try" ? state.tryBase : "try" })
              }
            >
              Try a move
            </Button>
          </div>
          {/* Tall enough for the longest note at 390px, so a mode switch moves nothing below. */}
          <div
            aria-live="polite"
            className="flex min-h-28 flex-col gap-1 font-sans text-sm lg:min-h-20"
          >
            <p className="m-0 text-ink">{note}</p>
            {state.mode === "try" ? (
              <p className="m-0 text-ink-soft">
                Moves you try are not checked by the engine. The game review shows the engine's move
                at every point of the game.
              </p>
            ) : null}
            {load === "failed" ? (
              <p className="m-0 text-ink-soft">
                Couldn't load the rest of the game. Open the full game to see it.
              </p>
            ) : null}
          </div>
          <Scoresheet
            moves={sheetMoves}
            currentPly={currentPly}
            onSelectPly={(p) => dispatch({ type: "goto", ply: p })}
            label={`Moves, ${state.mode === "try" ? "your moves" : (LINES.find((m) => m.value === state.mode)?.label ?? "")}`}
            startMoveNumber={startMoveNumber}
            className="h-56 lg:h-72"
          />
          <div className="flex flex-wrap items-center justify-between gap-x-4 gap-y-1">
            {href ? (
              <a href={href} className={linkClass}>
                Open full game
              </a>
            ) : (
              <span />
            )}
            {state.mode === "try" ? (
              <Button
                variant="outline"
                size="sm"
                disabled={state.tried.length === 0}
                onClick={() => dispatch({ type: "reset" })}
              >
                <Icon icon={ArrowCounterClockwiseIcon} size={16} />
                Reset
              </Button>
            ) : null}
          </div>
        </div>
      </div>
      <span aria-busy={load === "loading"} className="sr-only">
        {load === "loading" ? "Loading the game around this move" : ""}
      </span>
    </DialogContent>
  );
}

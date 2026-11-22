"use client";

import { Chess, type Square as ChessSquare } from "chess.js";
import { useEffect, useId, useLayoutEffect, useMemo, useRef, useState } from "react";
import { Chessboard } from "react-chessboard";

import { type Annotation, AnnotationLayer } from "../annotations/index.js";
import { cn } from "../lib/index.js";
import { type PieceType, useReducedMotion } from "../motion/index.js";
import { boardPieces, PieceMotionContext, type PiecePlan } from "./piece.js";
import type { PieceCode } from "./piece-types.js";
import { SquareGrid } from "./square-grid.js";
import {
  boardTheme,
  FILES,
  type Orientation,
  RANKS,
  type Square,
  squareStyles,
  squareToCell,
} from "./theme.js";

/** A board mark: an arrow, a ring or a glyph badge (`@ci/ui/annotations`). */
export type BoardAnnotation = Annotation;

export interface BoardMove {
  from: Square;
  to: Square;
  promotion?: "q" | "r" | "b" | "n";
}

export interface BoardProps {
  fen: string;
  /** the board's accessible name, such as "Position after 28. Nd2" */
  label: string;
  orientation?: Orientation;
  lastMove?: { from: Square; to: Square };
  check?: Square;
  annotations?: readonly BoardAnnotation[];
  /** Draw the marks on when they change (default true); reduced motion always draws them at once. */
  animateAnnotations?: boolean;
  interactive?: boolean;
  /** Called for a legal move; return false to refuse it. */
  onMove?: (m: BoardMove) => boolean;
  className?: string;
}

const TYPE: Record<string, PieceType> = {
  p: "pawn",
  n: "knight",
  b: "bishop",
  r: "rook",
  q: "queen",
  k: "king",
};

type Placement = Map<Square, PieceCode>;

/** Piece placement from a FEN's first field. */
function placement(fen: string): Placement {
  const map: Placement = new Map();
  const rows = (fen.split(" ")[0] ?? "").split("/");
  rows.forEach((row, i) => {
    let file = 0;
    for (const ch of row) {
      if (/\d/.test(ch)) file += Number(ch);
      else {
        const colour = ch === ch.toUpperCase() ? "w" : "b";
        map.set(`${FILES[file]}${8 - i}` as Square, `${colour}${ch.toUpperCase()}` as PieceCode);
        file += 1;
      }
    }
  });
  return map;
}

/** FNV-1a 32-bit into one mulberry32 draw in [0, 1), so a capture spins the same way every time. */
function seeded(key: string): number {
  let h = 2166136261;
  for (let i = 0; i < key.length; i++) h = Math.imul(h ^ key.charCodeAt(i), 16777619);
  let t = (h + 0x6d2b79f5) | 0;
  t = Math.imul(t ^ (t >>> 15), t | 1);
  t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
  return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
}

let nextPlanId = 1;

/** Works out which pieces travel between two positions, given the move that joined them. */
function planMoves(
  before: Placement,
  after: Placement,
  move: { from: Square; to: Square },
  orientation: Orientation,
  dropped: boolean,
  inflight: ReadonlyMap<Square, () => { dx: number; dy: number }>,
): Map<Square, PiecePlan> {
  const plans = new Map<Square, PiecePlan>();
  const mover = before.get(move.from);
  const landed = after.get(move.to);
  if (!mover || !landed || mover[0] !== landed[0] || after.get(move.from)) return plans;
  const offset = (from: Square, to: Square) => {
    const a = squareToCell(from, orientation);
    const b = squareToCell(to, orientation);
    return { dx: a.col - b.col, dy: a.row - b.row };
  };
  // A mover still in the air travels on from where it is drawn now (M7), already lifted.
  const air = inflight.get(move.from)?.();
  const base = offset(move.from, move.to);
  const dx = base.dx + (air?.dx ?? 0);
  const dy = base.dy + (air?.dy ?? 0);
  let captured: PiecePlan["captured"];
  const victimSquare =
    before.get(move.to) !== undefined
      ? move.to
      : mover[1] === "P" && move.from[0] !== move.to[0]
        ? (`${move.to[0]}${move.from[1]}` as Square) // en passant
        : null;
  const victim = victimSquare ? before.get(victimSquare) : undefined;
  if (victimSquare && victim && victim[0] !== mover[0]) {
    const rand = seeded(`capture:${move.from}${move.to}`);
    const sign = -dx !== 0 ? Math.sign(-dx) : rand < 0.5 ? -1 : 1;
    captured = { code: victim, ...offset(victimSquare, move.to), spin: sign * (12 + 8 * rand) };
  }
  const type = TYPE[(mover[1] ?? "p").toLowerCase()] ?? "pawn";
  plans.set(move.to, {
    id: nextPlanId++,
    code: landed,
    type,
    dx,
    dy,
    delay: 0,
    dropped,
    flying: air !== undefined,
    captured,
  });
  // Castling: the rook follows the king by 70ms, so the king lands first (DESIGN.md, "Sound").
  if (mover[1] === "K" && Math.abs(move.to.charCodeAt(0) - move.from.charCodeAt(0)) === 2) {
    const rank = move.from[1];
    const kingside = move.to[0] === "g";
    const rookFrom = `${kingside ? "h" : "a"}${rank}` as Square;
    const rookTo = `${kingside ? "f" : "d"}${rank}` as Square;
    const rook = after.get(rookTo);
    if (rook && before.get(rookFrom) === rook) {
      plans.set(rookTo, {
        id: nextPlanId++,
        code: rook,
        type: "rook",
        ...offset(rookFrom, rookTo),
        delay: 70,
        dropped: false,
      });
    }
  }
  return plans;
}

const NAME: Record<string, string> = {
  K: "king",
  Q: "queen",
  R: "rook",
  B: "bishop",
  N: "knight",
  P: "pawn",
};

/** True when `move` played on `before` gives the placement of `after`. */
function playsTo(before: string, move: BoardMove, after: string): boolean {
  const chess = load(before);
  if (!chess) return false;
  try {
    chess.move(move);
  } catch {
    return false;
  }
  return chess.fen().split(" ")[0] === after.split(" ")[0];
}

function load(fen: string): Chess | null {
  try {
    return new Chess(fen);
  } catch {
    return null;
  }
}

/**
 * The board (DESIGN.md, "The board"; variant c "Ink well"): react-chessboard with the shipped
 * Cburnett set, flat token squares, coordinates on the squares, and from 768px an 8px `well`
 * around it. Pieces move with weight (per-piece `settle`, lift, land, knights arc, captures
 * knocked off). Keyboard access is the `SquareGrid` over the board.
 */
export function Board({
  fen,
  label,
  orientation = "white",
  lastMove,
  check,
  annotations,
  animateAnnotations = true,
  interactive = false,
  onMove,
  className,
}: BoardProps) {
  const id = `board-${useId().replace(/[^a-zA-Z0-9]/g, "")}`;
  const reduced = useReducedMotion();
  const chess = useMemo(() => load(fen), [fen]);
  const position = useMemo(() => placement(fen), [fen]);
  const occupied = useMemo(() => new Set<string>(position.keys()), [position]);
  const [selected, setSelected] = useState<Square | null>(null);
  const [focus, setFocus] = useState<Square | null>(null);
  const played = useRef(new Set<number>()).current;
  const inflight = useRef(new Map<Square, () => { dx: number; dy: number }>()).current;
  const pending = useRef<{ move: BoardMove; dropped: boolean } | null>(null);

  // Plan the motion in the render that brings the new position, so the moved piece's first frame
  // is already at its origin. A re-render with the same FEN keeps the plan.
  const motion = useRef<{ fen: string; placement: Placement; plans: Map<Square, PiecePlan> }>({
    fen,
    placement: position,
    plans: new Map(),
  });
  if (motion.current.fen !== fen) {
    // Our own move counts only if this FEN is that move played on the previous one; a parent that
    // jumps somewhere else falls back to `lastMove`.
    const own = pending.current;
    const ownFits = own !== null && playsTo(motion.current.fen, own.move, fen);
    const move = ownFits ? own : lastMove ? { move: lastMove, dropped: false } : null;
    const plans = move
      ? planMoves(
          motion.current.placement,
          position,
          move.move,
          orientation,
          move.dropped,
          inflight,
        )
      : new Map<Square, PiecePlan>();
    motion.current = { fen, placement: position, plans };
    pending.current = null;
  }
  // A pending move lives for one render: whatever the parent did with it, it is spent.
  useEffect(() => {
    pending.current = null;
  });
  // biome-ignore lint/correctness/useExhaustiveDependencies: the plans change with the FEN
  const motionState = useMemo(
    () => ({ plans: motion.current.plans, reduced, played, inflight }),
    [fen, reduced, played, inflight],
  );

  const turn = chess?.turn() ?? "w";
  const canPlay = interactive && chess !== null;

  const targets = useMemo(() => {
    const map = new Map<Square, boolean>();
    if (!selected || !chess || !canPlay) return map;
    for (const m of chess.moves({ square: selected as ChessSquare, verbose: true })) {
      map.set(m.to as Square, m.isCapture() || m.isEnPassant());
    }
    return map;
  }, [selected, chess, canPlay]);

  const names = useMemo(() => {
    const map = new Map<Square, string>();
    for (const file of FILES) {
      for (const rank of RANKS) {
        const square = `${file}${rank}` as Square;
        const code = position.get(square);
        const piece = code
          ? `${code[0] === "w" ? "white" : "black"} ${NAME[code[1] ?? "P"]}`
          : "empty";
        const extras = [
          square === check ? "check" : null,
          targets.has(square) ? "legal move" : null,
        ]
          .filter(Boolean)
          .join(", ");
        map.set(square, `${square}, ${piece}${extras ? `, ${extras}` : ""}`);
      }
    }
    return map;
  }, [position, check, targets]);

  // The tab stop: the last move's destination, else the king of the side to move.
  const fallbackFocus = useMemo<Square>(() => {
    if (lastMove) return lastMove.to;
    for (const [square, code] of position) if (code === `${turn}K`) return square;
    return orientation === "white" ? "e1" : "e8";
  }, [lastMove, position, turn, orientation]);

  const tryMove = (from: Square, to: Square, dropped: boolean): boolean => {
    if (!chess || !canPlay) return false;
    const legal = chess
      .moves({ square: from as ChessSquare, verbose: true })
      .find((m) => m.to === to);
    if (!legal) return false;
    const move: BoardMove = legal.promotion
      ? { from, to, promotion: "q" } // a promotion picker arrives with the drill UI; queen for now
      : { from, to };
    pending.current = { move, dropped };
    const accepted = onMove?.(move) ?? false;
    if (!accepted) pending.current = null;
    setSelected(null);
    return accepted;
  };

  const ownPiece = (square: Square) => position.get(square)?.[0] === turn;

  const activate = (square: Square) => {
    if (!canPlay) return;
    if (selected === square) {
      setSelected(null);
      return;
    }
    if (selected && targets.has(square)) {
      tryMove(selected, square, false);
      return;
    }
    setSelected(ownPiece(square) ? square : null);
  };

  // dnd-kit makes every draggable piece a tab stop (role="button", tabindex 0). The SquareGrid is
  // the board's keyboard interface, so the pieces underneath leave the tab order. The server
  // markup still has them, so the surface is inert until this has run: an aria-hidden layer must
  // never hold a tab stop (axe aria-hidden-focus, M4 T28).
  const surface = useRef<HTMLDivElement>(null);
  const [untabbed, setUntabbed] = useState(false);
  useLayoutEffect(() => {
    const root = surface.current;
    if (!root) return;
    const untab = () => {
      for (const el of root.querySelectorAll('[tabindex]:not([tabindex="-1"])')) {
        el.setAttribute("tabindex", "-1");
      }
    };
    untab();
    setUntabbed(true);
    const observer = new MutationObserver(untab);
    observer.observe(root, { subtree: true, childList: true, attributeFilter: ["tabindex"] });
    return () => observer.disconnect();
  }, []);

  // Per-square coordinate rules (DESIGN.md, "Board coordinates"); react-chessboard styles
  // coordinates per square colour only, so these come from a scoped rule:
  // - on a tinted night square the pale tint drops `ink` below 3:1, so the coordinate turns `on-ink`;
  // - on the check square the 3px `ink` ring runs under the glyph, so the glyph is `ink` on a 2px
  //   `sheet` halo (15.28 and 13.32:1) and stays readable over the ring. Emitted last, so it wins.
  const coordinateCss = useMemo(() => {
    const tinted = [lastMove?.from, lastMove?.to, selected ?? undefined].filter(Boolean);
    const rules = tinted.map(
      (sq) =>
        `[data-theme="dark"] #${id}-square-${sq}{--board-coord-light:var(--color-on-ink);--board-coord-dark:var(--color-on-ink)}`,
    );
    if (check) {
      rules.push(
        `[data-board] #${id}-square-${check}{--board-coord-light:var(--color-ink);--board-coord-dark:var(--color-ink)}`,
        `[data-board] #${id}-square-${check}>span{-webkit-text-stroke:4px var(--color-sheet);paint-order:stroke fill}`,
      );
    }
    return rules.join("");
  }, [id, lastMove, selected, check]);

  const styles = useMemo(
    () => squareStyles({ lastMove, check, selected }),
    [lastMove, check, selected],
  );

  return (
    <div
      data-board
      data-orientation={orientation}
      className={cn(
        "w-fit md:rounded-(--radius-board) md:bg-well md:p-2",
        "[--board-coord-light:var(--color-ink)] dark:[--board-coord-light:var(--color-on-ink)]",
        "[--board-coord-dark:var(--color-ink)]",
        // PIECE_SCALE (theme.ts), written out so Tailwind sees the classes.
        "[--piece-scale:0.84] md:[--piece-scale:0.88]",
        className,
      )}
    >
      {coordinateCss ? <style data-board-coordinates>{coordinateCss}</style> : null}
      <div className="relative size-(--board-size)">
        <div ref={surface} aria-hidden="true" inert={!untabbed} className="size-full">
          <PieceMotionContext.Provider value={motionState}>
            <Chessboard
              options={{
                id,
                position: fen,
                boardOrientation: orientation,
                pieces: boardPieces,
                showAnimations: false,
                allowDrawingArrows: false,
                allowDragging: canPlay,
                canDragPiece: ({ square }) =>
                  canPlay && square !== null && ownPiece(square as Square),
                onPieceDrop: ({ sourceSquare, targetSquare }) =>
                  targetSquare
                    ? tryMove(sourceSquare as Square, targetSquare as Square, true)
                    : false,
                onSquareClick: ({ square }) => {
                  setFocus(square as Square);
                  activate(square as Square);
                },
                squareStyles: styles,
                ...boardTheme,
              }}
            />
          </PieceMotionContext.Provider>
        </div>
        {annotations && annotations.length > 0 ? (
          <AnnotationLayer
            annotations={annotations}
            orientation={orientation}
            animate={animateAnnotations}
            occupied={occupied}
          />
        ) : null}
        <SquareGrid
          label={label}
          orientation={orientation}
          names={names}
          focus={focus ?? fallbackFocus}
          onFocusChange={setFocus}
          selected={selected}
          targets={targets}
          check={check}
          onActivate={activate}
          onCancel={() => setSelected(null)}
        />
      </div>
    </div>
  );
}

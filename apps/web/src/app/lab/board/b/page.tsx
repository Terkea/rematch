"use client";

import { type Annotation, AnnotationLayer } from "@ci/ui/annotations";
import type { Square } from "@ci/ui/board";
import { games } from "@ci/ui/fixtures";
import { Children, cloneElement, isValidElement, type ReactElement, type ReactNode } from "react";
import { Chessboard, defaultPieces, type PieceRenderObject } from "react-chessboard";
import { LabFrame } from "../../lab-frame";

// Variant b, "Book hatching": dark squares as diagonal ink hatching over board-light,
// coordinates in Literata on all four sides, and a broad, constant 0.11-square marker line.
// Lab only: Task 11 builds the real wrapper and packages/ui/src/annotations the real geometry.

const game = games.middlegameBlunder;
const found = game.plies.find((p) => p.judgement === "blunder");
if (!found) throw new Error("middlegameBlunder has no blunder");
const blunder = found;
const from = blunder.uci.slice(0, 2) as Square;
const to = blunder.uci.slice(2, 4) as Square;

// The marks are the product's red pen (`AnnotationLayer`, Task 12): this lab compares the
// board treatments (squares, frame, coordinates); the stroke comparison is recorded in
// packages/ui/design/decisions/board.md.
const marks: Annotation[] = [
  { kind: "circle", square: to, tone: "pen" },
  { kind: "arrow", from, to, tone: "pen" },
  { kind: "mark", square: to, glyph: "??", tone: "pen" },
];

// Cburnett paths (react-chessboard's default set, identical to the Commons files) recoloured to
// the piece tokens. Lab only: the product recolours at build time into sprites (DESIGN.md).
function recolour(node: ReactNode): ReactNode {
  if (!isValidElement(node)) return node;
  const el = node as ReactElement<{ style?: Record<string, unknown>; children?: ReactNode }>;
  const map = (v: unknown) =>
    v === "#000000" ? "var(--color-piece-dark)" : v === "#ffffff" ? "var(--color-piece-light)" : v;
  const style = el.props.style
    ? Object.fromEntries(Object.entries(el.props.style).map(([k, v]) => [k, map(v)]))
    : undefined;
  const kids = el.props.children;
  return cloneElement(el, {
    ...(style ? { style } : {}),
    ...(kids === undefined
      ? {}
      : { children: Array.isArray(kids) ? Children.toArray(kids).map(recolour) : recolour(kids) }),
  });
}

const pieces: PieceRenderObject = Object.fromEntries(
  Object.entries(defaultPieces).map(([key, render]) => [
    key,
    (props?: Parameters<typeof render>[0]) => recolour(render(props)) as ReactElement,
  ]),
);

const files = ["a", "b", "c", "d", "e", "f", "g", "h"];
const ranks = ["8", "7", "6", "5", "4", "3", "2", "1"];
const fileRow = "grid grid-cols-8 font-serif text-xs text-ink-soft";
const rankCol = "grid grid-rows-8 font-serif text-xs text-ink-soft";

export default function BoardLabB() {
  return (
    <LabFrame
      surface="board"
      variant="b"
      title="Book hatching"
      rationale="Dark squares are diagonal ink hatching at low opacity over board-light, like a 19th-century book diagram, inside a 1px ink rule. Coordinates are set in Literata around all four sides, as in old books. The marks are the product pen (Task 12)."
    >
      <main className="px-(--space-gutter) py-8">
        <div className="mx-auto grid w-fit grid-cols-[16px_auto_16px] grid-rows-[16px_auto_16px]">
          <span />
          <ol aria-hidden="true" className={fileRow}>
            {files.map((f) => (
              <li key={f} className="flex items-end justify-center">
                {f}
              </li>
            ))}
          </ol>
          <span />
          <ol aria-hidden="true" className={rankCol}>
            {ranks.map((r) => (
              <li key={r} className="flex items-center justify-center">
                {r}
              </li>
            ))}
          </ol>
          <div
            data-lab-board
            data-ply={blunder.ply}
            data-squares="hatched"
            data-coords="outside-serif"
            data-stroke="tapered"
            className="relative box-content border border-ink"
            style={{ width: "calc(var(--board-size) - 32px)" }}
          >
            <Chessboard
              options={{
                id: "lab-b",
                position: blunder.fenAfter,
                pieces,
                showNotation: false,
                allowDragging: false,
                allowDrawingArrows: false,
                showAnimations: false,
                lightSquareStyle: { backgroundColor: "var(--color-board-light)" },
                darkSquareStyle: {
                  backgroundColor: "var(--color-board-light)",
                  backgroundImage:
                    "repeating-linear-gradient(135deg, color-mix(in srgb, var(--color-ink) 42%, transparent) 0 1px, transparent 1px 4px)",
                },
                squareStyles: {
                  [from]: { boxShadow: "inset 0 0 0 100vmax var(--color-board-lastmove)" },
                  [to]: { boxShadow: "inset 0 0 0 100vmax var(--color-board-lastmove)" },
                },
              }}
            />
            <div data-lab-marks className="pointer-events-none absolute inset-0">
              <AnnotationLayer annotations={marks} orientation="white" animate={false} />
            </div>
          </div>
          <ol aria-hidden="true" className={rankCol}>
            {ranks.map((r) => (
              <li key={r} className="flex items-center justify-center">
                {r}
              </li>
            ))}
          </ol>
          <span />
          <ol aria-hidden="true" className={fileRow}>
            {files.map((f) => (
              <li key={f} className="flex items-start justify-center">
                {f}
              </li>
            ))}
          </ol>
          <span />
        </div>
        <p className="mt-4 text-center font-serif text-sm text-ink-soft">
          After {Math.ceil(blunder.ply / 2)}. {blunder.san}?? (blunder)
        </p>
      </main>
    </LabFrame>
  );
}
